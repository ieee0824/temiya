// Actual pinned Test262 -> Engine CLI -> capture verifier -> regression gate.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';

const [engine, corpus, output] = process.argv.slice(2);
if (process.argv.length !== 5)
  throw Error('usage: node scripts/test-test262-regression-gate.mjs ENGINE CORPUS NEW_OUTPUT');
const root = path.resolve(output);
assert(!fs.existsSync(root), 'integration output must be new');
fs.mkdirSync(root, {recursive: true});
const scripts = path.dirname(fileURLToPath(import.meta.url));
function run(script, args, status = 0) {
  const result = spawnSync(process.execPath, [path.join(scripts, script), ...args],
    {encoding: 'utf8', timeout: 120000, maxBuffer: 16 * 1024 * 1024});
  fs.writeFileSync(path.join(root, 'commands.jsonl'), JSON.stringify({script, args,
    status: result.status, stdout: result.stdout, stderr: result.stderr}) + '\n', {flag: 'a'});
  assert.equal(result.status, status, result.stderr + '\n' + result.stdout);
  return result;
}
const selection = [
  'test/built-ins/String/prototype/at/returns-item.js',
  'test/built-ins/String/prototype/charAt/pos-coerce-err.js',
];
const selectionFile = path.join(root, 'selection.json');
fs.writeFileSync(selectionFile, JSON.stringify(selection));
function capture(name, extra = [], selectedFile = selectionFile) {
  const directory = path.join(root, name);
  run('test262-baseline.mjs', ['--engine', path.resolve(engine), '--corpus', path.resolve(corpus),
    '--output', directory, '--pinned', '--universe', selectedFile,
    '--timeout-ms', '1000', ...extra]);
  return directory;
}
const reference = capture('reference');
const candidate = capture('candidate');
const baselineBytes = fs.readFileSync(path.join(reference, 'baseline.json'));
const baseline = JSON.parse(baselineBytes);
assert.equal(baseline.totals.pass, 4, 'all four original strict/sloppy variants must pass');
assert.equal(baseline.totals.total, 4);
const policyFile = path.join(root, 'policy.json');
run('create-test262-regression-policy.mjs', [reference, policyFile, '128',
  'Integration uses four actually passing pinned original variants']);
const policy = JSON.parse(fs.readFileSync(policyFile));
assert.deepEqual(policy.pr_selection, selection);
assert.equal(policy.selection_evidence.selected_variants, 4);
assert.deepEqual(policy.selection_evidence.uncovered_labels, []);
const gateArgs = [reference, candidate, policyFile];
const invalidPolicyFile = path.join(root, 'invalid-outside-skip-policy.json');
fs.writeFileSync(invalidPolicyFile, JSON.stringify({...policy,
  update: {reason: 'Integration rejects reasonless exceptions outside the PR selection'},
  skip_reasons: [{path: 'test/outside-pr.js', variant: 'Strict', reason: ''}]}));
assert.match(run('check-test262-regressions.mjs',
  [reference, candidate, invalidPolicyFile], 2).stderr, /nonempty reason/);
const accepted = JSON.parse(run('check-test262-regressions.mjs', gateArgs).stdout);
assert.equal(accepted.passed, true);
assert.equal(accepted.observed_variants, 4);
run('check-test262-regressions.mjs', ['--full', ...gateArgs]);
const runnerOutput = path.join(root, 'runner-pr');
run('run-test262-regression.mjs', [engine, corpus, reference, policyFile,
  runnerOutput, 'pr', '0', '1', policyFile]);
assert.equal(JSON.parse(fs.readFileSync(path.join(runnerOutput,
  'check-test262-regressions.mjs.stdout'))).passed, true);

const stalePolicy = {...policy, update: {reason: 'Integration checks stale skip diagnostics'},
  skip_reasons: [{path: selection[0], variant: 'Sloppy', reason: 'This case actually passes, so this exception is stale'}]};
const stalePolicyFile = path.join(root, 'stale-policy.json');
fs.writeFileSync(stalePolicyFile, JSON.stringify(stalePolicy));
const failedRunnerOutput = path.join(root, 'runner-rejected');
run('run-test262-regression.mjs', [engine, corpus, reference, stalePolicyFile,
  failedRunnerOutput, 'pr', '0', '1', policyFile], 1);
assert(fs.existsSync(path.join(failedRunnerOutput, 'capture', 'baseline.json')));
assert.equal(JSON.parse(fs.readFileSync(path.join(failedRunnerOutput,
  'check-test262-regressions.mjs.exit.json'))).status, 2);
assert.match(fs.readFileSync(path.join(failedRunnerOutput,
  'check-test262-regressions.mjs.stderr'), 'utf8'), /stale or out-of-scope/);
for (let shard = 0; shard < 2; shard++) {
  const selectedFile = path.join(root, 'shard-' + shard + '.json');
  run('prepare-test262-regression-selection.mjs',
    [reference, policyFile, 'full', String(shard), '2', selectedFile]);
  const directory = capture('shard-' + shard, [], selectedFile);
  run('check-test262-regressions.mjs',
    ['--full', '--shard', shard + '/2', reference, directory, policyFile]);
}

const changedBudget = capture('changed-budget', ['--memory-mb', '256']);
assert.match(run('check-test262-regressions.mjs', [reference, changedBudget, policyFile], 2).stderr,
  /incomparable condition: memory_mb/);

const candidateSummary = path.join(candidate, 'baseline.json');
const original = fs.readFileSync(candidateSummary);
const forged = JSON.parse(original);
forged.totals.pass = 999;
fs.writeFileSync(candidateSummary, JSON.stringify(forged));
assert.match(run('check-test262-regressions.mjs', gateArgs, 2).stderr, /capture integrity verification failed/);
fs.writeFileSync(candidateSummary, original);

const changedPolicyFile = path.join(root, 'changed-policy.json');
fs.writeFileSync(changedPolicyFile, JSON.stringify({...policy, pr_selection: selection.slice(0, 1)}));
assert.match(run('check-test262-regressions.mjs', [reference, candidate, changedPolicyFile, policyFile], 2).stderr,
  /fresh update reason/);
console.log(JSON.stringify({integration: 'pass', original_variants: 4,
  checks: ['pr pass', 'full pass', 'PR runner pass', 'failed runner preserves raw capture and diagnostics',
    'all full shards pass', 'budget mismatch rejected', 'forged count rejected',
    'baseline change without fresh reason rejected'], output: root}));
