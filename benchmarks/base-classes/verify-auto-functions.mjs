// Retain the intermediate optimization, including six slower class comparisons.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {gunzipSync} from 'node:zlib';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';

const root = path.dirname(fileURLToPath(import.meta.url)), repo = path.resolve(root, '../..');
const evidence = JSON.parse(fs.readFileSync(path.join(root, 'auto-functions-v1/evidence.json')));
assert.equal(evidence.schema, 1);
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'temiya-auto-class-evidence-'));
try {
  const read = name => JSON.parse(fs.readFileSync(path.join(temporary, name)));
  const names = new Set();
  for (const row of evidence.files) {
    for (const name of [row.file, row.decoded_file]) assert(/^[a-zA-Z0-9_.-]+$/.test(name) && !name.includes('..'));
    assert(!names.has(row.decoded_file)); names.add(row.decoded_file);
    const compressed = fs.readFileSync(path.join(root, 'auto-functions-v1', row.file));
    assert.equal(compressed.length, row.bytes); assert.equal(sha(compressed), row.sha256);
    const bytes = gunzipSync(compressed); assert.equal(bytes.length, row.decoded_bytes); assert.equal(sha(bytes), row.decoded_sha256);
    fs.writeFileSync(path.join(temporary, row.decoded_file), bytes);
  }
  const audit = (script, files) => JSON.parse(execFileSync(process.execPath,
    [path.join(repo, 'scripts', script), ...files.map(name => path.join(temporary, name))], {encoding: 'utf8'}));
  const classes = audit('audit-class-workload-records.mjs', ['class-boa.json', 'class-beforeafter.json', 'class-same-binary.json']);
  const original = audit('audit-original-workload-records.mjs', ['beforeafter.json', 'boa.json', 'baseline-boa.json', 'same-binary.json']);
  assert.deepEqual(original.reports.map(r => r.outcomes), [
    {'within noise threshold': 33}, {'Temiya faster beyond threshold': 33},
    {'Temiya faster beyond threshold': 32, 'within noise threshold': 1},
    {'within noise threshold': 32, 'after faster beyond threshold': 1},
  ]);
  for (const file of ['class-boa.json', 'class-beforeafter.json', 'class-same-binary.json']) {
    for (const name of ['compare-class-workloads.mjs', 'class-benchmark-reference.mjs']) {
      const identity = read(file).identities.find(row => row.file.endsWith('/scripts/' + name));
      assert(identity); assert.equal(identity.sha256, sha(fs.readFileSync(path.join(temporary, name))));
    }
  }
  assert.deepEqual(classes.reports[0].outcomes, {'Temiya slower beyond threshold': 6, 'within noise threshold': 6});
  assert.deepEqual(classes.reports[1].outcomes, {'after faster beyond threshold': 11, 'within noise threshold': 1});
  assert.deepEqual(classes.reports[2].outcomes, {'within noise threshold': 12});
  const info = read('worker-info.json'); assert.equal(info.revision, evidence.source_revision);
  assert.equal(info.executable_sha256, evidence.worker_sha256);
  for (const file of ['class-boa.json', 'class-beforeafter.json', 'class-same-binary.json'])
    assert.equal(read(file).workers[0].executable_sha256, evidence.worker_sha256);
  assert.equal(read('class-beforeafter.json').workers[1].executable_sha256, '1118137892f762548213630a0e705a38793a4ad1add24498ee885d4e806b4c83');
  assert.equal(read('class-same-binary.json').workers[1].executable_sha256, evidence.worker_sha256);
  for (const file of ['beforeafter.json', 'boa.json', 'same-binary.json']) {
    assert.equal(read(file).workers[0].executable_sha256, evidence.worker_sha256);
  }
  assert.equal(read('same-binary.json').workers[1].executable_sha256, evidence.worker_sha256);
  assert.equal(read('beforeafter.json').workers[1].executable_sha256, read('baseline-info.json').executable_sha256);
  assert.equal(read('baseline-boa.json').workers[0].executable_sha256, read('baseline-info.json').executable_sha256);
  const log = fs.readFileSync(path.join(temporary, 'native-all-features.log'), 'utf8');
  const results = [...log.matchAll(/test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored/g)];
  assert.equal(results.reduce((n, r) => n + Number(r[1]), 0), 728);
  assert.equal(results.reduce((n, r) => n + Number(r[2]), 0), 0);
  assert.equal(results.reduce((n, r) => n + Number(r[3]), 0), 26);
  assert.equal(results.length, 96);
  const gate = read('pr-gate-result.json'); assert.equal(gate.passed, true); assert.equal(gate.observed_variants, 214);
  const originals = read('originals-result.json'); assert.equal(originals.accepted, true);
  assert.deepEqual(originals.modes.map(m => m.backend), ['ast', 'auto']);
  for (const mode of originals.modes) {
    assert.equal(mode.verification.totals.pass, 256); assert.equal(mode.verification.totals.fail, 38);
    assert.deepEqual(mode.unexpected, []);
  }
  console.log(JSON.stringify({integrity: 'pass', source: evidence.source_revision,
    original_reports: original.reports.length, class_reports: classes.reports.length, class_outcomes: classes.reports[0].outcomes, scope: evidence.scope}));
} finally {fs.rmSync(temporary, {recursive: true, force: true});}
