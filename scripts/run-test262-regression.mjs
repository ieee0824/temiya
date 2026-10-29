// One PR run or one full-corpus shard; preserve captures even when the gate fails.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {checkBaselineUpdate, validateSkipReasons} from './test262-regression-policy.mjs';

const [engine, corpus, reference, policyFile, output, mode, shardText, shardsText, previousFile]
  = process.argv.slice(2);
if (process.argv.length < 10 || process.argv.length > 11)
  throw Error('usage: node scripts/run-test262-regression.mjs ENGINE CORPUS REFERENCE POLICY NEW_OUTPUT pr|full SHARD SHARDS [PREVIOUS_POLICY]');
assert(['pr', 'full'].includes(mode), 'invalid execution mode');
const read = file => JSON.parse(fs.readFileSync(file));
checkBaselineUpdate(previousFile ? read(previousFile) : null, read(policyFile));
validateSkipReasons(read(policyFile).skip_reasons);
const root = path.resolve(output);
assert(!fs.existsSync(root), 'output must be new');
fs.mkdirSync(root, {recursive: true});
const scripts = path.dirname(fileURLToPath(import.meta.url));
function invoke(name, args, timeout) {
  const result = spawnSync(process.execPath, [path.join(scripts, name), ...args],
    {encoding: 'utf8', timeout, maxBuffer: 32 * 1024 * 1024});
  fs.writeFileSync(path.join(root, name + '.stdout'), result.stdout ?? '');
  fs.writeFileSync(path.join(root, name + '.stderr'), result.stderr ?? '');
  fs.writeFileSync(path.join(root, name + '.exit.json'), JSON.stringify({status: result.status,
    signal: result.signal, error: result.error?.message ?? null}));
  return result;
}
const selectedFile = path.join(root, 'selection.json');
const prepared = invoke('prepare-test262-regression-selection.mjs',
  [reference, policyFile, mode, shardText, shardsText, selectedFile], 120000);
assert.equal(prepared.status, 0, prepared.stderr);
const conditions = read(path.join(reference, 'conditions.json'));
const candidate = path.join(root, 'capture');
// Match actual baseline budgets; a slower case never receives a larger timeout.
const capture = invoke('test262-baseline.mjs', ['--engine', path.resolve(engine),
  '--corpus', path.resolve(corpus), '--pinned', '--manifest', path.join(reference, 'manifest.json'),
  '--output', candidate, '--universe', selectedFile, '--backend', conditions.backend_requested,
  '--timeout-ms', String(conditions.timeout_ms), '--memory-mb', String(conditions.memory_mb),
  '--stack-kb', String(conditions.stack_kb), '--batch-size', String(conditions.batch_size)],
  mode === 'pr' ? 20 * 60 * 1000 : 5.5 * 60 * 60 * 1000);
// Capture errors still reach the independent verifier, which rejects missing data.
const args = mode === 'full' ? ['--full', '--shard', shardText + '/' + shardsText] : [];
args.push(reference, candidate, policyFile);
if (previousFile) args.push(previousFile);
const gate = invoke('check-test262-regressions.mjs', args, 120000);
console.log(gate.stdout ?? '');
if (gate.stderr) console.error(gate.stderr);
process.exitCode = capture.status === 0 && gate.status === 0 ? 0 : 1;
