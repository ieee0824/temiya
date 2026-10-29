// Real pinned CLI capture plus explicit collector-fault injection on a copy.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const [engine, corpus, output] = process.argv.slice(2);
if (process.argv.length !== 5) throw Error('usage: node scripts/test-test262-recovery.mjs CLI CORPUS NEW_OUTPUT');
assert(!fs.existsSync(output)); fs.mkdirSync(output, {recursive: true});
const scripts = path.dirname(fileURLToPath(import.meta.url));
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const read = file => JSON.parse(fs.readFileSync(file));
function run(script, args, expected = 0) {
  const result = spawnSync(process.execPath, [path.join(scripts, script), ...args],
    {encoding: 'utf8', timeout: 120000, maxBuffer: 16 * 1024 * 1024});
  fs.writeFileSync(path.join(output, 'commands.jsonl'), JSON.stringify({script, args,
    status: result.status, stdout: result.stdout, stderr: result.stderr}) + '\n', {flag: 'a'});
  if (expected === 'failure') assert.notEqual(result.status, 0, 'invalid recovery unexpectedly passed');
  else assert.equal(result.status, expected, result.stderr + result.stdout);
  return result;
}
const selected = ['test/built-ins/Array/prototype/pop/S15.4.4.6_A1.1_T1.js',
  'test/built-ins/String/prototype/at/returns-item.js', 'test/built-ins/String/prototype/charAt/pos-coerce-err.js'];
const list = path.join(output, 'selection.json'); fs.writeFileSync(list, JSON.stringify(selected));
const pristine = path.join(output, 'pristine');
run('test262-baseline.mjs', ['--engine', engine, '--corpus', corpus, '--pinned', '--output', pristine,
  '--universe', list, '--batch-size', '2', '--timeout-ms', '1000']);
run('verify-test262-baseline.mjs', [pristine]);
const before = read(pristine + '/baseline.json'); assert.equal(before.totals.pass, 6);
const lost = path.join(output, 'synthetic-collector-loss'); fs.cpSync(pristine, lost, {recursive: true});
const broken = read(lost + '/baseline.json');
const raw = broken.raw_reports.find(row => row.stage === 'execution');
assert.equal(raw.paths.length, 2);
const missingPaths = new Set(raw.paths);
fs.writeFileSync(path.join(lost, raw.stdout), '');
raw.stdout_sha256 = sha(''); raw.exit_code = null; raw.signal = 'SIGTERM'; raw.error = 'synthetic collector interruption on copied capture';
const oldRows = read(lost + '/outcomes.json');
const rows = oldRows.map(row => missingPaths.has(row.path)
  ? {path: row.path, variant: row.variant, outcome: 'unreported'} : row);
const rowBytes = Buffer.from(JSON.stringify(rows, null, 2) + '\n');
fs.writeFileSync(lost + '/outcomes.json', rowBytes); broken.outcomes.sha256 = sha(rowBytes);
const catalog = read(lost + '/catalog.json');
function recount(group, members) {
  for (const status of ['total', 'pass', 'fail', 'skip', 'timeout', 'not_run', 'unreported']) group[status] = 0;
  for (const row of rows) if (!members || members.has(row.path)) {group.total++; group[row.outcome]++;}
  group.reported_attempts = group.pass + group.fail + group.timeout;
  group.pass_rate_reported_attempts = group.reported_attempts ? group.pass / group.reported_attempts : null;
}
recount(broken.totals);
for (const group of [...broken.features, broken.untagged]) {
  const members = new Set(catalog.filter(test => group.feature === null ? !test.features.length
    : test.features.includes(group.feature)).map(test => test.path));
  recount(group, members);
}
broken.capture_errors = [{stage: 'execution', paths: raw.paths, raw_report: raw.stdout.slice(0, -5),
  detail: 'Synthetic transport loss: actual pristine results are retained separately.'}];
broken.scope += ' This copy contains a labelled synthetic collector fault for recovery integration testing.';
fs.writeFileSync(lost + '/baseline.json', JSON.stringify(broken, null, 2) + '\n');
const progress = read(lost + '/progress.json'); progress.totals = broken.totals; progress.capture_errors = 1;
fs.writeFileSync(lost + '/progress.json', JSON.stringify(progress));
assert.equal(broken.totals.pass, 2); assert.equal(broken.totals.unreported, 4);
run('verify-test262-baseline.mjs', [lost], 'failure');
const originalHash = sha(fs.readFileSync(lost + '/baseline.json'));
const recovered = path.join(output, 'recovered');
run('recover-test262-baseline.mjs', [lost, engine, corpus, recovered]);
assert.equal(sha(fs.readFileSync(lost + '/baseline.json')), originalHash, 'source capture was changed');
const after = read(recovered + '/baseline.json');
assert.equal(after.totals.pass, 6); assert.equal(after.totals.unreported, 0);
assert.equal(after.recovered_capture_errors.length, 1);
assert.equal(fs.statSync(path.join(recovered, raw.stdout)).size, 0, 'failed raw attempt was overwritten');
for (const row of rows.filter(row => row.outcome !== 'unreported'))
  assert.deepEqual(read(recovered + '/outcomes.json').find(next => next.path === row.path && next.variant === row.variant), row);
run('recover-test262-baseline.mjs', [lost, engine, corpus, recovered], 'failure');
run('recover-test262-baseline.mjs', [lost, process.execPath, corpus, path.join(output, 'wrong-engine')], 'failure');
assert(!fs.existsSync(path.join(output, 'wrong-engine')));
const attemptFile = recovered + '/' + after.recovery.attempt.file;
const attemptBytes = fs.readFileSync(attemptFile), baselineBytes = fs.readFileSync(recovered + '/baseline.json');
const tampered = JSON.parse(attemptBytes);
tampered.expected_missing.push({path: selected[2], variant: 'Sloppy'});
fs.writeFileSync(attemptFile, JSON.stringify(tampered));
after.recovery.attempt.sha256 = sha(fs.readFileSync(attemptFile));
fs.writeFileSync(recovered + '/baseline.json', JSON.stringify(after));
run('verify-test262-baseline.mjs', [recovered], 'failure');
fs.writeFileSync(attemptFile, attemptBytes); fs.writeFileSync(recovered + '/baseline.json', baselineBytes);
run('verify-test262-baseline.mjs', [recovered]);
console.log(JSON.stringify({integration: 'pass', original_variants: 6, recollected_unreported_variants: 4,
  preserved_known_variants: 2, checks: ['failed raw retained', 'source unchanged', 'known outcomes unchanged',
    'overwrite refused', 'different binary refused', 'tampered recovery scope rejected']}));
