// Instruction diagnostics do not establish a native Boa speed qualification.
import fs from 'node:fs';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {gunzipSync} from 'node:zlib';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=fileURLToPath(new URL('./',import.meta.url)),repo=fileURLToPath(new URL('../../',import.meta.url));
const dir=root+'private-plans-diagnostic-v1',read=name=>JSON.parse(fs.readFileSync(dir+'/'+name));
const sha=bytes=>createHash('sha256').update(bytes).digest('hex'),e=read('evidence.json');
assert.equal(e.before,'709ffeb461fa184d65b3718f0b9a9d6373739e2c');
assert.equal(e.after,'622bc975af697c49298d2fb5b71b575aeed6cdb5');
assert.deepEqual(e.profiles,{'private-method-call':{before:256343432,after:129864291},'private-accessor':{before:507019863,after:177651256}});
for(const [name,hash]of Object.entries(e.files))assert.equal(sha(fs.readFileSync(dir+'/'+name)),hash,name);
for(const label of ['before','after']){
 const info=read(label+'-worker-info.json');assert.equal(info.revision,e[label]);
 assert.equal(info.provenance.source_clean,true);assert.equal(info.provenance.source_inputs.length,80);
 for(const input of info.provenance.source_inputs)assert.equal(sha(execFileSync('git',['show',e[label]+':'+input.file],{cwd:repo})),input.sha256,input.file);
 for(const [name,counts]of Object.entries(e.profiles)){
  const profile=gunzipSync(fs.readFileSync(dir+'/'+label+'-'+name+'.callgrind.gz')).toString();
  assert.equal(Number(profile.match(/^summary: (\d+)$/m)?.[1]),counts[label]);
  assert.equal(Number(profile.match(/^totals: (\d+)$/m)?.[1]),counts[label]);
  const worker=read(label+'-'+name+'-worker.json');assert.equal(worker.expected,50005000);assert.equal(worker.validated_passes,2);
 }
}
const native=fs.readFileSync(dir+'/native-tests.log','utf8');assert(!native.includes('test result: FAILED'));
const tests=[...native.matchAll(/test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored/g)];
assert.equal(tests.reduce((n,r)=>n+Number(r[1]),0),870);assert(tests.every(r=>Number(r[2])===0));assert.equal(tests.reduce((n,r)=>n+Number(r[3]),0),26);
assert.deepEqual(e.native,{passed:870,failed:0,ignored:26});
for(const name of ['clippy.log','msrv.log','i686.log'])assert.match(fs.readFileSync(dir+'/'+name,'utf8'),/Finished `dev` profile/);
const freeze=JSON.parse(fs.readFileSync(root+'workload-freeze.json'));
assert.equal(freeze.inputs.length,6);
for(const input of freeze.inputs){
 assert.equal(sha(fs.readFileSync(root+'workloads-v1/'+input.file)),input.sha256);
 const oracle=read('oracles/'+input.file);assert.equal(oracle.expected,50005000);assert.equal(oracle.validated_passes,2);
}
console.log(JSON.stringify({integrity:'pass',source:e.after,native:e.native,profiles:e.profiles,scope:e.scope}));
