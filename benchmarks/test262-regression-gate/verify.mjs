import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {gunzipSync} from 'node:zlib';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';

const root = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(root, '../..');
const evidence = JSON.parse(fs.readFileSync(path.join(root, 'evidence.json')));
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
// Historical captures must use their original policy, independently of current additions.
const productionPolicy = path.join(root, 'production-policy.json');
assert.equal(sha(fs.readFileSync(productionPolicy)),
  'd7031b0d18922a3a0333d47ff169f864334c9e57a23edbc1ce46bd3e83b36bfd');
const canonical = name => typeof name === 'string' && /^[a-zA-Z0-9_./-]+$/.test(name) &&
  !path.isAbsolute(name) && !name.split('/').some(s => !s || s === '.' || s === '..');
assert.equal(evidence.schema, 1);
const ciBytes = gunzipSync(fs.readFileSync(path.join(root, evidence.ci_log.file)));
assert.equal(ciBytes.length, evidence.ci_log.decoded_bytes);
assert.equal(sha(ciBytes), evidence.ci_log.decoded_sha256);
let captureCount = 0;
for (const archive of evidence.archives) {
  assert(canonical(archive.file) && !archive.file.includes('/'));
  const source = path.join(root, archive.file);
  const compressed = fs.readFileSync(source);
  assert.equal(compressed.length, archive.bytes);
  assert.equal(sha(compressed), archive.sha256);
  const listing = execFileSync('tar', ['-tzf', source], {encoding: 'utf8'}).trimEnd().split('\n');
  const verbose = execFileSync('tar', ['-tvzf', source], {encoding: 'utf8'}).trimEnd().split('\n');
  const expected = new Map(archive.files.map(row => [row.file, row]));
  assert.equal(expected.size, archive.files.length);
  assert.equal(listing.length, verbose.length);
  const seen = new Set();
  for (const [i, entry] of listing.entries()) {
    assert(['d', '-'].includes(verbose[i][0]));
    if (entry === './') continue;
    const name = entry.replace(/^\.\//, '').replace(/\/$/, '');
    assert(canonical(name));
    if (verbose[i][0] === 'd') continue;
    assert(expected.has(name) && !seen.has(name));
    seen.add(name);
  }
  assert.equal(seen.size, expected.size);
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'temiya-gate-evidence-'));
  try {
    execFileSync('tar', ['-xzf', source, '--no-same-owner', '--no-same-permissions', '-C', temporary]);
    const read = name => JSON.parse(fs.readFileSync(path.join(temporary, name)));
    for (const row of archive.files) {
      assert(canonical(row.file));
      const bytes = fs.readFileSync(path.join(temporary, row.file));
      assert.equal(bytes.length, row.bytes);
      assert.equal(sha(bytes), row.sha256);
    }
    for (const capture of archive.captures) {
      assert(canonical(capture.directory));
      const result = JSON.parse(execFileSync(process.execPath,
        [path.join(repo, 'scripts/verify-test262-baseline.mjs'), path.join(temporary, capture.directory)],
        {encoding: 'utf8'}));
      assert.deepEqual(result, capture.verification);
      captureCount++;
    }
    if (archive.production) {
      const runner = 'test262-regression';
      const conditions = read(runner + '/capture/conditions.json');
      assert.deepEqual(conditions.provided_engine_build_info, read('test262-build-info.json'));
      assert.equal(conditions.engine_sha256, conditions.provided_engine_build_info.executable_sha256);
      assert.deepEqual(read(runner + '/check-test262-regressions.mjs.exit.json'),
        {status: 0, signal: null, error: null});
      const reference = path.join(temporary, 'decoded-reference');
      execFileSync(process.execPath, [path.join(repo, 'scripts/verify-test262-archive.mjs'),
        path.join(repo, 'benchmarks/test262-feature-baseline'), reference]);
      const gate = JSON.parse(execFileSync(process.execPath,
        [path.join(repo, 'scripts/check-test262-regressions.mjs'), reference,
          path.join(temporary, runner, 'capture'), productionPolicy],
        {encoding: 'utf8'}));
      assert.deepEqual(gate, read(runner + '/check-test262-regressions.mjs.stdout'));
      assert.equal(gate.passed, true); assert.equal(gate.selected_files, 128);
      assert.equal(gate.observed_variants, 214);
      continue;
    }
    for (const runner of ['runner-pr', 'runner-rejected']) {
      const conditions = read(runner + '/capture/conditions.json');
      assert.deepEqual(conditions.provided_engine_build_info, read('engine-info.json'));
      assert.equal(conditions.provided_engine_build_info.executable_sha256, conditions.engine_sha256);
      const exit = read(runner + '/check-test262-regressions.mjs.exit.json');
      assert.equal(exit.status, runner === 'runner-pr' ? 0 : 2);
    }
    const policy = read('policy.json');
    const selected = new Set(policy.pr_selection);
    const rows = read('reference/outcomes.json').filter(row => selected.has(row.path));
    assert.equal(rows.length, policy.selection_evidence.selected_variants);
    assert(rows.every(row => row.outcome === 'pass'));
  } finally {
    fs.rmSync(temporary, {recursive: true, force: true});
  }
}
console.log(JSON.stringify({integrity: 'pass', archives: evidence.archives.length,
  captures: captureCount, ci_run: evidence.ci_run,
  scope: 'Focused original CLI integration evidence, not a full-corpus qualification.'}));
