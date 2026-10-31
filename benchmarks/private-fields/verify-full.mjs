// Replay completed candidate captures without merging their source identities.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..');
const read=p=>JSON.parse(fs.readFileSync(p));
const captures=new Map([['full-bound-constructor-v1',{source:'072f6315ab26b4c11b3a023fd6de68c48d7632cc',run:41168380143,historicalFailure:false}],['full-bound-private-v1',{source:'a65fa12bb7123b1af0e0a2b2254411ee7170c3e8',run:41162604053,historicalFailure:false}],['full-default-private-v1',{source:'e3eb58767719aef346140d49771891807048ec37',run:41158820135,historicalFailure:false}]]);
const selected=process.argv.slice(2);if(selected.length===0)selected.push(...captures.keys());
assert.equal(new Set(selected).size,selected.length);
for(const name of selected){
 assert(captures.has(name),'unknown retained capture');
 const expected=captures.get(name),dir=root+'/'+name,e=read(dir+'/evidence.json');
 assert.equal(e.source_revision,expected.source);assert.equal(e.run,expected.run);
 assert.equal(e.archive.file,'capture.tar.gz');
 const archive=dir+'/'+e.archive.file,bytes=fs.readFileSync(archive);
 assert.equal(bytes.length,e.archive.bytes);assert.equal(createHash('sha256').update(bytes).digest('hex'),e.archive.sha256);
 const temp=fs.mkdtempSync(path.join(os.tmpdir(),'temiya-derived-full-'));
 try {
  const names=execFileSync('tar',['-tzf',archive],{encoding:'utf8',maxBuffer:16*1024*1024}).trim().split('\n');
  assert.equal(new Set(names).size,names.length);
  assert(names.every(n=>!path.isAbsolute(n)&&!n.split('/').some(p=>p==='..'||p==='.')));
  assert(execFileSync('tar',['-tvzf',archive],{encoding:'utf8',maxBuffer:16*1024*1024}).trim().split('\n').every(n=>n[0]==='d'||n[0]==='-'));
  execFileSync('tar',['-xzf',archive,'--no-same-owner','--no-same-permissions','-C',temp]);
  fs.copyFileSync(dir+'/policy.json',temp+'/policy.json');
  const reference=temp+'/reference';
  execFileSync(process.execPath,[repo+'/scripts/verify-test262-archive.mjs',repo+'/benchmarks/test262-feature-baseline',reference],{stdio:'pipe'});
  const result=JSON.parse(execFileSync(process.execPath,[expected.historicalFailure?repo+'/benchmarks/super-properties/replay-failed-full.mjs':repo+'/scripts/audit-full-candidate.mjs',temp,reference,temp+'/policy.json',e.source_revision,String(e.run)],{encoding:'utf8',cwd:repo,maxBuffer:1024*1024}));
  assert.equal(result.files,52133);assert.equal(result.variants,100211);
  assert.equal(result.failures.length,expected.historicalFailure?1:0);if(expected.historicalFailure){assert.deepEqual(result.failures,e.knownpass_regressions);assert.equal(result.gate,'historical failure retained:7 gates passed, namespace membership regression in shard3');}else{assert.deepEqual(result.failures,[]);assert.equal(result.gate,'all8 independently replayed known-pass gates passed');}
  assert.deepEqual(result.totals,e.totals);
  const actual=read(temp+'/independent-candidate-audit.json');assert.deepEqual(actual,read(dir+'/independent-audit.json'));
  for(const row of actual.results){
   assert.equal(row.engine.profile,'release');assert.deepEqual(row.engine.features,[]);
   assert.match(row.engine.executable_sha256,/^[a-f0-9]{64}$/);
   assert(row.engine.rustc.includes('host: x86_64-unknown-linux-gnu'));
  }
  console.log(JSON.stringify({integrity:'pass',capture:name,source:e.source_revision,run:e.run,files:result.files,variants:result.variants,totals:result.totals,knownpass_regressions:result.failures.length,scope:e.scope}));
 } finally {fs.rmSync(temp,{recursive:true,force:true});}
}
