import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {fileURLToPath} from 'node:url';
import {gunzipSync} from 'node:zlib';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..');
const dir=root+'/key-and-result-diagnostic-v1',read=p=>JSON.parse(fs.readFileSync(p)),sha=b=>createHash('sha256').update(b).digest('hex');
const e=read(dir+'/evidence.json');
assert.equal(e.nodes.length,26);
for(const [file,hash]of Object.entries(e.files)){
 assert(!path.isAbsolute(file)&&!file.split('/').includes('..'));
 assert.equal(sha(fs.readFileSync(dir+'/'+file)),hash,file);
}
for(const node of e.nodes){
 const info=read(dir+'/'+node.name+'/worker-info.json');assert.equal(info.revision,node.source);assert.equal(info.profile,'release');assert.deepEqual(info.features,[]);assert.equal(info.provenance.source_clean,true);assert.equal(info.provenance.source_inputs.length,['short-functions','metadata-observers','stable-roots','outlined-strings','deferred-arrays','owned-sites','weak-environments','literal-accessors','weak-objects','hot-sites','reference-updates','direct-region','catch-records','resolved-hints','active-handles'].includes(node.name)?82:['empty-frame','error-strings','activation-fingerprints','inline-stacks','inline-auxiliary'].includes(node.name)?81:80);
 for(const input of info.provenance.source_inputs)assert.equal(sha(execFileSync('git',['-c','safe.directory='+repo,'show',node.source+':'+input.file],{cwd:repo})),input.sha256,input.file);
 if(node.name==='catch-records'){
  const v=read(dir+'/'+node.name+'/validation.json');assert.equal(v.source,node.source);assert.equal(v.miri.status,'pass');
  assert.equal(v.miri.toolchain,'nightly-2026-10-20');assert.deepEqual(v.miri.flags,['-Zmiri-strict-provenance','-Zmiri-symbolic-alignment-check']);
  assert(fs.readFileSync(dir+'/'+node.name+'/miri.log','utf8').includes('test result: ok. 1 passed; 0 failed; 0 ignored;'));
 }
 if(node.name==='direct-region'){
  const v=read(dir+'/'+node.name+'/validation.json');assert.equal(v.source,node.source);assert.equal(v.miri.status,'pass');
  assert.equal(v.miri.toolchain,'nightly-2026-10-20');assert.deepEqual(v.miri.flags,['-Zmiri-strict-provenance','-Zmiri-symbolic-alignment-check']);
  assert(fs.readFileSync(dir+'/'+node.name+'/miri.log','utf8').includes('test result: ok. 6 passed; 0 failed; 0 ignored;'));
 }
 if(node.name==='reference-updates'){
  const v=read(dir+'/'+node.name+'/validation.json');assert.equal(v.source,node.source);assert.equal(v.miri.status,'pass');
  assert.equal(v.miri.toolchain,'nightly-2026-10-20');assert.deepEqual(v.miri.flags,['-Zmiri-strict-provenance','-Zmiri-symbolic-alignment-check']);
  assert.equal(sha(execFileSync('git',['-c','safe.directory='+repo,'show',node.source+':'+v.miri.integration_source.file],{cwd:repo})),v.miri.integration_source.sha256);
  assert(fs.readFileSync(dir+'/'+node.name+'/miri.log','utf8').includes('test result: ok. 1 passed; 0 failed; 0 ignored;'));
 }
 if(node.name==='hot-sites'){
  const validation=read(dir+'/'+node.name+'/validation.json');assert.equal(validation.source,node.source);
  assert.equal(validation.miri.toolchain,'nightly-2026-10-20');
  assert.deepEqual(validation.miri.flags,['-Zmiri-strict-provenance','-Zmiri-symbolic-alignment-check']);
  assert(fs.readFileSync(dir+'/'+node.name+'/miri.log','utf8').includes('test result: ok. 3 passed; 0 failed; 0 ignored;'));
 }
 if(node.name==='weak-objects'){
  const decision=read(dir+'/'+node.name+'/decision.json');
  assert.equal(decision.source,node.source);assert.equal(decision.decision,'rejected and reverted');
  assert.equal(decision.miri.toolchain,'nightly-2026-10-20');
  assert.deepEqual(decision.miri.flags,['-Zmiri-strict-provenance','-Zmiri-symbolic-alignment-check']);
  assert(fs.readFileSync(dir+'/'+node.name+'/miri.log','utf8').includes('test result: ok. 1 passed; 0 failed; 0 ignored;'));
  for(const name of ['done-value-order','invalid-next-result'])assert(decision.profiles.find(p=>p.name===name).change_percent>0);
 }
 if(node.name==='literal-accessors'){
  const m=read(dir+'/'+node.name+'/focused-miri.json');assert.equal(m.source,node.source);assert.equal(m.toolchain,'nightly-2026-10-20');
  assert.deepEqual(m.flags,['-Zmiri-strict-provenance','-Zmiri-symbolic-alignment-check']);assert.deepEqual(m.checks.map(c=>c.passed),[2,1,1]);
  assert.equal(sha(execFileSync('git',['-c','safe.directory='+repo,'show',node.source+':'+m.integration_source.file],{cwd:repo})),m.integration_source.sha256);
  for(const c of m.checks)assert(fs.readFileSync(dir+'/'+node.name+'/'+c.file,'utf8').includes(`test result: ok. ${c.passed} passed; 0 failed; 0 ignored;`));
 }
 assert.equal(node.profiles.length,({'definition-cache':3,'property-keys':3,'iterator-results':3,'environment-roots':3,'native-errors':1,'native-step':6,'empty-frame':6,'error-strings':3,'activation-fingerprints':6,'inline-stacks':6,'inline-auxiliary':6,'short-functions':6,'metadata-observers':6,'stable-roots':6,'outlined-strings':6,'deferred-arrays':6,'owned-sites':6,'weak-environments':6,'literal-accessors':6,'weak-objects':6,'hot-sites':6,'reference-updates':6,'direct-region':6,'catch-records':6,'resolved-hints':6,'active-handles':6})[node.name]);
 for(const p of node.profiles){
  const base=dir+'/'+node.name+'/'+p.name,raw=gunzipSync(fs.readFileSync(base+'.callgrind.gz')).toString();
  assert.equal(Number(raw.match(/^summary: (\d+)$/m)?.[1]),p.instructions);
  assert.equal(Number(raw.match(/^totals: (\d+)$/m)?.[1]),p.instructions);
  const output=read(base+'-worker.json');assert.equal(output.expected,50005000);assert.equal(output.validated_passes,2);
 }
}
console.log(JSON.stringify({integrity:'pass',nodes:e.nodes,scope:e.scope}));
