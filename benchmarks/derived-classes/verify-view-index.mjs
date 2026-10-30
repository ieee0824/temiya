// Retain the address-width validation and its completed/interrupted native controls.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..');
const read=p=>JSON.parse(fs.readFileSync(p)),sha=b=>createHash('sha256').update(b).digest('hex');
const e=read(root+'/native-view-index-v1/evidence.json'),archive=root+'/native-view-index-v1/capture.tar.gz';
assert.equal(e.source_revision,'2be3d7c82091d27f7d2fae58a346c44144d3666b');
const bytes=fs.readFileSync(archive);assert.equal(bytes.length,e.archive.bytes);assert.equal(sha(bytes),e.archive.sha256);
const temp=fs.mkdtempSync(path.join(os.tmpdir(),'temiya-view-index-'));
const run=(script,args)=>JSON.parse(execFileSync(process.execPath,[repo+'/scripts/'+script,...args],{encoding:'utf8',cwd:repo,maxBuffer:16*1024*1024}));
try {
 const names=execFileSync('tar',['-tzf',archive],{encoding:'utf8'}).trim().split('\n');
 assert.equal(new Set(names).size,names.length);assert(names.every(n=>!path.isAbsolute(n)&&!n.split('/').some(p=>p==='..'||p==='.')));
 assert(execFileSync('tar',['-tvzf',archive],{encoding:'utf8'}).trim().split('\n').every(n=>n[0]==='d'||n[0]==='-'));
 execFileSync('tar',['-xzf',archive,'--no-same-owner','--no-same-permissions','-C',temp]);
 const info=read(temp+'/engine-info.json');assert.equal(info.source_revision,e.source_revision);assert.equal(info.executable_sha256,e.engine_sha256);assert.equal(info.source_clean,true);
 const equivalence=read(temp+'/binary-equivalence.json');assert.equal(equivalence.new_source,e.source_revision);
 assert.equal(equivalence.old_source,'06ceeee645204673aa06f6c407fa2cfa9e536c17');
 for(const row of equivalence.rows){assert.equal(row.old_sha256,row.new_sha256);assert.equal(row.byte_equal,true);assert.equal(row.new_sha256,row.name==='engine'?e.engine_sha256:e.worker_sha256);}
 const rejected=read(temp+'/x64-engine-identity-check.json');assert.equal(rejected.byte_identity_established,false);assert.notEqual(rejected.old_sha256,rejected.new_sha256);
 const interrupted=read(temp+'/interrupted-native-controls/interrupted.json');assert.equal(interrupted.status,'interrupted');assert.equal(interrupted.exit_code,143);
 assert(fs.readFileSync(temp+'/controls/original-beforeafter.json').equals(fs.readFileSync(temp+'/interrupted-native-controls/original-beforeafter.json')));
 assert.equal(fs.statSync(temp+'/interrupted-native-controls/original-boa.json').size,0);
 const results=[];
 for(const [suite,script] of [['original','audit-original-workload-records.mjs'],['class','audit-class-workload-records.mjs']]){
  const reports=['beforeafter','boa','baseline-boa','same-binary'].map(n=>suite+'-'+n+'.json');
  const audit=run(script,reports.map(n=>temp+'/controls/'+n)),saved=read(temp+'/controls/'+suite+'-audit.json');
  const comparable=r=>({sha256:r.sha256,outcomes:r.outcomes,runs:r.runs,checked_values:r.checked_values,comparisons:r.comparisons});
  assert.deepEqual(audit.reports.map(comparable),saved.reports.map(comparable));
  for(const [index,name] of reports.entries()){
   const r=read(temp+'/controls/'+name),first=r.workers[0];assert.equal(r.machine.architecture,'arm64');assert.equal(r.machine.cpu_affinity,'0');
   assert.equal(first.revision,index===2?'f1ed7c0ab3260de78cbf169651e5093ca8239aa0':suite==='original'&&index===0?equivalence.old_source:e.source_revision);
   if(index!==2)assert.equal(first.executable_sha256,e.worker_sha256);
   if(index===3)assert(r.workers.every(w=>w.executable_sha256===e.worker_sha256));
  }
  results.push({suite,outcomes:audit.reports.map(r=>r.outcomes)});
 }
 for(const mode of ['ast','auto'])for(const [dir,files,variants,pass,fail] of [['derived-'+mode,203,401,214,187],['base-originals/'+mode,149,294,276,18]]){
  const capture=temp+'/'+dir,v=run('verify-test262-baseline.mjs',[capture]);assert.equal(v.files,files);assert.equal(v.variants,variants);assert.equal(v.totals.pass,pass);assert.equal(v.totals.fail,fail);
  for(const key of ['skip','timeout','not_run','unreported'])assert.equal(v.totals[key],0);
  assert.equal(read(capture+'/conditions.json').engine_sha256,e.engine_sha256);
 }
 const gate=run('verify-test262-baseline.mjs',[temp+'/pr-gate/capture']);assert.equal(gate.variants,216);assert.equal(gate.totals.pass,216);
 const rows=[...fs.readFileSync(temp+'/native-all-tests.log','utf8').matchAll(/test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored/g)];
 assert.equal(rows.length,97);assert.equal(rows.reduce((n,r)=>n+ +r[1],0),766);assert(rows.every(r=>+r[2]===0));assert.equal(rows.reduce((n,r)=>n+ +r[3],0),26);
 console.log(JSON.stringify({integrity:'pass',source:e.source_revision,reports:8,results,interrupted_report_counted_as_pass:false,x64_equivalence:false,scope:e.scope}));
} finally {fs.rmSync(temp,{recursive:true,force:true});}
