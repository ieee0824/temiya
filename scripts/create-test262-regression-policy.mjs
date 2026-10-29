// Explicit initial known-pass selection from a verified, completely executed universe.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {checkBaselineUpdate, selectPaths} from './test262-regression-policy.mjs';

const [reference, destination, limitText, reason] = process.argv.slice(2);
if (process.argv.length !== 6)
  throw Error('usage: node scripts/create-test262-regression-policy.mjs REFERENCE_DIR NEW_POLICY_JSON MAX_FILES REASON');
const limit = Number(limitText);
assert(Number.isInteger(limit) && limit >= 1 && limit <= 512, 'MAX_FILES must be 1..512');
assert(!fs.existsSync(destination), 'policy destination must be new');
const verifier = fileURLToPath(new URL('./verify-test262-baseline.mjs', import.meta.url));
const verification = JSON.parse(execFileSync(process.execPath, [verifier, reference],
  {encoding: 'utf8', maxBuffer: 16 * 1024 * 1024}));
const bytes = fs.readFileSync(path.join(reference, 'baseline.json'));
const baseline = JSON.parse(bytes);
assert.match(verification.corpus_revision, /^[a-f0-9]{40}$/, 'reference must be pinned');
assert.equal(baseline.universe.metadata_error_files, 0);
assert.equal(baseline.totals.not_run, 0, 'reference universe must be completely executed');
const read = file => JSON.parse(fs.readFileSync(path.join(reference, file)));
const rows = new Map();
for (const row of read(baseline.outcomes.file)) {
  if (!rows.has(row.path)) rows.set(row.path, []);
  rows.get(row.path).push(row);
}
const candidates = read(baseline.catalog.file).filter(test => {
  const results = rows.get(test.path);
  return test.parsed === true && results?.length === test.variants.length &&
    results.every(row => row.outcome === 'pass');
}).map(test => ({...test, labels: new Set([
  ...(test.features.length ? test.features.map(f => 'feature:' + f) : ['untagged']),
  ...test.variants.map(v => 'variant:' + v),
  ...test.flags.filter(f => ['async', 'raw', 'module'].includes(f)).map(f => 'flag:' + f),
  ...(test.negative ? ['negative:' + test.negative.phase + ':' + test.negative.type] : ['positive']),
])})).sort((a, b) => a.path < b.path ? -1 : a.path > b.path ? 1 : 0);
assert(candidates.length > 0, 'no completely passing files');
const available = new Set(candidates.flatMap(test => [...test.labels]));
const covered = new Set(), chosen = [], selected = new Set();
while (chosen.length < Math.min(limit, candidates.length)) {
  let best, score = -1;
  for (const test of candidates) {
    if (selected.has(test.path)) continue;
    const added = [...test.labels].filter(label => !covered.has(label)).length;
    if (added > score) { best = test; score = added; }
  }
  chosen.push(best);
  selected.add(best.path);
  for (const label of best.labels) covered.add(label);
}
const policy = {schema: 1, update: {reason},
  reference_baseline_sha256: createHash('sha256').update(bytes).digest('hex'),
  pr_selection: [...selected].sort(), skip_reasons: [],
  selection_evidence: {strategy: 'all-variants-pass; greedy uncovered feature/variant/negative/async/raw labels; lexical ties',
    max_files: limit, candidate_files: candidates.length,
    selected_variants: chosen.reduce((sum, test) => sum + test.variants.length, 0),
    available_labels: [...available].sort(), covered_labels: [...covered].sort(),
    uncovered_labels: [...available].filter(label => !covered.has(label)).sort()}};
checkBaselineUpdate(null, policy);
selectPaths(policy, read('universe.json'));
fs.writeFileSync(destination, JSON.stringify(policy, null, 2) + '\n', {flag: 'wx'});
console.log(JSON.stringify({policy: destination, files: chosen.length,
  variants: policy.selection_evidence.selected_variants, uncovered_labels: policy.selection_evidence.uncovered_labels}));
