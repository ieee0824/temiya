// Reconstruct function class bridge measurements without turning noisy observations into wins.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..');
const read=p=>JSON.parse(fs.readFileSync(p)),sha=b=>createHash('sha256').update(b).digest('hex');
const e=read(root+'/class-loop-capture-v1/evidence.json'),archive=root+'/class-loop-capture-v1/capture.tar.gz';
assert.equal(e.source_revision,'c654c020daca34565ad181dcb63c3bbd7e6b0b70');
assert.equal(fs.statSync(archive).size,e.archive.bytes);assert.equal(sha(fs.readFileSync(archive)),e.archive.sha256);
const temp=fs.mkdtempSync(path.join(os.tmpdir(),'temiya-derived-class-loop-capture-'));
const run=(script,args)=>JSON.parse(execFileSync(process.execPath,[repo+'/scripts/'+script,...args],{encoding:'utf8',maxBuffer:16*1024*1024,cwd:repo}));
try {
 const names=execFileSync('tar',['-tzf',archive],{encoding:'utf8'}).trim().split('\n');
 assert.equal(new Set(names).size,names.length);assert(names.every(n=>!path.isAbsolute(n)&&!n.split('/').some(p=>p==='..'||p==='.')));
 assert(execFileSync('tar',['-tvzf',archive],{encoding:'utf8'}).trim().split('\n').every(n=>n[0]==='d'||n[0]==='-'));
 execFileSync('tar',['-xzf',archive,'--no-same-owner','--no-same-permissions','-C',temp]);
 const reports=['derived-boa.json','derived-beforeafter.json','derived-same-binary.json','derived-baseline-boa.json'];
 const audit=run('audit-class-workload-records.mjs',['--derived',...reports.map(n=>temp+'/'+n)]);
 assert.deepEqual(audit.reports.map(r=>r.outcomes),[
  {'Temiya faster beyond threshold':15},
  {'within noise threshold':15},
  {'within noise threshold':15},
  {'Temiya faster beyond threshold':14,'within noise threshold':1},
 ]);
 assert.deepEqual(audit.reports.map(r=>r.outcomes),e.outcomes.derived);
 const recovered=read(temp+'/boa-build-info.json');
 assert.equal(recovered.executable_sha256,'06d4bf9bec5c298d3072903438a796499c4b111634826edb7d811c3cf984661f');
 assert.equal(recovered.recovery.identical_sha256,true);
 const baseReports=['class-boa.json','class-beforeafter.json','class-same-binary.json','class-baseline-boa.json'];
 const baseAudit=run('audit-class-workload-records.mjs',baseReports.map(n=>temp+'/'+n));
 assert.deepEqual(baseAudit.reports.map(r=>r.outcomes),[
  {'Temiya faster beyond threshold':11,'within noise threshold':1},
  {'within noise threshold':12},
  {'within noise threshold':12},
  {'Temiya faster beyond threshold':9,'within noise threshold':3},
 ]);
 assert.deepEqual(baseAudit.reports.map(r=>r.outcomes),e.outcomes.class);
 for(const name of [...reports,...baseReports]){const report=read(temp+'/'+name),baseline=name.endsWith('-baseline-boa.json');const identity=baseline?read(temp+'/baseline-worker-info.json'):{revision:e.source_revision,executable_sha256:e.worker_sha256};assert.equal(report.workers[0].revision,identity.revision);assert.equal(report.workers[0].executable_sha256,identity.executable_sha256);}
 assert(read(temp+'/derived-same-binary.json').workers.every(w=>w.executable_sha256===e.worker_sha256));
 const baseline=read(temp+'/baseline-worker-info.json');
 assert.equal(baseline.revision,'e11890a12fbd67a395dafad3e43b763e14dd29de');
 assert.equal(baseline.executable_sha256,'1081aa52e40d0fe74e337d165da0cf5176c90d356163e4ac18761e66668d2424');
 const preceding=read(root+'/class-prototype-v1/evidence.json');
 const previousArchive=root+'/class-prototype-v1/capture.tar.gz';
 assert.equal(sha(fs.readFileSync(previousArchive)),preceding.archive.sha256);
 const previous=temp+'/preceding';fs.mkdirSync(previous);
 const previousNames=execFileSync('tar',['-tzf',previousArchive],{encoding:'utf8'}).trim().split('\n');
 assert.equal(new Set(previousNames).size,previousNames.length);
 assert(previousNames.every(n=>!path.isAbsolute(n)&&!n.split('/').some(p=>p==='..'||p==='.')));
 assert(execFileSync('tar',['-tvzf',previousArchive],{encoding:'utf8'}).trim().split('\n').every(n=>n[0]==='d'||n[0]==='-'));
 execFileSync('tar',['-xzf',previousArchive,'--no-same-owner','--no-same-permissions','-C',previous]);
 for(const mode of ['ast','auto']) {
  const dir='derived-'+mode,rows=read(temp+'/'+dir+'/outcomes.json');
  assert.deepEqual(rows,read(previous+'/'+dir+'/outcomes.json'));
  for(const row of rows) {
   if(row.outcome==='pass')continue;
   const [file,pointer]=row.raw_record.split('#'),index=Number(pointer.split('/').at(-1));
   const current=read(temp+'/'+dir+'/'+file).results[index],old=read(previous+'/'+dir+'/'+file).results[index];
   for(const key of ['detail','diagnostic','observed_failure','reason','status'])assert.deepEqual(current[key],old[key],row.path+':'+key);
  }
 }
 const info=read(temp+'/engine-info.json');assert.equal(info.source_revision,e.source_revision);assert.equal(info.executable_sha256,e.engine_sha256);assert.equal(info.source_clean,true);
 for(const mode of ['ast','auto'])for(const [dir,files,variants,pass,fail] of [
  ['derived-'+mode,203,401,214,187],['base-originals/'+mode,149,294,276,18],
 ]){
  const capture=temp+'/'+dir,v=run('verify-test262-baseline.mjs',[capture]);
  assert.equal(v.files,files);assert.equal(v.variants,variants);assert.equal(v.totals.pass,pass);assert.equal(v.totals.fail,fail);
  for(const name of ['skip','timeout','not_run','unreported'])assert.equal(v.totals[name],0);
  assert.equal(read(capture+'/conditions.json').engine_sha256,e.engine_sha256);
 }
 const gate=run('verify-test262-baseline.mjs',[temp+'/pr-gate/capture']);
 assert.equal(gate.files,129);assert.equal(gate.variants,216);assert.equal(gate.totals.pass,216);assert.equal(gate.totals.fail,0);
 const rows=[...fs.readFileSync(temp+'/native-all-tests.log','utf8').matchAll(/test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored/g)];
 assert.equal(rows.length,98);assert.equal(rows.reduce((n,r)=>n+ +r[1],0),778);assert(rows.every(r=>+r[2]===0));assert.equal(rows.reduce((n,r)=>n+ +r[3],0),26);
 console.log(JSON.stringify({integrity:'pass',source:e.source_revision,performance:audit.reports.map(r=>r.outcomes),base_performance:baseAudit.reports.map(r=>r.outcomes),variants_per_backend:401,upstream_pass:214,upstream_fail:187,pr_gate_pass:216,scope:e.scope}));
} finally {fs.rmSync(temp,{recursive:true,force:true});}
