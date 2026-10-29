// Actual unchanged upstream class cases, with explicit dependency failures.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync, spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {checkBaselineUpdate} from './test262-regression-policy.mjs';

const [engineArg, corpusArg, outputArg] = process.argv.slice(2);
assert.equal(process.argv.length, 5, 'usage: node scripts/test-base-class-originals.mjs CLI CORPUS NEW_OUTPUT');
const engine = path.resolve(engineArg), corpus = path.resolve(corpusArg), output = path.resolve(outputArg);
const scripts = path.dirname(fileURLToPath(import.meta.url)), repo = path.resolve(scripts, '..');
const fixture = path.join(repo, 'tests/fixtures/test262-base-classes');
const selectionBytes = fs.readFileSync(path.join(fixture, 'selection.json'));
const expectationBytes = fs.readFileSync(path.join(fixture, 'expectations.json'));
const expected = JSON.parse(expectationBytes), selected = JSON.parse(selectionBytes);
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const canonical = name => typeof name === 'string' && /^[a-zA-Z0-9_./-]+$/.test(name) &&
  !path.isAbsolute(name) && !name.split('/').some(part => !part || part === '.' || part === '..');
assert.equal(expected.schema, 1); assert.match(expected.corpus_revision, /^[a-f0-9]{40}$/);
assert.equal(sha(selectionBytes), expected.selection_sha256);
assert.equal(selected.length, expected.source_capture.files);
assert.deepEqual(selected, [...new Set(selected)].sort());
assert(selected.every(canonical));
let previous = null;
if (process.env.CLASS_FIXTURE_BASE_SHA) {
  const base = process.env.CLASS_FIXTURE_BASE_SHA; assert.match(base, /^[a-f0-9]{40}$/);
  execFileSync('git', ['fetch', '--no-tags', '--depth=1', 'origin', base], {cwd: repo, stdio: 'pipe'});
  const result = spawnSync('git', ['show', base + ':tests/fixtures/test262-base-classes/expectations.json'],
    {cwd: repo, encoding: 'utf8'});
  assert(!result.error && [0, 128].includes(result.status));
  if (result.status === 0) previous = JSON.parse(result.stdout);
}
checkBaselineUpdate(previous, expected);
assert(!fs.existsSync(output), 'output must be new'); fs.mkdirSync(output, {recursive: true});
fs.writeFileSync(path.join(output, 'selection.json'), selectionBytes);
fs.writeFileSync(path.join(output, 'expectations.json'), expectationBytes);
if (previous) fs.writeFileSync(path.join(output, 'previous-expectations.json'), JSON.stringify(previous, null, 2));
const identities = new Map();
for (const row of expected.sources) {
  assert(canonical(row.file) && !identities.has(row.file)); assert.match(row.sha256, /^[a-f0-9]{64}$/);
  identities.set(row.file, row.sha256);
  const original = execFileSync('git', ['-C', corpus, 'show', expected.corpus_revision + ':' + row.file], {maxBuffer: 8 * 1024 * 1024});
  assert.equal(sha(original), row.sha256, row.file);
  assert.deepEqual(fs.readFileSync(path.join(corpus, row.file)), original, 'working copy differs from pinned original: ' + row.file);
  const destination = path.join(output, 'originals', row.file);
  fs.mkdirSync(path.dirname(destination), {recursive: true}); fs.writeFileSync(destination, original);
}
assert(selected.every(name => identities.has(name)));
const key = row => JSON.stringify([row.path, row.variant]), known = new Map();
for (const row of expected.known_failures) {
  assert(selected.includes(row.path) && !known.has(key(row)));
  assert(Number.isInteger(row.issue) && row.issue > 0 && row.reason.trim());
  assert(row.observed_failure && typeof row.diagnostic === 'string'); known.set(key(row), row);
}
function invoke(name, args) {
  const result = spawnSync(process.execPath, [path.join(scripts, name), ...args],
    {encoding: 'utf8', timeout: 10 * 60 * 1000, maxBuffer: 16 * 1024 * 1024});
  const stem = path.join(output, args.includes('--backend') ? args[args.indexOf('--backend') + 1] + '-capture' : path.basename(args[0]) + '-verification');
  fs.writeFileSync(stem + '.stdout', result.stdout ?? ''); fs.writeFileSync(stem + '.stderr', result.stderr ?? '');
  fs.writeFileSync(stem + '.exit.json', JSON.stringify({status: result.status, signal: result.signal, error: result.error?.message ?? null}));
  assert.equal(result.status, 0, result.stderr); return result.stdout;
}
const result = {schema: 1, kind: 'test262-base-class-original-integration', corpus_revision: expected.corpus_revision,
  selection_sha256: sha(selectionBytes), engine_sha256: sha(fs.readFileSync(engine)), modes: [],
  scope: 'Known-pass preservation and explicitly recorded dependency failures in the fixed selection; not full class or corpus conformance.'};
let engineInfo = process.env.TEST262_ENGINE_INFO
  ? JSON.parse(fs.readFileSync(path.resolve(process.env.TEST262_ENGINE_INFO))) : null;
if (!engineInfo && process.env.CLASS_TEST_RUST_TOOLCHAIN) {
  const toolchain = process.env.CLASS_TEST_RUST_TOOLCHAIN;
  assert(/^(stable|[0-9]+\.[0-9]+\.[0-9]+)$/.test(toolchain));
  const git = args => execFileSync('git', args, {cwd: repo, encoding: 'utf8'}).trim();
  engineInfo = {name: 'CI Test262 CLI', source_revision: git(['rev-parse', 'HEAD']),
    source_tree: git(['rev-parse', 'HEAD^{tree}']), source_clean: git(['status', '--porcelain']) === '',
    rustc: execFileSync('rustc', ['+' + toolchain, '-vV'], {encoding: 'utf8'}).trim(),
    profile: 'test/debug', features: ['default', 'jit'], cargo_args: ['test', '--locked', '--all-features'],
    rustflags: process.env.RUSTFLAGS ?? null, executable_sha256: result.engine_sha256};
}
if (engineInfo) {
  assert.equal(engineInfo.executable_sha256, result.engine_sha256);
  fs.writeFileSync(path.join(output, 'engine-info.json'), JSON.stringify(engineInfo, null, 2) + '\n');
}
for (const backend of ['ast', 'auto']) {
  const capture = path.join(output, backend);
  const buildInfo = engineInfo ? ['--engine-info', path.join(output, 'engine-info.json')] : [];
  invoke('test262-baseline.mjs', ['--engine', engine, '--corpus', corpus, '--pinned', '--output', capture,
    '--universe', path.join(output, 'selection.json'), '--backend', backend, '--timeout-ms', '1000',
    '--memory-mb', '512', '--stack-kb', '8192', '--batch-size', '8', ...buildInfo]);
  const verification = JSON.parse(invoke('verify-test262-baseline.mjs', [capture]));
  assert.equal(verification.variants, expected.source_capture.variants);
  const read = file => JSON.parse(fs.readFileSync(path.join(capture, file)));
  const baseline = read('baseline.json'), outcomes = read(baseline.outcomes.file), observations = new Map();
  for (const raw of baseline.raw_reports.filter(row => row.stage === 'execution'))
    for (const row of read(raw.stdout).results) {assert(!observations.has(key(row))); observations.set(key(row), row);}
  const unexpected = [], remaining = [], improved = [];
  for (const row of outcomes) {
    const rule = known.get(key(row));
    if (row.outcome === 'pass') { if (rule) improved.push({path: row.path, variant: row.variant, issue: rule.issue}); continue; }
    if (row.outcome !== 'fail' || !rule) { unexpected.push(row); continue; }
    const observed = observations.get(key(row));
    if (JSON.stringify(observed?.observed_failure) !== JSON.stringify(rule.observed_failure) || observed.diagnostic !== rule.diagnostic)
      unexpected.push({...row, reason: 'known dependency failure changed', observed});
    else remaining.push({...row, issue: rule.issue, reason: rule.reason});
  }
  result.modes.push({backend, verification, remaining_dependency_failures: remaining, improvements: improved, unexpected});
}
for (const [file, hash] of identities) assert.equal(sha(fs.readFileSync(path.join(corpus, file))), hash, 'source changed during capture');
assert.equal(sha(fs.readFileSync(engine)), result.engine_sha256, 'engine changed during capture');
result.accepted = result.modes.every(mode => mode.unexpected.length === 0);
fs.writeFileSync(path.join(output, 'result.json'), JSON.stringify(result, null, 2) + '\n');
console.log(JSON.stringify({accepted: result.accepted, modes: result.modes.map(mode => ({backend: mode.backend,
  totals: mode.verification.totals, dependency_failures: mode.remaining_dependency_failures.length,
  improvements: mode.improvements.length, unexpected: mode.unexpected.length})), scope: result.scope}));
process.exitCode = result.accepted ? 0 : 1;
