// Output/source validation only; no native timing qualification.
import fs from 'node:fs';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=fileURLToPath(new URL('./',import.meta.url)),repo=fileURLToPath(new URL('../../',import.meta.url)),dir=root+'validation-v1';
const read=p=>JSON.parse(fs.readFileSync(p)),sha=b=>createHash('sha256').update(b).digest('hex'),e=read(dir+'/evidence.json');
assert.equal(e.source,'c2bc478647815fdc013f254f5e1e72cfd804945b');assert.equal(e.baseline,'622bc975af697c49298d2fb5b71b575aeed6cdb5');
for(const [file,hash]of Object.entries(e.files))assert.equal(sha(fs.readFileSync(dir+'/'+file)),hash,file);
for(const [file,revision]of [['worker-info.json',e.source],['baseline-worker-info.json',e.baseline]]){
 const info=read(dir+'/'+file);assert.equal(info.revision,revision);assert.equal(info.provenance.source_clean,true);assert.equal(info.provenance.source_inputs.length,80);
 for(const input of info.provenance.source_inputs)assert.equal(sha(execFileSync('git',['show',revision+':'+input.file],{cwd:repo})),input.sha256,input.file);
}
const boa=read(dir+'/boa-build-info.json');assert.equal(boa.omoikane_revision,'007556adccb8fb4329236932ca8175f8e8d119d2');assert.equal(boa.checkout_clean,true);
const freeze=read(root+'workload-freeze.json');assert.equal(freeze.inputs.length,6);
for(const x of freeze.inputs){
 assert.equal(sha(fs.readFileSync(root+'workloads-v1/'+x.file)),x.sha256);
 for(const group of ['boa-oracles','baseline-oracles','candidate-oracles']){const r=read(dir+'/'+group+'/'+x.file);assert.equal(r.expected,50005000);assert.equal(r.validated_passes,2);assert.equal(r.samples_ns.length,1);}
}
const native=fs.readFileSync(dir+'/native-tests.log','utf8');assert(!native.includes('test result: FAILED'));const tests=[...native.matchAll(/test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored/g)];assert.equal(tests.reduce((n,r)=>n+Number(r[1]),0),878);assert(tests.every(r=>Number(r[2])===0));assert.equal(tests.reduce((n,r)=>n+Number(r[3]),0),26);assert.deepEqual(e.native,{passed:878,failed:0,ignored:26});
for(const file of ['clippy.log','msrv.log','i686.log'])assert.match(fs.readFileSync(dir+'/'+file,'utf8'),/Finished `dev` profile/);
const probe=read(dir+'/source-probe-result.json');assert.equal(read(dir+'/source-probe.json').expected,42);assert.equal(probe.expected,42);assert.equal(probe.validated_passes,2);
const rejected=read(dir+'/rejected-worker-info.json'),failure=read(dir+'/rejected-probe-result.json');assert.equal(rejected.invalid_snapshot,true);assert.equal(rejected.attributed_source,null);assert.equal(rejected.source_clean,false);assert.equal(failure.status,1);assert.equal(failure.qualification,false);assert(failure.stderr.includes('expected 42, got Number(0.0)'));
console.log(JSON.stringify({integrity:'pass',source:e.source,native:e.native,oracles:36,source_probe:'pass',rejected_snapshot:'unqualified',scope:e.scope}));
