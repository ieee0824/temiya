// Re-collect only wholly unreported files, retaining the original capture intact.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {spawnSync, execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const [sourceArg, engineArg, corpusArg, outputArg] = process.argv.slice(2);
if (process.argv.length !== 6)
  throw Error('usage: node scripts/recover-test262-baseline.mjs COMPLETED_CAPTURE SAME_ENGINE CORPUS NEW_OUTPUT');
const source = path.resolve(sourceArg), output = path.resolve(outputArg), engine = path.resolve(engineArg);
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const read = name => JSON.parse(fs.readFileSync(path.join(source, name)));
const baselineBytes = fs.readFileSync(path.join(source, 'baseline.json'));
const before = JSON.parse(baselineBytes), conditions = read('conditions.json');
assert.equal(read('progress.json').stage, 'completed', 'producer must finish first');
assert(!before.recovery && !fs.existsSync(path.join(source, 'recovery/attempt.json')), 'inspect existing recovery evidence before any further attempt');
assert.equal(before.schema, 1); assert.equal(before.kind, 'test262-feature-baseline');
assert.deepEqual(before.conditions, conditions); assert.equal(conditions.engine_frozen_during_run, true);
assert.match(conditions.corpus_revision, /^[a-f0-9]{40}$/);
assert.equal(sha(fs.readFileSync(engine)), conditions.engine_sha256, 'recovery must use exactly the original binary');
assert.equal(before.universe.metadata_error_files, 0); assert.equal(before.universe.variant_count_exact, true);
assert.equal(before.totals.not_run, 0); assert(before.totals.unreported > 0);
assert(before.capture_errors.length > 0 && before.capture_errors.every(error => error.stage === 'execution'));
assert.deepEqual(before.file_setup_failures, [], 'setup failures require separate review');
const catalog = read(before.catalog.file), oldOutcomeBytes = fs.readFileSync(path.join(source, before.outcomes.file));
const oldRows = JSON.parse(oldOutcomeBytes), selected = read('selection.json');
assert.equal(sha(oldOutcomeBytes), before.outcomes.sha256);
assert.equal(sha(fs.readFileSync(path.join(source, before.catalog.file))), before.catalog.sha256);
assert.deepEqual(selected, read('universe.json'), 'recovery requires a completely selected universe');
const key = row => JSON.stringify([row.path, row.variant]);
const missing = oldRows.filter(row => row.outcome === 'unreported');
const missingIds = new Set(missing.map(key));
assert.equal(missingIds.size, before.totals.unreported);
const paths = [...new Set(missing.map(row => row.path))].sort();
const indexed = new Map(catalog.map(test => [test.path, test]));
assert.deepEqual([...new Set(before.capture_errors.flatMap(error => error.paths))].sort(), paths);
for (const name of paths) {
  const test = indexed.get(name); assert.equal(test.parsed, true);
  for (const variant of test.variants) assert(missingIds.has(key({path: name, variant})), 'never re-run a file with an already reported variant');
}
const optional = name => {try {return fs.readFileSync(name, 'utf8').trim();} catch {return null;}};
const host = {platform: os.platform(), architecture: os.arch(), kernel: os.release(),
  cpu_models: [...new Set(os.cpus().map(cpu => cpu.model))].sort(), node: process.version,
  cpu_allowed_list: optional('/proc/self/status')?.match(/^Cpus_allowed_list:\s*(.+)$/m)?.[1] ?? null,
  cgroup_cpu_max: optional('/sys/fs/cgroup/cpu.max'), cgroup_memory_max: optional('/sys/fs/cgroup/memory.max'),
  locale_environment: {TZ: process.env.TZ ?? null, LANG: process.env.LANG ?? null, LC_ALL: process.env.LC_ALL ?? null}};
assert.deepEqual(host, conditions.host, 'retain the original host/affinity/locale conditions');
assert(!fs.existsSync(output), 'output must be new');
fs.cpSync(source, output, {recursive: true});
fs.mkdirSync(path.join(output, 'history'));
fs.mkdirSync(path.join(output, 'recovery/raw'), {recursive: true});
fs.writeFileSync(path.join(output, 'history/initial-baseline.json'), baselineBytes);
fs.writeFileSync(path.join(output, 'history/initial-outcomes.json'), oldOutcomeBytes);
fs.copyFileSync(path.join(output, 'progress.json'), path.join(output, 'history/initial-progress.json'));
const collectorBytes = fs.readFileSync(fileURLToPath(import.meta.url));
fs.writeFileSync(path.join(output, 'recovery/collector.mjs'), collectorBytes);
const recoveryConditions = {...conditions, batch_size: 1, host, collector_sha256: sha(collectorBytes)};
fs.writeFileSync(path.join(output, 'recovery/conditions.json'), JSON.stringify(recoveryConditions, null, 2) + '\n');
const attempt = {schema: 1, kind: 'test262-execution-recovery', started_at: new Date().toISOString(),
  conditions: recoveryConditions, expected_missing: missing.map(({path, variant}) => ({path, variant})),
  selected_paths: paths, raw_reports: [], capture_errors: [],
  reason: 'Only missing reports are recollected. Original pass/fail/skip/timeout outcomes and all failed raw attempts remain unchanged.'};
const recovered = new Map();
const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'temiya-recovery-engine-'));
const frozen = path.join(temporary, 'temiya-test262');
fs.copyFileSync(engine, frozen); fs.chmodSync(frozen, 0o700);
assert.equal(sha(fs.readFileSync(frozen)), conditions.engine_sha256);
try {
  for (const [number, name] of paths.entries()) {
    const stem = 'recovery/raw/execution-' + String(number).padStart(5, '0');
    const stdout = fs.openSync(path.join(output, stem + '.json'), 'wx');
    const stderr = fs.openSync(path.join(output, stem + '.stderr'), 'wx');
    const args = ['--corpus', path.resolve(corpusArg), '--manifest', path.join(output, 'manifest.json'),
      '--backend', conditions.backend_requested, '--timeout-ms', String(conditions.timeout_ms),
      '--memory-mb', String(conditions.memory_mb), '--stack-kb', String(conditions.stack_kb), '--pinned', '--', name];
    const timeout = Math.max(60000, indexed.get(name).variants.length * conditions.timeout_ms + 30000);
    const result = spawnSync(frozen, args, {stdio: ['ignore', stdout, stderr], detached: process.platform !== 'win32', timeout});
    fs.closeSync(stdout); fs.closeSync(stderr);
    if (result.pid && process.platform !== 'win32') {
      try {process.kill(-result.pid, 'SIGKILL');} catch (error) {if (error.code !== 'ESRCH') throw error;}
    }
    const bytes = fs.readFileSync(path.join(output, stem + '.json'));
    const raw = {stage: 'execution', paths: [name], stdout: stem + '.json', stderr: stem + '.stderr',
      stdout_sha256: sha(bytes), stderr_sha256: sha(fs.readFileSync(path.join(output, stem + '.stderr'))),
      exit_code: result.status, signal: result.signal, error: result.error?.message ?? null,
      recovery: true, collection_timeout_ms: timeout};
    attempt.raw_reports.push(raw);
    try {
      assert(!result.error && !result.signal && [0, 1, 3].includes(result.status));
      const report = JSON.parse(bytes);
      assert.equal(report.schema, 1); assert.equal(report.corpus_revision, conditions.corpus_revision);
      assert.equal(report.manifest_revision, conditions.manifest_revision);
      assert.equal(report.backend, {auto: 'Auto', ast: 'Ast', bytecode: 'Bytecode'}[conditions.backend_requested]);
      const observed = new Set(), counts = {pass: 0, fail: 0, skip: 0};
      for (const row of report.results) {
        assert.equal(row.path, name); assert(indexed.get(name).variants.includes(row.variant));
        assert(!observed.has(key(row))); observed.add(key(row)); assert(Object.hasOwn(counts, row.status)); counts[row.status]++;
        assert(row.timed_out !== true || row.status === 'fail');
      }
      assert.deepEqual([...observed].sort(), indexed.get(name).variants.map(variant => key({path: name, variant})).sort());
      assert.deepEqual(counts, report.counts); assert.equal(result.status, counts.fail ? 1 : counts.pass ? 0 : 3);
      for (const [index, row] of report.results.entries()) {
        assert(missingIds.has(key(row)) && !recovered.has(key(row)));
        recovered.set(key(row), {path: row.path, variant: row.variant,
          outcome: row.status === 'fail' && row.timed_out === true ? 'timeout' : row.status,
          raw_record: raw.stdout + '#/results/' + index});
      }
    } catch (error) {
      attempt.capture_errors.push({stage: 'execution', paths: [name], raw_report: stem, detail: error.message});
    }
    fs.writeFileSync(path.join(output, 'recovery/progress.json'), JSON.stringify({completed_files: number + 1,
      selected_files: paths.length, observed_variants: recovered.size, capture_errors: attempt.capture_errors.length}));
  }
} finally {fs.rmSync(temporary, {recursive: true, force: true});}
attempt.completed_at = new Date().toISOString();
fs.writeFileSync(path.join(output, 'recovery/attempt.json'), JSON.stringify(attempt, null, 2) + '\n');
if (attempt.capture_errors.length || recovered.size !== missingIds.size) {
  fs.writeFileSync(path.join(output, 'recovery/review-needed.json'), JSON.stringify({recovered: [...recovered.values()],
    still_unreported: missing.filter(row => !recovered.has(key(row))), capture_errors: attempt.capture_errors,
    instruction: 'Preserve and reuse all newly reported outcomes. Do not re-run them during further recovery.'}, null, 2));
  throw Error('recovery incomplete; original baseline and all attempts retained for review');
}
const rows = oldRows.map(row => recovered.get(key(row)) ?? row);
const statuses = ['pass', 'fail', 'skip', 'timeout', 'not_run', 'unreported'];
const summarize = members => {
  const result = {total: 0, pass: 0, fail: 0, skip: 0, timeout: 0, not_run: 0, unreported: 0};
  for (const row of rows) if (!members || members.has(row.path)) {
    assert(statuses.includes(row.outcome)); result.total++; result[row.outcome]++;
  }
  const attempts = result.pass + result.fail + result.timeout;
  return {...result, reported_attempts: attempts, pass_rate_reported_attempts: attempts ? result.pass / attempts : null};
};
const tags = new Map(), untagged = new Set();
for (const test of catalog) {
  assert.equal(test.parsed, true);
  if (!test.features.length) untagged.add(test.path);
  for (const tag of new Set(test.features)) {if (!tags.has(tag)) tags.set(tag, new Set()); tags.get(tag).add(test.path);}
}
const group = (feature, members) => ({feature, files: members.size, selected_files: members.size,
  setup_failure_files: 0, ...summarize(members)});
const updated = {...before, completed_at: attempt.completed_at,
  totals: summarize(), features: [...tags].sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0).map(([tag, members]) => group(tag, members)),
  untagged: group(null, untagged), capture_errors: [], recovered_capture_errors: before.capture_errors,
  raw_reports: [...before.raw_reports, ...attempt.raw_reports],
  recovery: {source_baseline: {file: 'history/initial-baseline.json', sha256: sha(baselineBytes)},
    source_outcomes: {file: 'history/initial-outcomes.json', sha256: sha(oldOutcomeBytes)},
    attempt: {file: 'recovery/attempt.json', sha256: sha(fs.readFileSync(path.join(output, 'recovery/attempt.json')))}},
  scope: before.scope + ' Lost CLI reports were recovered at identical binary/backend/per-variant bounds; original failed collection attempts remain in raw_reports and recovery history.'};
const newBytes = Buffer.from(JSON.stringify(rows, null, 2) + '\n');
fs.writeFileSync(path.join(output, before.outcomes.file), newBytes);
updated.outcomes = {...before.outcomes, sha256: sha(newBytes)};
assert.equal(updated.totals.unreported, 0); assert.equal(updated.totals.not_run, 0);
fs.writeFileSync(path.join(output, 'baseline.json'), JSON.stringify(updated, null, 2) + '\n');
fs.writeFileSync(path.join(output, 'progress.json'), JSON.stringify({...read('progress.json'),
  updated_at: attempt.completed_at, totals: updated.totals, capture_errors: 0,
  recovered_capture_errors: before.capture_errors.length}, null, 2) + '\n');
const verifier = fileURLToPath(new URL('./verify-test262-baseline.mjs', import.meta.url));
execFileSync(process.execPath, [verifier, output], {stdio: 'inherit'});
