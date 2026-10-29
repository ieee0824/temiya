// Verify preserved build evidence independently of the feature baseline capture.
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {createHash} from 'node:crypto';
import {gunzipSync} from 'node:zlib';
import assert from 'node:assert/strict';

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, '../..');
const root = path.join(here, 'preflight');
const read = name => JSON.parse(fs.readFileSync(path.join(root, name)));
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const evidence = read('evidence.json');
const identity = read('worker-identity.json');
assert.equal(evidence.schema, 1);
assert.equal(identity.schema, 1);
assert.equal(identity.source, evidence.source);

const logs = new Map();
for (const row of evidence.files) {
  assert.equal(path.basename(row.file), row.file);
  const bytes = gunzipSync(fs.readFileSync(path.join(root, row.file)));
  assert.equal(bytes.length, row.decoded_bytes);
  assert.equal(sha(bytes), row.decoded_sha256);
  logs.set(row.file, bytes.toString('utf8'));
}
const totals = {};
for (const line of logs.get('temiya-151-ci-final.log.gz').split('\n')) {
  const [job, step] = line.split('\t');
  if (step !== 'Test') continue;
  const match = line.match(/test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored/);
  if (!match) continue;
  const total = totals[job] ??= {passed: 0, failed: 0, ignored: 0, groups: 0};
  total.passed += Number(match[1]);
  total.failed += Number(match[2]);
  total.ignored += Number(match[3]);
  total.groups++;
}
assert.deepEqual(totals, evidence.toolchains);
for (const [name, digest] of Object.entries(identity.unchanged_worker_inputs)) {
  assert(!path.isAbsolute(name) && !name.split('/').includes('..'));
  assert.equal(sha(fs.readFileSync(path.join(repo, name))), digest, name);
}
const workerLine = logs.get(identity.x64_ci_log)
  .match(/([a-f0-9]{64})  target\/release\/examples\/measure_workload/);
assert(workerLine);
assert.equal(workerLine[1], identity.x64_worker_sha256);
for (const previous of identity.previous_hosted_reports) {
  assert(!path.isAbsolute(previous.report) && !previous.report.split('/').includes('..'));
  const report = JSON.parse(gunzipSync(fs.readFileSync(path.join(repo, previous.report))));
  assert.equal(report.workers[0].executable_sha256, identity.x64_worker_sha256);
  assert.equal(previous.worker_sha256, identity.x64_worker_sha256);
  assert.equal(report.workers[0].revision, identity.previous_measured_source);
}
const arm = JSON.parse(gunzipSync(fs.readFileSync(path.join(repo,
  'benchmarks/test262-process-isolation/performance/arm64/current-info.json.gz'))));
assert.equal(arm.executable_sha256, identity.arm_worker_sha256);
console.log(JSON.stringify({integrity: 'pass', logs: logs.size,
  worker_inputs: Object.keys(identity.unchanged_worker_inputs).length,
  scope: 'Preserved build evidence and previous measured worker identities; full capture verified separately.'}));
