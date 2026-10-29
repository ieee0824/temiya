// Retain the initial slow class results; passing integrity is not speed acceptance.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {gunzipSync} from 'node:zlib';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';

const root = path.dirname(fileURLToPath(import.meta.url)), repo = path.resolve(root, '../..');
const evidence = JSON.parse(fs.readFileSync(path.join(root, 'initial/evidence.json')));
assert.equal(evidence.schema, 1);
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'temiya-initial-class-evidence-'));
try {
  const read = name => JSON.parse(fs.readFileSync(path.join(temporary, name)));
  const names = new Set();
  for (const row of evidence.files) {
    for (const name of [row.file, row.decoded_file]) assert(/^[a-zA-Z0-9_.-]+$/.test(name) && !name.includes('..'));
    assert(!names.has(row.decoded_file)); names.add(row.decoded_file);
    const compressed = fs.readFileSync(path.join(root, 'initial', row.file));
    assert.equal(compressed.length, row.bytes); assert.equal(sha(compressed), row.sha256);
    const bytes = gunzipSync(compressed); assert.equal(bytes.length, row.decoded_bytes); assert.equal(sha(bytes), row.decoded_sha256);
    fs.writeFileSync(path.join(temporary, row.decoded_file), bytes);
  }
  const audit = (script, files) => JSON.parse(execFileSync(process.execPath,
    [path.join(repo, 'scripts', script), ...files.map(name => path.join(temporary, name))], {encoding: 'utf8'}));
  const original = audit('audit-original-workload-records.mjs', ['beforeafter.json', 'boa.json', 'baseline-boa.json', 'same-binary.json']);
  const classes = audit('audit-class-workload-records.mjs', ['class-boa.json', 'class-same-binary.json']);
  for (const file of ['class-boa.json', 'class-same-binary.json']) {
    for (const name of ['compare-class-workloads.mjs', 'class-benchmark-reference.mjs']) {
      const identity = read(file).identities.find(row => row.file.endsWith('/scripts/' + name));
      assert(identity); assert.equal(identity.sha256, sha(fs.readFileSync(path.join(temporary, name))));
    }
  }
  assert.equal(original.reports[1].outcomes['Temiya faster beyond threshold'], 33);
  assert.deepEqual(classes.reports[0].outcomes, {'Temiya slower beyond threshold': 9, 'within noise threshold': 3});
  const info = read('current-info.json'); assert.equal(info.revision, evidence.source_revision);
  assert.equal(info.executable_sha256, evidence.worker_sha256);
  for (const file of ['beforeafter.json', 'boa.json', 'same-binary.json', 'class-boa.json', 'class-same-binary.json'])
    assert.equal(read(file).workers[0].executable_sha256, evidence.worker_sha256);
  const log = fs.readFileSync(path.join(temporary, 'native-all-features.log'), 'utf8');
  const results = [...log.matchAll(/test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored/g)];
  assert.equal(results.reduce((n, r) => n + Number(r[1]), 0), 725);
  assert.equal(results.reduce((n, r) => n + Number(r[2]), 0), 0);
  assert.equal(results.reduce((n, r) => n + Number(r[3]), 0), 26);
  assert.equal(results.length, 96);
  assert.equal(read('pr-gate-before-caller-fix.log').passed, false);
  const fixed = read('pr-gate-after-caller-fix.log'); assert.equal(fixed.passed, true); assert.equal(fixed.observed_variants, 214);
  console.log(JSON.stringify({integrity: 'pass', source: evidence.source_revision, original_reports: original.reports.length,
    class_reports: classes.reports.length, class_outcomes: classes.reports[0].outcomes, scope: evidence.scope}));
} finally {fs.rmSync(temporary, {recursive: true, force: true});}
