import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const dir=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(dir,'../../..'),read=p=>JSON.parse(fs.readFileSync(p));
const sha=b=>createHash('sha256').update(b).digest('hex'),e=read(dir+'/evidence.json');
assert.equal(e.source,'2c09b52f7b9692ea4781932bc923ecf0ff60a3c9');assert.equal(e.run,41285427317);
for(const [file,hash]of Object.entries(e.files)){assert.equal(path.basename(file),file);assert.equal(sha(fs.readFileSync(dir+'/'+file)),hash);}
const temp=fs.mkdtempSync(path.join(os.tmpdir(),'temiya-object-inline-full-'));
try{
 const reference=temp+'/reference';execFileSync(process.execPath,[repo+'/scripts/verify-test262-archive.mjs',repo+'/benchmarks/test262-feature-baseline',reference],{cwd:repo});
 const captures=[{name:'candidate',base:dir,meta:e,policy:'policy.json',archive:'capture.tar.gz'},...[
  ['array','benchmarks/array-binding/full-inline-thrown-v1'],['previous','benchmarks/object-binding/full-initial-catch-v1']
 ].map(([name,rel])=>{const base=repo+'/'+rel,meta=read(base+'/evidence.json');return{name,base,meta,policy:'policy.json',archive:meta.archive?.file??'capture.tar.gz'};})];
 const maps=new Map();
 for(const c of captures){
  const archive=c.base+'/'+c.archive;assert.equal(sha(fs.readFileSync(archive)),c.meta.archive?.sha256??c.meta.files[c.archive]);
  const names=execFileSync('tar',['-tzf',archive],{encoding:'utf8',maxBuffer:16*1024*1024}).trim().split('\n');assert.equal(new Set(names).size,names.length);assert(names.every(n=>!path.isAbsolute(n)&&!n.split('/').includes('..')));
  assert(execFileSync('tar',['-tvzf',archive],{encoding:'utf8',maxBuffer:16*1024*1024}).trim().split('\n').every(n=>['d','-'].includes(n[0])));
  const out=temp+'/'+c.name;fs.mkdirSync(out);execFileSync('tar',['-xzf',archive,'--no-same-owner','--no-same-permissions','-C',out]);
  const audit=JSON.parse(execFileSync(process.execPath,[repo+'/scripts/audit-full-candidate.mjs',out,reference,c.base+'/'+c.policy,c.meta.source,String(c.meta.run)],{cwd:repo,encoding:'utf8',maxBuffer:1024*1024}));
  assert.equal(audit.variants,100211);assert.deepEqual(audit.failures,[]);
  if(c.name==='candidate')assert.deepEqual(read(out+'/independent-candidate-audit.json'),read(dir+'/independent-candidate-audit.json'));
  const map=new Map();for(let i=0;i<8;i++)for(const row of read(`${out}/test262-full-${i}-of-8/test262-regression/capture/outcomes.json`)){const key=JSON.stringify([row.path,row.variant]);assert(!map.has(key));map.set(key,row.outcome);}assert.equal(map.size,100211);maps.set(c.name,map);
 }
 const after=maps.get('candidate');
 for(const [name,file,expected]of [['array','array-parent-comparison.json',33565],['previous','previous-object-comparison.json',37919]]){
  let preserved=0;const regressions=[],newpasses=[];for(const [key,b]of maps.get(name)){assert(after.has(key));const a=after.get(key);if(b==='pass'){if(a==='pass')preserved++;else regressions.push({key:JSON.parse(key),before:b,after:a});}else if(a==='pass')newpasses.push(JSON.parse(key));}
  const saved=read(dir+'/'+file);assert.equal(saved.previous_source,captures.find(c=>c.name===name).meta.source);assert.equal(saved.source,e.source);assert.equal(saved.variants,100211);assert.equal(preserved,expected);assert.equal(saved.preserved_passes,preserved);assert.deepEqual(regressions,[]);assert.deepEqual(saved.regressions,regressions);assert.deepEqual(saved.newpasses,newpasses);
  console.log(JSON.stringify({integrity:'pass',comparison:name,preserved,newpasses:newpasses.length,regressions:0,scope:e.scope}));
 }
}finally{fs.rmSync(temp,{recursive:true,force:true});}
