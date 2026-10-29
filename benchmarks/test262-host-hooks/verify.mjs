// Validate preserved evidence. Audit success alone is not a speed qualification.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {gunzipSync} from 'node:zlib';
import {fileURLToPath} from 'node:url';
const directory = path.dirname(fileURLToPath(import.meta.url));
const repository = path.resolve(directory, '../..');
const bytes = file => { const absolute=path.join(directory,file); return fs.existsSync(absolute) ? fs.readFileSync(absolute) : gunzipSync(fs.readFileSync(absolute+'.gz')); };
const read = file => JSON.parse(bytes(file));
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const p = read('provenance.json');
assert.equal(p.schema, 1);
for (const [file, expected] of Object.entries(p.files)) {
  assert.equal(hash(fs.readFileSync(path.join(directory, file))), expected, file);
}
for (const [revision, inputs] of Object.entries(p.source_inputs)) {
  for (const [file, expected] of Object.entries(inputs)) {
    const actual = execFileSync('git', ['show', `${revision}:${file}`], {cwd: repository});
    assert.equal(hash(actual), expected, `${revision}:${file}`);
  }
}
for (const file of p.library_equivalence_files) {
  const before = execFileSync('git', ['show', `${p.measured_revision}:${file}`], {cwd: repository});
  const after = execFileSync('git', ['show', `${p.implementation_revision}:${file}`], {cwd: repository});
  assert.equal(hash(before), hash(after), `compiled Engine input ${file}`);
}
for (const entry of p.compressed_files ?? []) {
  const compressed = fs.readFileSync(path.join(directory, entry.path));
  assert.equal(hash(compressed), entry.compressed_sha256);
  const raw = gunzipSync(compressed);
  assert.equal(hash(raw), entry.uncompressed_sha256);
  assert.equal(raw.length, entry.uncompressed_bytes);
}
const temporary = fs.mkdtempSync(path.join(os.tmpdir(),'temiya-host-evidence-'));
process.on('exit',()=>fs.rmSync(temporary,{recursive:true,force:true}));
for (const folder of ['arm64','hosted']) {
  const unpack = (source,target) => {
    fs.mkdirSync(target,{recursive:true});
    for (const entry of fs.readdirSync(source,{withFileTypes:true})) {
      if(entry.isDirectory()) unpack(path.join(source,entry.name),path.join(target,entry.name));
      else { const name=entry.name.endsWith('.gz')?entry.name.slice(0,-3):entry.name; const data=fs.readFileSync(path.join(source,entry.name)); fs.writeFileSync(path.join(target,name),entry.name.endsWith('.gz')?gunzipSync(data):data); }
    }
  };
  unpack(path.join(directory,folder),path.join(temporary,folder));
}
const expectedReports=['arm64/beforeafter.json','arm64/boa.json','arm64/beforeafter-recheck.json','arm64/same-binary-control.json'];
for(const cohort of ['copy1','copy2']) for(const name of ['beforeafter','boa','baseline-boa','same-binary-control']) expectedReports.push(`hosted/${cohort}/${name}.json`);
assert.deepEqual([...p.native_reports].sort(),expectedReports.sort());
const audit = JSON.parse(execFileSync('node', [
  path.join(repository, 'scripts/audit-original-workload-records.mjs'),
  ...p.native_reports.map(file => path.join(temporary, file)),
], {cwd: repository, encoding: 'utf8', maxBuffer: 8 * 1024 * 1024}));
for (const report of audit.reports) {
  assert.equal(report.runs, 66);
  assert.equal(report.checked_values, 1056);
  assert.equal(report.comparisons, 33);
}
const tallies = Object.fromEntries(audit.reports.map(r => [path.relative(temporary, r.file), r.outcomes]));
assert.deepEqual(tallies, p.native_outcomes);
assert.deepEqual(p.qualified_boa_reports, ['hosted/copy1/boa.json','hosted/copy2/boa.json']);
for (const file of p.qualified_boa_reports) {
  const r = read(file);
  assert.equal(r.all_eleven_workloads_faster_beyond_threshold,true);
  assert(r.comparisons.every(c=>c.outcome === 'Temiya faster beyond threshold'));
}
assert.deepEqual(read('arm64/beforeafter-recheck.json').comparisons.filter(c=>c.outcome === 'after slower beyond threshold').map(c=>[c.workload,c.pair]),[['arith',2],['prop-mega',0],['object-alloc',1]]);
// In particular, the first ARM observation cannot disappear into a replay.
assert.deepEqual(read('arm64/beforeafter.json').comparisons.filter(c => c.outcome === 'after slower beyond threshold').map(c => [c.workload, c.pair]), [
  ['prop-mono', 1], ['call', 1], ['call', 2], ['array', 0], ['primitive-string-property', 0],
]);
for (const folder of ['hosted/copy1', 'hosted/copy2']) {
  execFileSync('node', [path.join(repository, 'scripts/audit-omoikane-hosted-identities.mjs'), path.join(temporary, folder), p.measured_revision, p.baseline_revision], {cwd: repository, stdio: 'pipe'});
}
for (const [file, expected] of Object.entries({
  'phase-regressions': {fail:0,pass:13,skip:0},
  'harness-dependencies-ast': {fail:0,pass:60,skip:0},
  'harness-dependencies-auto': {fail:0,pass:60,skip:0},
  'host-regressions-ast': {fail:0,pass:34,skip:0},
  'host-regressions-auto': {fail:0,pass:34,skip:0},
  'all-prior-failures': {fail:60,pass:66,skip:0},
  'byteLength-full': {fail:4,pass:16,skip:0},
  'global-script-declarations': {fail:9,pass:16,skip:0},
})) {
  const r = read(`functional/${file}.json`);
  assert.deepEqual(r.counts, expected, file);
  assert.equal(r.source_revision, p.implementation_revision, file);
  assert.equal(r.corpus_revision, 'd53ebc8b1931d9f1541e43d59650776e4b766248');
}
const utf16 = read('functional/utf16-source.json');
assert.deepEqual(utf16.counts,{fail:1,pass:0,skip:0});
assert.equal(utf16.corpus_revision,null);
assert.equal(utf16.source_revision,p.implementation_revision);
assert.deepEqual(utf16.tracking,[527]);
assert.equal(utf16.results[0].observed_failure.type,'EngineError');
const profile = read('profile/metadata.json');
assert.deepEqual(profile.diagnostic_workloads,['prop-mono','call','array','primitive-string-property']);
assert.equal(profile.runs.length,8);
assert.equal(profile.rustc,read('hosted/copy1/boa.json').rustc);
assert.deepEqual(profile.original_manifest,read('hosted/copy1/manifest.json'));
for (const build of profile.builds) {
  assert.equal(build.revision,build.label==='measured'?p.measured_revision:p.baseline_revision);
  const native=read('hosted/copy1/beforeafter.json').workers[build.label==='measured'?0:1];
  assert.equal(build.executable_sha256,native.executable_sha256);
  assert.deepEqual(build.features,[]);
}
for (const name of profile.diagnostic_workloads) {
  const runs=profile.runs.filter(r=>r.name===name);assert.equal(runs.length,2);
  assert(runs.every(r=>r.exit_status===0&&r.signal===null&&r.error===null&&r.validated_passes===2&&r.annotation_exit===0));
  for(const run of runs) {
    assert.equal(run.input_sha256,hash(bytes('profile/inputs/'+name+'.json')));
    const raw=bytes('profile/'+run.label+'/'+name+'.callgrind').toString();
    const events=raw.match(/^events:\s+(.+)$/m)[1].trim().split(/\s+/);
    const counters=raw.match(/^summary:\s+(.+)$/m)[1].trim().split(/\s+/).map(Number);
    assert.deepEqual(Object.fromEntries(events.map((event,index)=>[event,counters[index]])),run.counters);
    const stdout=JSON.parse(bytes('profile/'+run.label+'/'+name+'.stdout'));
    assert.equal(stdout.validated_passes,2);
    assert.equal(stdout.samples_ns.length,1);
    assert.equal(stdout.expected,read('profile/inputs/'+name+'.json').expected);
  }
}
const ci = read('validation/ci-status.json');
assert.equal(ci.conclusion, 'success');
assert.equal(ci.headSha, p.implementation_revision);
assert.equal(ci.jobs.length, 4);
assert(ci.jobs.every(job => job.conclusion === 'success'));
const log = gunzipSync(fs.readFileSync(path.join(directory, 'validation/ci.log.gz'))).toString();
const stable = log.split('\n').filter(l => l.includes('Rust stable')).map(l => l.match(/([a-f0-9]{64})\s+target\/release\/examples\/measure_workload/)).filter(Boolean);
assert.equal(stable.length, 1);
for (const folder of ['hosted/copy1', 'hosted/copy2']) assert.equal(read(`${folder}/boa.json`).workers[0].executable_sha256, stable[0][1]);
console.log(JSON.stringify({verified_files:Object.keys(p.files).length,native_reports:audit.reports.length,original_positive_variants:201,native_outcomes:tallies,qualification:p.qualification}, null, 2));
