// Compare independently verified real CLI captures, retaining per-variant failures.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {compareOutcomes, checkComparableConditions, checkBaselineUpdate, selectPaths, validateSkipReasons}
  from './test262-regression-policy.mjs';

try {
  const args = process.argv.slice(2);
  const full = args[0] === '--full';
  if (full) args.shift();
  let shard = 0, shards = 1;
  if (args[0] === '--shard') {
    args.shift();
    assert.match(args[0] ?? '', /^\d+\/\d+$/, 'shard must be INDEX/COUNT');
    [shard, shards] = args.shift().split('/').map(Number);
  }
  if (args.length < 3 || args.length > 4)
    throw Error('usage: node scripts/check-test262-regressions.mjs [--full [--shard INDEX/COUNT]] REFERENCE_DIR CANDIDATE_DIR POLICY_JSON [PREVIOUS_POLICY_JSON]');
  const [reference, candidate, policyFile, previousFile] = args;
  const read = file => JSON.parse(fs.readFileSync(file));
  const policy = read(policyFile);
  checkBaselineUpdate(previousFile ? read(previousFile) : null, policy);
  validateSkipReasons(policy.skip_reasons);
  const referenceBytes = fs.readFileSync(path.join(reference, 'baseline.json'));
  assert.equal(createHash('sha256').update(referenceBytes).digest('hex'),
    policy.reference_baseline_sha256, 'policy must identify the exact reference baseline');
  const verifier = fileURLToPath(new URL('./verify-test262-baseline.mjs', import.meta.url));
  for (const directory of [reference, candidate]) {
    const verification = spawnSync(process.execPath, [verifier, directory],
      {encoding: 'utf8', timeout: 120000, maxBuffer: 16 * 1024 * 1024});
    assert.equal(verification.status, 0,
      'capture integrity verification failed: ' + directory + '\n' + verification.stderr);
  }
  const before = JSON.parse(referenceBytes);
  const after = read(path.join(candidate, 'baseline.json'));
  checkComparableConditions(before.conditions, after.conditions);
  assert.equal(before.universe.metadata_error_files, 0, 'reference must index all expected variants');
  assert.equal(before.totals.not_run, 0, 'reference must execute the complete universe');
  assert.equal(after.universe.metadata_error_files, 0, 'candidate metadata errors');
  const expected = read(path.join(reference, before.outcomes.file));
  const actual = read(path.join(candidate, after.outcomes.file));
  assert(expected.some(row => row.outcome === 'pass'), 'reference has no known passes');
  const selection = selectPaths(policy, read(path.join(reference, 'universe.json')), {full, shard, shards});
  assert.deepEqual(read(path.join(candidate, 'selection.json')), [...selection].sort(),
    'candidate did not execute the required selection');
  // A small capture indexes only its small universe; a full capture covers every file.
  assert.deepEqual(read(path.join(candidate, 'universe.json')), [...selection].sort(),
    'candidate universe differs from required execution selection');
  const selected = new Set(selection);
  if (!full) {
    const successfulPaths = new Set(expected.filter(row => row.outcome === 'pass').map(row => row.path));
    assert(selection.every(name => successfulPaths.has(name)), 'PR selection must exercise known passes');
  }
  const exceptions = policy.skip_reasons.filter(row => selected.has(row.path));
  const result = compareOutcomes(expected, actual, selection, exceptions);
  console.log(JSON.stringify({schema: 1, kind: 'test262-regression-gate',
    mode: full ? 'full' : 'pr', shard, shards, reference_baseline_sha256: policy.reference_baseline_sha256,
    corpus_revision: before.conditions.corpus_revision,
    candidate_engine_sha256: after.conditions.engine_sha256, ...result}, null, 2));
  process.exitCode = result.passed ? 0 : 1;
} catch (error) {
  console.error(error.stack ?? String(error));
  process.exitCode = 2;
}
