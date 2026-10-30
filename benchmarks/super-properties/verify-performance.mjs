// Replay intermediate slow and noisy records without converting them into wins.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..');
const read=p=>JSON.parse(fs.readFileSync(p)),sha=b=>createHash('sha256').update(b).digest('hex');
const run=(script,args)=>JSON.parse(execFileSync(process.execPath,[repo+'/scripts/'+script,...args],{encoding:'utf8',cwd:repo,maxBuffer:8*1024*1024}));
const temp=fs.mkdtempSync(path.join(os.tmpdir(),'temiya-super-performance-'));
function extract(name){
 const dir=root+'/'+name,e=read(dir+'/evidence.json'),archive=dir+'/capture.tar.gz',bytes=fs.readFileSync(archive);
 assert.equal(bytes.length,e.archive.bytes);assert.equal(sha(bytes),e.archive.sha256);
 const names=execFileSync('tar',['-tzf',archive],{encoding:'utf8'}).trim().split('\n');
 assert.equal(new Set(names).size,names.length);assert(names.every(n=>!path.isAbsolute(n)&&!n.split('/').some(p=>p==='.'||p==='..')));
 assert(execFileSync('tar',['-tvzf',archive],{encoding:'utf8'}).trim().split('\n').every(n=>n[0]==='d'||n[0]==='-'));
 const dest=temp+'/'+name;fs.mkdirSync(dest);execFileSync('tar',['-xzf',archive,'--no-same-owner','--no-same-permissions','-C',dest]);return {dest,e};
}
try{
 const baseline=extract('dependencies-v1');assert.equal(baseline.e.source_revision,'b1557a52fcd5b574a90dfa31ba7b65d363c31879');
 let previous=baseline.dest,previousPrefix='capture-',previousRevision=baseline.e.source_revision;const summaries=[];
 for(const [phase,revision,native,expected]of [
  ['class-bridge-v1','68bf209e09a0d6a79831a46a3738a3b663dd578b',789,[{'within noise threshold':6,'Temiya slower beyond threshold':9},{'within noise threshold':15},{'within noise threshold':15},{'within noise threshold':6,'Temiya slower beyond threshold':9}]],
  ['ast-arguments-v1','23a809c4c639f95ca8b09371111f50ffbc0dffa7',790,[{'within noise threshold':6,'Temiya faster beyond threshold':3,'Temiya slower beyond threshold':6},{'after faster beyond threshold':11,'within noise threshold':4},{'within noise threshold':15},{'within noise threshold':6,'Temiya slower beyond threshold':9}]],
  ['sloppy-leaf-v1','a4c25776352b6edf205441e64a42bf0c10ebb762',791,[{'within noise threshold':9,'Temiya faster beyond threshold':3,'Temiya slower beyond threshold':3},{'within noise threshold':4,'after faster beyond threshold':11},{'within noise threshold':15},{'within noise threshold':6,'Temiya faster beyond threshold':3,'Temiya slower beyond threshold':6}]],
 ]){
  const {dest,e}=extract(phase);assert.equal(e.source_revision,revision);const w=read(dest+'/worker-info.json'),engine=read(dest+'/engine-info.json');assert.equal(w.revision,revision);assert.equal(w.executable_sha256,e.worker_sha256);assert.equal(engine.source_revision,revision);assert.equal(engine.executable_sha256,e.engine_sha256);assert.equal(engine.source_clean,true);
  const names=['boa','beforeafter','same-binary','baseline-boa'],a=run('audit-class-workload-records.mjs',['--super',...names.map(n=>dest+'/super-'+n+'.json')]);assert.deepEqual(a.reports.map(r=>r.outcomes),expected);assert.deepEqual(e.outcomes,expected);
  const before=read(dest+'/baseline-worker-info.json');assert.equal(before.revision,previousRevision);
  for(const name of names){const r=read(dest+'/super-'+name+'.json'),identity=name==='baseline-boa'?before:w;assert.equal(r.workers[0].revision,identity.revision);assert.equal(r.workers[0].executable_sha256,identity.executable_sha256);if(name==='beforeafter')assert.equal(r.workers[1].executable_sha256,before.executable_sha256);if(name==='same-binary')assert(r.workers.every(v=>v.executable_sha256===w.executable_sha256));}
  for(const mode of ['ast','auto']){
   const dir=dest+'/super-'+mode,v=run('verify-test262-baseline.mjs',[dir]);assert.equal(v.files,310);assert.equal(v.variants,601);assert.equal(v.totals.pass,405);assert.equal(v.totals.fail,196);for(const key of ['skip','timeout','not_run','unreported'])assert.equal(v.totals[key],0);assert.equal(read(dir+'/conditions.json').engine_sha256,e.engine_sha256);
   const old=previous+'/'+previousPrefix+mode,rows=read(dir+'/outcomes.json');assert.deepEqual(rows,read(old+'/outcomes.json'));
   for(const row of rows.filter(r=>r.outcome!=='pass')){const[file,pointer]=row.raw_record.split('#'),index=Number(pointer.split('/').at(-1)),current=read(dir+'/'+file).results[index],prior=read(old+'/'+file).results[index];for(const key of ['detail','diagnostic','observed_failure','reason','status'])assert.deepEqual(current[key],prior[key],row.path+':'+key);}
  }
  if(phase==='sloppy-leaf-v1'){
   for(const mode of ['ast','auto'])for(const [dir,files,variants,pass,fail]of [['derived-'+mode,203,401,264,137],['base-originals/'+mode,149,294,276,18]]){const capture=dest+'/'+dir,v=run('verify-test262-baseline.mjs',[capture]);assert.equal(v.files,files);assert.equal(v.variants,variants);assert.equal(v.totals.pass,pass);assert.equal(v.totals.fail,fail);for(const key of ['skip','timeout','not_run','unreported'])assert.equal(v.totals[key],0);assert.equal(read(capture+'/conditions.json').engine_sha256,e.engine_sha256);}
   const gate=run('verify-test262-baseline.mjs',[dest+'/pr-gate/capture']);assert.equal(gate.files,129);assert.equal(gate.variants,216);assert.equal(gate.totals.pass,216);assert.equal(gate.totals.fail,0);
  }
  const rows=[...fs.readFileSync(dest+'/native-all-tests.log','utf8').matchAll(/test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored/g)];assert.equal(rows.length,99);assert.equal(rows.reduce((n,r)=>n+ +r[1],0),native);assert(rows.every(r=>+r[2]===0));assert.equal(rows.reduce((n,r)=>n+ +r[3],0),26);
  summaries.push({source:revision,outcomes:expected,variants_per_backend:601,pass:405,fail:196,scope:e.scope});previous=dest;previousPrefix='super-';previousRevision=revision;
 }
 console.log(JSON.stringify({integrity:'pass',reports:summaries,scope:'Intermediate negative or incomplete speed evidence, not final qualification.'}));
}finally{fs.rmSync(temp,{recursive:true,force:true});}
