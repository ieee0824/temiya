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
const captures=new Map([
 ['full-class-loop-runtime-v1',{source:'a10e7a389e76faca5c8d38f5ab4619d8d678da05',run:41105090864}],
 ['full-class-prototype-v1',{source:'e11890a12fbd67a395dafad3e43b763e14dd29de',run:41090356693}],
 ['full-class-bridge-v1',{source:'306c32c24a6c678276284c6ff898713137447370',run:41083660571}],
 ['full-function-metadata-v1',{source:'43d3ff7a8c2203c71750409ff20442cc3344f056',run:41069429383}],
 ['full-anonymous-env-v1',{source:'13a91d105f205aeb2f0b5b296709702fa577082e',run:41062837342}],
 ['full-native-prototypes-v1',{source:'336a38dbc74242a107761680bed5c1f429b602a1',run:41044387471}],
 ['full-view-index-v1',{source:'2be3d7c82091d27f7d2fae58a346c44144d3666b',run:41047319155}],
 ['full-delegation-v1',{source:'8b019733ed8e08d3bb8a31319501a4d3c08bf070',run:41053545979}],
]);
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
  const result=JSON.parse(execFileSync(process.execPath,[repo+'/scripts/audit-full-candidate.mjs',temp,reference,temp+'/policy.json',e.source_revision,String(e.run)],{encoding:'utf8',cwd:repo,maxBuffer:1024*1024}));
  assert.equal(result.files,52133);assert.equal(result.variants,100211);
  assert.equal(result.failures.length,0);assert.equal(result.gate,'all8 independently replayed known-pass gates passed');
  assert.deepEqual(result.totals,e.totals);
  const actual=read(temp+'/independent-candidate-audit.json');assert.deepEqual(actual,read(dir+'/independent-audit.json'));
  for(const row of actual.results){
   assert.equal(row.engine.profile,'release');assert.deepEqual(row.engine.features,[]);
   assert.match(row.engine.executable_sha256,/^[a-f0-9]{64}$/);
   assert(row.engine.rustc.includes('host: x86_64-unknown-linux-gnu'));
  }
  console.log(JSON.stringify({integrity:'pass',capture:name,source:e.source_revision,run:e.run,files:result.files,variants:result.variants,totals:result.totals,knownpass_regressions:0,scope:e.scope}));
 } finally {fs.rmSync(temp,{recursive:true,force:true});}
}
