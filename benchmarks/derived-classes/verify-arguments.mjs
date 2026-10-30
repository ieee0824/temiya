// Replay historical arguments-elision evidence independently of its summaries.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..');
const read=p=>JSON.parse(fs.readFileSync(p)),sha=b=>createHash('sha256').update(b).digest('hex');
const e=read(root+'/arguments-v1/evidence.json'),archive=root+'/arguments-v1/capture.tar.gz';
assert.equal(e.source_revision,'f1ed7c0ab3260de78cbf169651e5093ca8239aa0');
assert.equal(fs.statSync(archive).size,e.archive.bytes);assert.equal(sha(fs.readFileSync(archive)),e.archive.sha256);
const temp=fs.mkdtempSync(path.join(os.tmpdir(),'temiya-derived-arguments-'));
const run=(script,args)=>JSON.parse(execFileSync(process.execPath,[repo+'/scripts/'+script,...args],{encoding:'utf8',maxBuffer:16*1024*1024,cwd:repo}));
try {
 const names=execFileSync('tar',['-tzf',archive],{encoding:'utf8'}).trim().split('\n');
 assert.equal(new Set(names).size,names.length);assert(names.every(n=>!path.isAbsolute(n)&&!n.split('/').some(p=>p==='..'||p==='.')));
 assert(execFileSync('tar',['-tvzf',archive],{encoding:'utf8'}).trim().split('\n').every(n=>n[0]==='d'||n[0]==='-'));
 execFileSync('tar',['-xzf',archive,'--no-same-owner','--no-same-permissions','-C',temp]);
 const reports=['derived-boa.json','derived-beforeafter.json','derived-same-binary.json'];
 const audit=run('audit-class-workload-records.mjs',['--derived',...reports.map(n=>temp+'/'+n)]);
 assert.deepEqual(audit.reports.map(r=>r.outcomes),[
  {'Temiya faster beyond threshold':15},
  {'after faster beyond threshold':3,'within noise threshold':12},
  {'within noise threshold':15},
 ]);
 for(const name of reports){const report=read(temp+'/'+name);assert.equal(report.workers[0].revision,e.source_revision);assert.equal(report.workers[0].executable_sha256,e.worker_sha256);}
 assert(read(temp+'/derived-same-binary.json').workers.every(w=>w.executable_sha256===e.worker_sha256));
 const info=read(temp+'/engine-info.json');assert.equal(info.source_revision,e.source_revision);assert.equal(info.executable_sha256,e.engine_sha256);assert.equal(info.source_clean,true);
 for(const mode of ['ast','auto'])for(const [dir,files,variants,pass,fail] of [
  ['derived-'+mode,203,401,208,193],['base-originals/'+mode,149,294,276,18],
 ]){
  const capture=temp+'/'+dir,v=run('verify-test262-baseline.mjs',[capture]);
  assert.equal(v.files,files);assert.equal(v.variants,variants);assert.equal(v.totals.pass,pass);assert.equal(v.totals.fail,fail);
  for(const name of ['skip','timeout','not_run','unreported'])assert.equal(v.totals[name],0);
  assert.equal(read(capture+'/conditions.json').engine_sha256,e.engine_sha256);
 }
 const gate=run('verify-test262-baseline.mjs',[temp+'/pr-gate/capture']);
 assert.equal(gate.files,129);assert.equal(gate.variants,216);assert.equal(gate.totals.pass,216);assert.equal(gate.totals.fail,0);
 const rows=[...fs.readFileSync(temp+'/native-all-tests.log','utf8').matchAll(/test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored/g)];
 assert.equal(rows.length,97);assert.equal(rows.reduce((n,r)=>n+ +r[1],0),757);assert(rows.every(r=>+r[2]===0));assert.equal(rows.reduce((n,r)=>n+ +r[3],0),26);
 console.log(JSON.stringify({integrity:'pass',source:e.source_revision,performance:audit.reports.map(r=>r.outcomes),variants_per_backend:401,upstream_pass:208,upstream_fail:193,pr_gate_pass:216,scope:e.scope}));
} finally {fs.rmSync(temp,{recursive:true,force:true});}
