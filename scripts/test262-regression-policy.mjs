// Policy shared by the small PR selection and periodic full-corpus comparison.
// Inputs are actual capture outcomes; callers must verify raw capture integrity.
import assert from 'node:assert/strict';

const statuses = new Set(['pass', 'fail', 'skip', 'timeout', 'not_run', 'unreported']);
const variants = new Set(['Sloppy', 'Strict', 'Module']);
const id = row => JSON.stringify([row.path, row.variant]);
const reason = text => typeof text === 'string' && text.trim().length > 0;
function canonicalPath(name) {
  return typeof name === 'string' && name.startsWith('test/') &&
    !name.includes('\\') && !name.split('/').some(s => !s || s === '.' || s === '..');
}
function index(rows) {
  assert(Array.isArray(rows), 'outcomes must be an array');
  const map = new Map();
  for (const row of rows) {
    assert(canonicalPath(row.path), 'invalid test path');
    assert(variants.has(row.variant), 'invalid variant');
    assert(statuses.has(row.outcome), 'invalid outcome');
    assert(!map.has(id(row)), 'duplicate test variant');
    map.set(id(row), row);
  }
  return map;
}

export function compareOutcomes(expectedRows, actualRows, selectedPaths, skipReasons = []) {
  const expected = index(expectedRows), actual = index(actualRows);
  assert(Array.isArray(selectedPaths) && selectedPaths.length > 0, 'empty selection');
  assert(selectedPaths.every(canonicalPath), 'invalid selection path');
  const selected = new Set(selectedPaths);
  assert.equal(selected.size, selectedPaths.length, 'duplicate selection path');
  const baselinePaths = new Set(expectedRows.map(r => r.path));
  for (const p of selected) assert(baselinePaths.has(p), 'selection outside baseline');
  const approvals = new Map();
  for (const row of skipReasons) {
    assert(canonicalPath(row.path) && variants.has(row.variant) && reason(row.reason),
      'every skip exception requires a variant and nonempty reason');
    assert(!approvals.has(id(row)), 'duplicate skip exception');
    approvals.set(id(row), row.reason.trim());
  }
  const failures = [], transitions = [], used = new Set();
  for (const [key, before] of expected) {
    if (!selected.has(before.path)) continue;
    const after = actual.get(key);
    if (!after) {
      failures.push({path: before.path, variant: before.variant, reason: 'missing result'});
      continue;
    }
    if (after.outcome === 'not_run' || after.outcome === 'unreported') {
      failures.push({path: after.path, variant: after.variant, reason: after.outcome});
      continue;
    }
    const newlySkipped = before.outcome !== 'skip' && after.outcome === 'skip';
    const approved = newlySkipped && approvals.has(key);
    if (approved) used.add(key);
    if (before.outcome !== after.outcome) transitions.push({path: before.path,
      variant: before.variant, before: before.outcome, after: after.outcome,
      ...(approved ? {reason: approvals.get(key)} : {})});
    if (newlySkipped && !approved) failures.push({path: after.path, variant: after.variant,
      reason: 'new skip requires an explicit reason'});
    else if (before.outcome === 'pass' && after.outcome !== 'pass' && !approved)
      failures.push({path: after.path, variant: after.variant, reason: 'known pass regressed to ' + after.outcome});
  }
  for (const [key, row] of actual) {
    if (!selected.has(row.path) || !expected.has(key))
      failures.push({path: row.path, variant: row.variant, reason: 'unexpected result'});
  }
  for (const key of approvals.keys()) assert(used.has(key), 'stale or out-of-scope skip exception');
  return {passed: failures.length === 0, selected_files: selected.size,
    observed_variants: actual.size, failures, transitions};
}

export function checkComparableConditions(before, after) {
  // Engine identity and host may change; execution semantics and budgets may not.
  for (const key of ['corpus_revision', 'manifest_revision', 'manifest_sha256',
    'backend_requested', 'module_execution_path', 'timeout_ms', 'memory_mb', 'stack_kb']) {
    assert(before[key] !== undefined && after[key] !== undefined, 'missing condition: ' + key);
    assert.deepEqual(after[key], before[key], 'incomparable condition: ' + key);
  }
  assert.match(before.corpus_revision, /^[a-f0-9]{40}$/, 'baseline must use a pinned corpus');
}

export function checkBaselineUpdate(previous, next) {
  assert.equal(next.schema, 1);
  assert(reason(next.update?.reason), 'baseline requires an update reason');
  const payload = value => { const {update, ...rest} = value; return rest; };
  if (previous && JSON.stringify(payload(previous)) !== JSON.stringify(payload(next))) {
    assert.notDeepEqual(next.update, previous.update,
      'baseline changes require a fresh update reason');
  }
}
