import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {gunzipSync} from 'node:zlib';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const dir=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(dir,'../../..');
const read=f=>JSON.parse(fs.readFileSync(dir+'/'+f)),sha=b=>createHash('sha256').update(b).digest('hex');
const e=read('evidence.json'),s=read('diagnostic-summary.json');
assert.equal(e.before_source,'6c65967973ad4de72986db751d7c7e30547251c2');assert.equal(e.after_source,'3739cd0b07d4eb11e66865b0d1b6d121d83029e0');
assert.equal(s.baseline,e.before_source);assert.equal(s.revision,e.after_source);assert.equal(e.native_qualification,false);assert.equal(s.native_qualification,false);
for(const [file,hash]of Object.entries(e.files)){assert(!path.isAbsolute(file)&&!file.split('/').includes('..'));assert.equal(sha(fs.readFileSync(dir+'/'+file)),hash,file);}
for(const label of ['before','after']){
 const w=read(label+'/worker-info.json');assert.equal(w.revision,e[label+'_source']);assert.equal(w.provenance.source_clean,true);assert.equal(w.profile,'release');assert.deepEqual(w.features,[]);assert.equal(w.provenance.source_inputs.length,83);
 for(const input of w.provenance.source_inputs)assert.equal(sha(execFileSync('git',['-c','safe.directory='+repo,'show',w.revision+':'+input.file],{cwd:repo})),input.sha256,input.file);
 for(const p of s.results){
  const raw=gunzipSync(fs.readFileSync(dir+'/'+label+'/'+p.name+'.callgrind.gz')).toString();
  assert.equal(Number(raw.match(/^summary: (\d+)$/m)?.[1]),p[label]);assert.equal(Number(raw.match(/^totals: (\d+)$/m)?.[1]),p[label]);
  const output=read(label+'/'+p.name+'-worker.json');assert.equal(output.expected,50005000);assert.equal(output.validated_passes,2);
 }
}
assert.equal(e.catch_miri.source,e.after_source);assert.equal(e.catch_miri.toolchain,'nightly-2026-10-20');assert.deepEqual(e.catch_miri.flags,['-Zmiri-strict-provenance','-Zmiri-symbolic-alignment-check']);
assert.equal(sha(execFileSync('git',['-c','safe.directory='+repo,'show',e.after_source+':'+e.catch_miri.integration_source.file],{cwd:repo})),e.catch_miri.integration_source.sha256);
assert(fs.readFileSync(dir+'/catch-miri.log','utf8').includes('test '+e.catch_miri.test+' ... ok'));
assert.equal(s.results.length,6);
for(const p of s.results){assert.equal(p.changePercent,100*(p.after/p.before-1));assert.equal(p.inputSha256,sha(execFileSync('git',['-c','safe.directory='+repo,'show',e.after_source+':benchmarks/array-binding/workloads-v1/'+p.name+'.json'],{cwd:repo})));}
const tests=fs.readFileSync(dir+'/all-targets.log','utf8'),summaries=[...tests.matchAll(/test result: ok\. (\d+) passed; 0 failed; (\d+) ignored;/g)];
assert.equal(summaries.reduce((n,m)=>n+Number(m[1]),0),926);assert.equal(summaries.reduce((n,m)=>n+Number(m[2]),0),26);assert(!tests.includes('test result: FAILED'));
for(const name of ['catch-regression','catch-miri'])assert(fs.readFileSync(dir+'/'+name+'.log','utf8').includes('test result: ok. 1 passed; 0 failed; 0 ignored;'));
assert(fs.readFileSync(dir+'/doc.log','utf8').includes('test result: ok. 1 passed; 0 failed; 0 ignored;'));
console.log(JSON.stringify({integrity:'pass',before:e.before_source,after:e.after_source,profiles:12,inputs:6,allTargets:926,catchMiri:1,nativeQualification:false,results:s.results}));
