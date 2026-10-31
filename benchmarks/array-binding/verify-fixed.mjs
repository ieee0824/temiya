// Retained adverse outcomes remain adverse after integrity verification.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..');
const dir=root+'/fixed-initial-ast-v1',read=p=>JSON.parse(fs.readFileSync(p));
const e=read(dir+'/evidence.json'),archive=dir+'/'+e.archive.file,bytes=fs.readFileSync(archive);
assert.equal(e.archive.file,'capture.tar.gz');assert.equal(bytes.length,e.archive.bytes);
assert.equal(createHash('sha256').update(bytes).digest('hex'),e.archive.sha256);
assert.equal(e.baseline_source,'c2bc478647815fdc013f254f5e1e72cfd804945b');
assert.equal(e.candidate_source,'208ecd71d2287269405b2b20630b73908650e8d2');
const temp=fs.mkdtempSync(path.join(os.tmpdir(),'temiya-array-binding-fixed-'));
try{
 const names=execFileSync('tar',['-tzf',archive],{encoding:'utf8',maxBuffer:16*1024*1024}).trim().split('\n');
 assert.equal(new Set(names).size,names.length);
 assert(names.every(n=>!path.isAbsolute(n)&&!n.split('/').some(p=>p==='.'||p==='..')));
 assert(execFileSync('tar',['-tvzf',archive],{encoding:'utf8',maxBuffer:16*1024*1024}).trim().split('\n').every(n=>n[0]==='d'||n[0]==='-'));
 execFileSync('tar',['-xzf',archive,'--no-same-owner','--no-same-permissions','-C',temp]);
 const maps=[];
 for(const [name,source]of [['baseline',e.baseline_source],['candidate',e.candidate_source]]){
  const capture=temp+'/'+name;
  const result=JSON.parse(execFileSync(process.execPath,[repo+'/scripts/verify-test262-baseline.mjs',capture],{encoding:'utf8',maxBuffer:1024*1024}));
  assert.equal(result.integrity,'pass');assert.equal(result.files,8806);assert.equal(result.variants,17414);
  const conditions=read(capture+'/conditions.json');assert.equal(conditions.backend_requested,'ast');
  assert.equal(conditions.provided_engine_build_info.source_revision,source);
  assert.equal(conditions.engine_sha256,conditions.provided_engine_build_info.executable_sha256);
  assert.equal(conditions.timeout_ms,1000);assert.equal(conditions.memory_mb,512);assert.equal(conditions.stack_kb,8192);
  assert.deepEqual(read(capture+'/selection.json'),read(root+'/selection.json'));
  const map=new Map();for(const row of read(capture+'/outcomes.json')){
   const key=JSON.stringify([row.path,row.variant]);assert(!map.has(key));map.set(key,row);
  }assert.equal(map.size,17414);maps.push(map);
 }
 const changes={},regressions=[],newpasses=[];
 for(const [key,a]of maps[0]){
  const b=maps[1].get(key);assert(b);const change=a.outcome+' -> '+b.outcome;changes[change]=(changes[change]??0)+1;
  if(a.outcome==='pass'&&b.outcome!=='pass')regressions.push({path:a.path,variant:a.variant,before:a.outcome,after:b.outcome});
  if(a.outcome!=='pass'&&b.outcome==='pass')newpasses.push({path:a.path,variant:a.variant,before:a.outcome});
 }
 const saved=read(dir+'/comparison.json');assert.equal(saved.backend,'Ast');assert.equal(saved.variants,17414);
 assert.deepEqual(saved.baseline_source,read(temp+'/baseline/conditions.json'));
 assert.deepEqual(saved.candidate_source,read(temp+'/candidate/conditions.json'));
 assert.deepEqual(saved.changes,changes);assert.deepEqual(saved.regressions,regressions);assert.deepEqual(saved.newpasses,newpasses);
 console.log(JSON.stringify({integrity:'pass',baseline:e.baseline_source,candidate:e.candidate_source,changes,regressions:regressions.length,newpasses:newpasses.length,scope:e.scope}));
}finally{fs.rmSync(temp,{recursive:true,force:true});}
