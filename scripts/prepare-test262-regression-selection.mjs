import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {selectPaths, checkBaselineUpdate} from './test262-regression-policy.mjs';

const [reference, policyFile, mode, shardText, shardsText, destination] = process.argv.slice(2);
if (process.argv.length !== 8)
  throw Error('usage: node scripts/prepare-test262-regression-selection.mjs REFERENCE_DIR POLICY_JSON pr|full SHARD SHARDS NEW_SELECTION_JSON');
assert(['pr', 'full'].includes(mode), 'invalid execution mode');
assert(!fs.existsSync(destination), 'selection destination must be new');
const policy = JSON.parse(fs.readFileSync(policyFile));
checkBaselineUpdate(null, policy);
const bytes = fs.readFileSync(path.join(reference, 'baseline.json'));
assert.equal(createHash('sha256').update(bytes).digest('hex'), policy.reference_baseline_sha256);
const universe = JSON.parse(fs.readFileSync(path.join(reference, 'universe.json')));
const selection = selectPaths(policy, universe,
  {full: mode === 'full', shard: Number(shardText), shards: Number(shardsText)});
fs.writeFileSync(destination, JSON.stringify(selection, null, 2) + '\n', {flag: 'wx'});
console.log(JSON.stringify({mode, shard: Number(shardText), shards: Number(shardsText), files: selection.length}));
