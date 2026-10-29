// Actual native workers only. Unknown/timeout/unsupported is never an equal pass.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const options = {mode: 'interpreter', backend: 'auto', 'timeout-ms': '1000'};
const names = new Set(['temiya', 'spidermonkey', 'spidermonkey-info', 'cases', 'known', 'output', 'mode', 'backend', 'timeout-ms']);
for (let i = 2; i < process.argv.length; i += 2) {
  const key = process.argv[i].replace(/^--/, '');
  assert(process.argv[i].startsWith('--') && names.has(key) && process.argv[i + 1], 'invalid option');
  options[key] = process.argv[i + 1];
}
for (const key of ['temiya', 'spidermonkey', 'spidermonkey-info', 'cases', 'output'])
  assert(options[key], '--' + key + ' required');
assert(['interpreter', 'default'].includes(options.mode));
assert(['ast', 'auto', 'bytecode'].includes(options.backend));
const timeout = Number(options['timeout-ms']);
assert(Number.isInteger(timeout) && timeout >= 1 && timeout <= 120000);
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const read = file => JSON.parse(fs.readFileSync(file));
const stable = value => JSON.stringify(value, (_, item) => item && typeof item === 'object' && !Array.isArray(item)
  ? Object.fromEntries(Object.entries(item).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)) : item);
const target = read(new URL('../benchmarks/targets/spidermonkey.json', import.meta.url));
const smInfo = read(options['spidermonkey-info']);
const sm = path.resolve(options.spidermonkey), temiya = path.resolve(options.temiya);
assert.equal(smInfo.target.revision, target.revision);
assert.equal(smInfo.source_clean, true);
assert.equal(smInfo.binary_sha256, sha(fs.readFileSync(sm)), 'shell build-info binary mismatch');
const cases = read(options.cases), known = options.known ? read(options.known) : [];
assert(Array.isArray(cases) && cases.length > 0 && Array.isArray(known));
const ids = new Set();
for (const row of cases) {
  assert(/^[a-zA-Z0-9_-]+$/.test(row.id) && !ids.has(row.id) && typeof row.source === 'string');
  ids.add(row.id);
}
const knownIds = new Set();
for (const row of known) {
  assert(typeof row.id === 'string' && row.id && !knownIds.has(row.id));
  assert(/^[a-f0-9]{64}$/.test(row.source_sha256) && /^[a-f0-9]{64}$/.test(row.observation_sha256));
  assert(typeof row.reason === 'string' && row.reason.trim() && typeof row.issue === 'string' && row.issue.trim());
  assert(typeof row.reproducer === 'string' && !path.isAbsolute(row.reproducer) &&
    !row.reproducer.split(/[\\/]/).some(part => !part || part === '.' || part === '..'), 'canonical reproducer path required');
  assert.equal(sha(fs.readFileSync(path.join(path.dirname(fileURLToPath(import.meta.url)), '..', row.reproducer))),
    row.source_sha256, 'known reproducer must match the exact source identity');
  knownIds.add(row.id);
}
const output = path.resolve(options.output);
assert(!fs.existsSync(output), 'output must be new');
fs.mkdirSync(output, {recursive: true});
fs.copyFileSync(options.cases, path.join(output, 'cases.json'));
const worker = fileURLToPath(new URL('./spidermonkey-differential-worker.js', import.meta.url));
function invoke(executable, args, input, stem) {
  const result = spawnSync(executable, args, {input, detached: process.platform !== 'win32',
    timeout, killSignal: 'SIGKILL', maxBuffer: 8 * 1024 * 1024});
  if (result.pid && process.platform !== 'win32') {
    try { process.kill(-result.pid, 'SIGKILL'); } catch (error) { if (error.code !== 'ESRCH') throw error; }
  }
  const stdout = result.stdout ?? Buffer.alloc(0), stderr = result.stderr ?? Buffer.alloc(0);
  fs.writeFileSync(path.join(output, stem + '.stdout'), stdout);
  fs.writeFileSync(path.join(output, stem + '.stderr'), stderr);
  const record = {exit_code: result.status, signal: result.signal, error: result.error?.message ?? null,
    stdout: stem + '.stdout', stdout_sha256: sha(stdout), stderr: stem + '.stderr', stderr_sha256: sha(stderr)};
  if (result.status !== 0 || result.error || result.signal) return {record, observation: null};
  try {
    const text = stdout.toString('utf8');
    assert(Buffer.from(text).equals(stdout), 'stdout is not valid UTF-8');
    const report = JSON.parse(text);
    assert.equal(report.schema, 1); assert.equal(report.kind, 'differential-observation');
    assert.equal(typeof report.comparison_eligible, 'boolean');
    for (const name of ['effects', 'jobs', 'job_errors', 'events']) assert(Array.isArray(report[name]));
    assert(report.completion && ['normal', 'throw', 'compile-error'].includes(report.completion.kind));
    if (executable === sm) {
      assert.equal(report.engine, 'SpiderMonkey'); assert.equal(report.mode, options.mode);
      if (options.mode === 'interpreter')
        for (const key of ['ion.enable', 'baseline.enable', 'blinterp.enable']) assert.equal(report.jit_options[key], 0);
    } else { assert.equal(report.engine, 'Temiya'); assert.equal(report.backend, {auto: 'Auto', ast: 'Ast', bytecode: 'Bytecode'}[options.backend]); }
    return {record, observation: report};
  } catch (error) { return {record: {...record, protocol_error: error.message}, observation: null}; }
}
const results = [];
const projected = report => ({completion: report.completion, effects: report.effects,
  jobs: report.jobs, job_errors: report.job_errors, events: report.events});
for (const row of cases) {
  const directory = path.join(output, row.id); fs.mkdirSync(directory);
  const input = JSON.stringify({source: row.source, backend: options.backend});
  const request = path.join(directory, 'request.json');
  fs.writeFileSync(request, input); fs.writeFileSync(path.join(directory, 'reproducer.js'), row.source);
  const a = invoke(temiya, [], input, row.id + '/temiya');
  const b = invoke(sm, [...target.modes[options.mode], worker, request, options.mode], undefined, row.id + '/spidermonkey');
  let classification = 'infrastructure-failure', signature = null, knownId = null;
  if (a.observation && b.observation) {
    classification = 'unsupported-observation';
    if (a.observation.comparison_eligible && b.observation.comparison_eligible) {
      const observations = [projected(a.observation), projected(b.observation)];
      signature = sha(stable(observations));
      classification = stable(observations[0]) === stable(observations[1]) ? 'match' : 'mismatch';
      if (classification === 'mismatch') {
        const rule = known.find(item => item.source_sha256 === sha(row.source) && item.observation_sha256 === signature);
        if (rule) { classification = 'known-difference'; knownId = rule.id; }
      }
    }
  }
  results.push({id: row.id, source_sha256: sha(row.source), provenance: row.provenance ?? null, classification,
    observation_sha256: signature, known_id: knownId, temiya: a, spidermonkey: b});
}
const counts = {};
for (const row of results) counts[row.classification] = (counts[row.classification] ?? 0) + 1;
const report = {schema: 1, kind: 'native-engine-differential', target, mode: options.mode,
  backend: options.backend, timeout_ms: timeout, cases_sha256: sha(fs.readFileSync(options.cases)), temiya_sha256: sha(fs.readFileSync(temiya)),
  spidermonkey_sha256: smInfo.binary_sha256, provided_spidermonkey_build_info: smInfo,
  controller_sha256: sha(fs.readFileSync(fileURLToPath(import.meta.url))), worker_sha256: sha(fs.readFileSync(worker)),
  host: {platform: os.platform(), architecture: os.arch(), kernel: os.release(), node: process.version},
  counts, results, scope: 'Actual recorded observations; no timing or full language conformance qualification.'};
fs.writeFileSync(path.join(output, 'report.json'), JSON.stringify(report, null, 2) + '\n');
console.log(JSON.stringify({output, counts}));
if (results.some(row => !['match', 'known-difference'].includes(row.classification))) process.exitCode = 1;
