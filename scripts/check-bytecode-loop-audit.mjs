// Preserve the complete audit, while checking loop regressions independently
// of explicitly named sibling features. Failed dependency cases are not passes.
import fs from 'node:fs';
import assert from 'node:assert/strict';
const [path, exitText] = process.argv.slice(2);
const report = JSON.parse(fs.readFileSync(path));
assert.equal(report.revision, 'd53ebc8b1931d9f1541e43d59650776e4b766248');
assert.equal(report.selected_files, 24);
assert.equal(report.outcomes.length, 46);
assert.equal(report.skipped, 0);
const boundary = row => {
  if (/for\/scope-head-var-none\.js$/.test(row.path) ||
      /(?:do-while\/S12\.6\.1_A3|while\/S12\.6\.2_A3)\.js$/.test(row.path)) {
    return 'eval is not defined';
  }
  if (/for\/scope-body-var-none\.js$/.test(row.path)) return 'unsupported bytecode expression';
  if (/(?:do-while\/S12\.6\.1_A2|while\/S12\.6\.2_A2)\.js$/.test(row.path)) {
    return 'unsupported bytecode statement'; // Whole original try/catch program.
  }
  if (/break\/S12\.8_A3\.js$/.test(row.path) && row.variant === 'Sloppy') {
    return 'unsupported sloppy block function Annex B semantics';
  }
  return null;
};
const seen = new Set();
let core = 0, dependencyPass = 0, dependencyFail = 0;
for (const row of report.outcomes) {
  const key = `${row.path}:${row.variant}`;
  assert(!seen.has(key)); seen.add(key);
  assert(['Sloppy', 'Strict'].includes(row.variant));
  assert(/^[a-f0-9]{40}$/.test(report.fixture_git_blobs[row.path]));
  const expected = boundary(row);
  if (!expected) {
    assert.equal(row.status, 'pass', `${key}: ${row.message}`);
    core++;
  } else if (row.status === 'pass') {
    dependencyPass++;
  } else {
    assert.equal(row.status, 'fail');
    assert(row.message.includes(expected), `${key}: unexpected failure ${row.message}`);
    dependencyFail++;
  }
}
assert.equal(core, 35);
assert.equal(dependencyPass + dependencyFail, 11);
assert.equal(report.passed, core + dependencyPass);
assert.equal(report.failed, dependencyFail);
assert.equal(Number(exitText), dependencyFail ? 101 : 0);
console.log(`Loop target: ${core} pass, 0 fail. Dependency boundary: ${dependencyPass} pass, ${dependencyFail} fail. Complete selected audit: ${report.passed} pass / ${report.failed} fail; 0 skipped. No full-suite conformance claim.`);
