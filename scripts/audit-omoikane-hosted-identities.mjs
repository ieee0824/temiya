// Cross-check all four original-eleven hosted reports, compiler, actual Boa features/locks and same-binary controls.
import fs from 'node:fs';
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
const [directory,measuredRevision,baselineRevision]=process.argv.slice(2);
assert(/^[a-f0-9]{40}$/.test(measuredRevision));assert(/^[a-f0-9]{40}$/.test(baselineRevision));
const read=n=>JSON.parse(fs.readFileSync(`${directory}/${n}.json`));
const reports=Object.fromEntries(['beforeafter','boa','baseline-boa','same-binary-control'].map(n=>[n,read(n)]));
const baseline=read('baseline-build-info'),boa=read('boa-build-info'),same=read('same-binary-build-info'),availability=read('hardware-availability'),manifest=read('manifest');
const compiler=fs.readFileSync(`${directory}/compiler.txt`,'utf8').trim();
assert.equal(fs.readFileSync(`${directory}/measured-revision.txt`,'utf8').trim(),measuredRevision);
assert.equal(fs.readFileSync(`${directory}/baseline-revision.txt`,'utf8').trim(),baselineRevision);
assert.equal(baseline.revision,baselineRevision);assert.equal(same.revision,measuredRevision);
assert.equal(baseline.rustc,compiler);assert.equal(boa.rustc,compiler);assert.equal(same.rustc,compiler);
assert.equal(boa.omoikane_revision,manifest.revision);assert.equal(boa.checkout_clean,true);assert.equal(boa.profile,'release');assert.equal(boa.workload_kind,'sync');assert.equal(boa.rustflags,'');
assert.deepEqual(boa.cargo_args,['build','--locked','--release']);
assert.deepEqual(boa.features,{omoikane:[],boa_engine:['annex-b','default','experimental','float16','intl','intl_bundled','temporal','xsum'],regress:['backend-pikevm','default','std','utf16']});
assert.equal(boa.execution_mode,'interpreter; baseline-jit feature absent');
const originalWorker=execFileSync('git',['show',`${measuredRevision}:benchmarks/omoikane-worker/main.rs`]);
assert.equal(boa.worker_source_sha256,createHash('sha256').update(originalWorker).digest('hex'));
assert.equal(boa.original_lock_sha256,'f3988c4d9b0ecc28560df18df1cae7760c7c55a016318ea3e31f3ebbfcaea211');
assert.equal(boa.adapter_lock_sha256,'6b4debb1e4368b52de4586b7b30c3d4b6b8882a1bd4ee87d78aa488a252517e7');
assert.equal(availability.target_available,true);assert.equal(availability.required_cpu_model,'');
const candidate=reports.boa.workers[0];assert.equal(candidate.revision,measuredRevision);
const expected={beforeafter:[[candidate,measuredRevision],[baseline,baselineRevision]],boa:[[candidate,measuredRevision],[boa,manifest.revision]],'baseline-boa':[[baseline,baselineRevision],[boa,manifest.revision]],'same-binary-control':[[candidate,measuredRevision],[candidate,measuredRevision]]};
for(const [name,report]of Object.entries(reports)){
 assert.equal(report.rustc,compiler);assert.deepEqual(report.manifest,manifest);assert.deepEqual(report.machine,reports.boa.machine);assert.deepEqual(report.machine.cpu_models,availability.observed_cpu_models);
 for(let i=0;i<2;i++){
  const worker=report.workers[i],[info,revision]=expected[name][i];assert.equal(worker.revision,revision);assert.equal(worker.executable,info.executable);
  if(worker.name==='omoikane vendored Boa'){assert.deepEqual(worker.build,boa);assert.equal(worker.build.executable_sha256,boa.executable_sha256)}
  else{assert.equal(worker.mode,'Auto Script');assert.equal(worker.profile,'release');assert.deepEqual(worker.features,[]);assert.equal(worker.executable_sha256,info.executable_sha256)}
 }
}
assert.equal(same.executable_sha256,candidate.executable_sha256);assert.equal(same.mode_probe.expected,42);assert.equal(same.mode_probe.validated_passes,2);assert.equal(baseline.mode_probe.expected,42);assert.equal(baseline.mode_probe.validated_passes,2);
console.log(JSON.stringify({measured_revision:measuredRevision,baseline_revision:baselineRevision,actual_omoikane_revision:boa.omoikane_revision,machine:reports.boa.machine,compiler,report_identity_consistent:4,original_adapter_source_sha256:boa.worker_source_sha256,same_binary_hash:candidate.executable_sha256,original_features_and_lock_preserved:true,scope:'Cross-file source/compiler/feature/binary/manifest/control provenance; independent statistical audit is also required'},null,2));
