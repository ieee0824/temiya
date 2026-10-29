import test from 'node:test';
import assert from 'node:assert/strict';
import {compareOutcomes, checkComparableConditions, checkBaselineUpdate, selectPaths, validateSkipReasons}
  from '../scripts/test262-regression-policy.mjs';
const row = (outcome, variant = 'Sloppy') => ({path: 'test/a.js', variant, outcome});
const paths = ['test/a.js'];

test('all skip settings require reasons and unique identities before selection filtering', () => {
  const outside = {path: 'test/outside-pr.js', variant: 'Strict', reason: 'Tracked missing host hook'};
  assert.equal(validateSkipReasons([outside]).size, 1);
  assert.throws(() => validateSkipReasons([{...outside, reason: ' '}]), /nonempty/);
  assert.throws(() => validateSkipReasons([outside, outside]), /duplicate/);
  assert.throws(() => validateSkipReasons([null]), /nonempty/);
  assert.throws(() => validateSkipReasons(undefined), /explicit array/);
});

test('full shards cover every original path exactly once, including uneven boundaries', () => {
  const universe = Array.from({length: 52133}, (_, n) => 'test/' + String(n).padStart(5, '0') + '.js');
  const rows = Array.from({length: 8}, (_, shard) => selectPaths({}, universe, {full: true, shard, shards: 8}));
  assert.deepEqual(rows.flat().sort(), universe);
  assert.equal(new Set(rows.flat()).size, universe.length);
  assert.equal(Math.max(...rows.map(r => r.length)) - Math.min(...rows.map(r => r.length)), 1);
  assert.deepEqual(selectPaths({pr_selection: paths}, paths), paths);
  assert.throws(() => selectPaths({}, universe, {full: true, shard: 8, shards: 8}), /index/);
  assert.throws(() => selectPaths({pr_selection: paths}, paths, {shards: 2}), /only full/);
  assert.throws(() => selectPaths({pr_selection: ['test/z.js']}, paths), /outside/);
});

test('known pass cannot become fail, timeout, skip, missing, or unexecuted', () => {
  for (const status of ['fail', 'timeout', 'skip', 'not_run', 'unreported'])
    assert.equal(compareOutcomes([row('pass')], [row(status)], paths).passed, false, status);
  assert.equal(compareOutcomes([row('pass')], [], paths).passed, false);
  assert.equal(compareOutcomes([row('pass')], [row('pass')], paths).passed, true);
});
test('skip exceptions require an explicit reason for the exact variant', () => {
  const reason = {...row('skip'), reason: 'Unsupported host contract tracked in #152'};
  assert.equal(compareOutcomes([row('pass')], [row('skip')], paths, [reason]).passed, true);
  assert.throws(() => compareOutcomes([row('pass')], [row('skip')], paths,
    [{...reason, reason: ' '}]), /reason/);
  assert.throws(() => compareOutcomes([row('pass')], [row('pass')], paths, [reason]), /stale/);
  assert.throws(() => compareOutcomes([row('pass')], [row('skip')], paths,
    [{...reason, variant: 'Strict'}]), /stale/);
});
test('existing failures remain failures and improvements do not mask other regressions', () => {
  const before = [row('pass'), row('fail', 'Strict')];
  const after = [row('fail'), row('pass', 'Strict')];
  const result = compareOutcomes(before, after, paths);
  assert.equal(result.passed, false);
  assert.equal(result.failures.length, 1);
  assert.equal(compareOutcomes([row('fail')], [row('fail')], paths).passed, true);
  assert.equal(compareOutcomes([row('fail')], [row('skip')], paths).passed, false);
});
test('selection and variant identities cannot silently drop or duplicate records', () => {
  assert.throws(() => compareOutcomes([row('pass')], [row('pass'), row('pass')], paths), /duplicate/);
  assert.throws(() => compareOutcomes([row('pass')], [], []), /empty/);
  assert.throws(() => compareOutcomes([row('pass')], [], ['test/../a.js']), /invalid/);
  assert.equal(compareOutcomes([row('pass')], [row('pass'), row('pass', 'Strict')], paths).passed, false);
});
test('baseline updates need a new reason, including selection and budget changes', () => {
  const initial = {schema: 1, cases: ['test/a.js'], update: {reason: 'Initial measured baseline'}};
  checkBaselineUpdate(null, initial);
  checkBaselineUpdate(initial, structuredClone(initial));
  assert.throws(() => checkBaselineUpdate(initial, {...initial, cases: []}), /fresh/);
  assert.throws(() => checkBaselineUpdate(initial,
    {...initial, cases: [], update: {...initial.update, date: 'new date'}}), /fresh/);
  checkBaselineUpdate(initial, {...initial, cases: [], update: {reason: 'Remove obsolete corpus case'}});
  assert.throws(() => checkBaselineUpdate(null, {...initial, update: {reason: ''}}), /reason/);
});
test('comparison rejects changed corpus, backend and execution budgets', () => {
  const before = {corpus_revision: 'a'.repeat(40), manifest_revision: 'a'.repeat(40),
    manifest_sha256: 'b'.repeat(64), backend_requested: 'auto', module_execution_path: 'AST ModuleGraph',
    timeout_ms: 1000, memory_mb: 512, stack_kb: 8192};
  checkComparableConditions(before, {...before, engine_sha256: 'different worker'});
  for (const key of Object.keys(before))
    assert.throws(() => checkComparableConditions(before, {...before, [key]: 'changed'}), /incomparable/);
});
