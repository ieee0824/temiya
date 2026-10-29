// Retained measurements are checked independently of their reported gate flags.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {gunzipSync} from 'node:zlib';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';

const directory = path.dirname(fileURLToPath(import.meta.url));
const repository = path.resolve(directory,'../..');
const read = file => JSON.parse(fs.readFileSync(path.join(directory,file)));
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const provenance = read('provenance.json');
assert.equal(provenance.schema,1);
for (const [file,expected] of Object.entries(provenance.files)) {
  assert.equal(hash(fs.readFileSync(path.join(directory,file))),expected,file);
}
for (const [file,expected] of Object.entries(provenance.source_inputs)) {
  const actual = execFileSync('git',['show',`${provenance.measured_revision}:${file}`],{cwd:repository});
  assert.equal(hash(actual),expected,file);
  // Evidence-only commits must describe the same implementation when created.
  // Future commits may evolve these files; the immutable input pin remains valid.
}
for (const entry of provenance.lossless_profiles) {
  const compressed = fs.readFileSync(path.join(directory,entry.path));
  assert.equal(hash(compressed),entry.compressed_sha256);
  const raw = gunzipSync(compressed);
  assert.equal(hash(raw),entry.uncompressed_sha256);
  assert.equal(raw.length,entry.uncompressed_bytes);
}
const reports = provenance.native_reports.map(file => path.join(directory,file));
const audit = JSON.parse(execFileSync('node',[path.join(repository,'scripts/audit-original-workload-records.mjs'),...reports],{encoding:'utf8',cwd:repository,maxBuffer:8*1024*1024}));
assert.equal(audit.reports.length,31);
for (const report of audit.reports) {
  assert.equal(report.runs,66);
  assert.equal(report.checked_values,1056);
  assert.equal(report.comparisons,33);
}
for (const folder of ['hosted/copy1','hosted/copy2']) {
  execFileSync('node',[path.join(repository,'scripts/audit-omoikane-hosted-identities.mjs'),path.join(directory,folder),provenance.measured_revision,provenance.baseline_revision],{cwd:repository,stdio:'pipe'});
}
for (const folder of ['arm64','hosted/copy1','hosted/copy2']) {
  const before = read(`${folder}/beforeafter.json`);
  const boa = read(`${folder}/boa.json`);
  assert.equal(before.workers[0].revision,provenance.measured_revision);
  assert.equal(before.workers[1].revision,provenance.baseline_revision);
  assert.equal(boa.workers[0].revision,provenance.measured_revision);
  assert(before.comparisons.every(c=>c.outcome!=='after slower beyond threshold'));
  if (folder === 'arm64') {
    assert.equal(boa.comparisons.filter(c=>c.outcome==='Temiya faster beyond threshold').length,32);
    assert.equal(boa.comparisons.filter(c=>c.outcome==='within noise threshold').length,1);
    const replay = read('arm64/boa-recheck.json');
    assert.equal(replay.workers[0].executable_sha256,boa.workers[0].executable_sha256);
    assert.equal(replay.workers[0].revision,provenance.measured_revision);
    assert.equal(replay.comparisons.filter(c=>c.outcome==='Temiya faster beyond threshold').length,31);
    assert.equal(replay.comparisons.filter(c=>c.outcome==='within noise threshold').length,2);
    assert.equal(boa.all_eleven_workloads_faster_beyond_threshold,false);
    assert.equal(replay.all_eleven_workloads_faster_beyond_threshold,false);
  } else {
    assert(boa.comparisons.every(c=>c.outcome==='Temiya faster beyond threshold'));
  }
}
// Both rejected experiments remain distinct sources, including every slowdown.
assert.equal(read('experiments/sdk-560/hosted/copy1/beforeafter.json').comparisons.filter(c=>c.outcome==='after slower beyond threshold').length,1);
assert.equal(read('experiments/cli-8b/hosted/copy2/beforeafter.json').comparisons.filter(c=>c.outcome==='after slower beyond threshold').length,2);
for (const [file,counts] of [
  ['functional/phase-regressions.json',{fail:0,pass:13,skip:0}],
  ['functional/harness-dependencies-ast.json',{fail:0,pass:60,skip:0}],
  ['functional/harness-dependencies-auto.json',{fail:0,pass:60,skip:0}],
  ['functional/all-prior-failures.json',{fail:60,pass:66,skip:0}],
]) {
  const report = read(file);
  assert.deepEqual(report.counts,counts);
  assert.equal(report.source_revision,provenance.measured_revision);
  assert.equal(report.corpus_revision,'d53ebc8b1931d9f1541e43d59650776e4b766248');
}
const ci = read('validation/ci-status.json');
assert.equal(ci.conclusion,'success');
assert.equal(ci.headSha,provenance.measured_revision);
assert.equal(ci.jobs.length,4);
assert(ci.jobs.every(job=>job.conclusion==='success'));
const log = fs.readFileSync(path.join(directory,'validation/ci.log'),'utf8');
const stable = log.split('\n').filter(line=>line.includes('Rust stable')).map(line=>line.match(/([a-f0-9]{64})\s+target\/release\/examples\/measure_workload/)).filter(Boolean);
assert.equal(stable.length,1);
for (const folder of ['hosted/copy1','hosted/copy2']) assert.equal(read(`${folder}/boa.json`).workers[0].executable_sha256,stable[0][1]);
console.log(JSON.stringify({verified_files:Object.keys(provenance.files).length,native_reports:audit.reports.length,original_cli_variants:133,preserved_prior_failures:60,current_native_gate:'Both hosted cohorts must pass all33 actual-Boa decisions; ARM32+1inconclusive and replay31+2inconclusive are NOT all33 qualification; before/after outcomes retained'},null,2));
