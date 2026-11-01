import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const dir=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(dir,'../../..'),read=p=>JSON.parse(fs.readFileSync(p));
const sha=b=>createHash('sha256').update(b).digest('hex'),meta=read(dir+'/evidence.json');
assert.equal(meta.source,'eb29c4169c0fdfd9246546f76b5874097a53ef62');assert.equal(meta.run,41303173669);
const temp=fs.mkdtempSync(path.join(os.tmpdir(),'temiya-assignment-full-'));
try {
 const reference=temp+'/reference';execFileSync(process.execPath,[repo+'/scripts/verify-test262-archive.mjs',repo+'/benchmarks/test262-feature-baseline',reference],{cwd:repo});
 const maps=new Map();
 for(const [name,base]of [['candidate',dir],['initial',path.resolve(dir,'../full-initial-v1')],['main',repo+'/benchmarks/object-binding/full-prepared-guards-v1']]){
  const e=read(base+'/evidence.json');for(const [file,hash]of Object.entries(e.files)){assert.equal(path.basename(file),file);assert.equal(sha(fs.readFileSync(base+'/'+file)),hash);}
  const archive=base+'/capture.tar.gz';const names=execFileSync('tar',['-tzf',archive],{encoding:'utf8',maxBuffer:16*1024*1024}).trim().split('\n');assert.equal(new Set(names).size,names.length);assert(names.every(n=>!path.isAbsolute(n)&&!n.split('/').includes('..')));assert(execFileSync('tar',['-tvzf',archive],{encoding:'utf8',maxBuffer:16*1024*1024}).trim().split('\n').every(n=>['d','-'].includes(n[0])));
  const out=temp+'/'+name;fs.mkdirSync(out);execFileSync('tar',['-xzf',archive,'--no-same-owner','--no-same-permissions','-C',out]);
  const result=JSON.parse(execFileSync(process.execPath,[repo+'/scripts/audit-full-candidate.mjs',out,reference,base+'/policy.json',e.source,String(e.run)],{cwd:repo,encoding:'utf8',maxBuffer:1024*1024}));assert.equal(result.variants,100211);assert.deepEqual(result.failures,[]);assert.deepEqual(read(out+'/independent-candidate-audit.json'),read(base+'/independent-candidate-audit.json'));
  const map=new Map();for(let i=0;i<8;i++)for(const r of read(`${out}/test262-full-${i}-of-8/test262-regression/capture/outcomes.json`)){const key=JSON.stringify([r.path,r.variant]);assert(!map.has(key));map.set(key,r.outcome);}assert.equal(map.size,100211);maps.set(name,map);
 }
 const next=maps.get('candidate'),saved=read(dir+'/initial-assignment-comparison.json');assert.equal(saved.complete,true);assert.equal(saved.coveredVariants,100211);assert.equal(saved.sources.after,meta.source);assert.equal(saved.sources.before,'d460123c29dfd7a250b390e9c9464552f7b330db');
 for(const [name,expected]of [['initial',38758],['main',37919]]){
  let preserved=0;const regressions=[],newPasses=[];for(const [key,old]of maps.get(name)){assert(next.has(key));const after=next.get(key),[p,variant]=JSON.parse(key);if(old==='pass'){if(after==='pass')preserved++;else regressions.push({path:p,variant,before:old,after});}else if(after==='pass')newPasses.push({path:p,variant,before:old,after});}
  assert.equal(preserved,expected);assert.deepEqual(regressions,[]);if(name==='initial'){assert.equal(saved.preservedPasses,preserved);assert.deepEqual(saved.regressions,regressions);assert.deepEqual(saved.newPasses,newPasses);}
  console.log(JSON.stringify({integrity:'pass',comparison:name,preserved,newPasses:newPasses.length,regressions:0,scope:meta.scope}));
 }
} finally { fs.rmSync(temp,{recursive:true,force:true}); }
