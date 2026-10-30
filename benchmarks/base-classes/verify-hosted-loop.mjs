// Reconstruct both machines, preserving the noisy class-definition comparison.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..');
const e=JSON.parse(fs.readFileSync(path.join(root,'hosted-loop-v1/evidence.json')));
assert.equal(e.source_revision,'05d2eee7171d6f658e5a1ca383c888c3a8d660ea');
assert.equal(e.baseline_revision,'9970cd5221bf6b74ae3543c59085a40b6d6cafa0');assert.equal(e.run,41004747984);
assert.equal(e.archive.file,'reports.tar.gz');
const archive=path.join(root,'hosted-loop-v1',e.archive.file),bytes=fs.readFileSync(archive);
assert.equal(bytes.length,e.archive.bytes);assert.equal(createHash('sha256').update(bytes).digest('hex'),e.archive.sha256);
const temp=fs.mkdtempSync(path.join(os.tmpdir(),'temiya-hosted-loop-'));
try {
 const names=execFileSync('tar',['-tzf',archive],{encoding:'utf8'}).trim().split('\n');
 assert.equal(new Set(names).size,names.length);
 assert(names.every(n=>!path.isAbsolute(n)&&!n.split('/').some(p=>p==='..'||p==='.')));
 assert(execFileSync('tar',['-tvzf',archive],{encoding:'utf8'}).trim().split('\n').every(n=>n[0]==='d'||n[0]==='-'));
 execFileSync('tar',['-xzf',archive,'--no-same-owner','--no-same-permissions','-C',temp]);
 const results=[];
 for(const copy of [1,2]){
  const dir=path.join(temp,'copy-'+copy),read=n=>JSON.parse(fs.readFileSync(path.join(dir,n)));
  const audit=(name,files)=>JSON.parse(execFileSync(process.execPath,[path.join(repo,'scripts',name),...files.map(n=>path.join(dir,n))],{encoding:'utf8',maxBuffer:16*1024*1024}));
  const originals=audit('audit-original-workload-records.mjs',['beforeafter.json','boa.json','baseline-boa.json','same-binary-control.json']);
  const classes=audit('audit-class-workload-records.mjs',['class-beforeafter.json','class-boa.json','class-baseline-boa.json','class-same-binary.json']);
  assert.deepEqual(originals.reports[1].outcomes,{'Temiya faster beyond threshold':33});
  assert.deepEqual(originals.reports[3].outcomes,{'within noise threshold':33});
  assert.deepEqual(classes.reports[3].outcomes,{'within noise threshold':12});
  assert.deepEqual(classes.reports[1].outcomes,copy===1?{'Temiya faster beyond threshold':11,'within noise threshold':1}:{'Temiya faster beyond threshold':12});
  const boa=read('class-boa.json'),before=read('class-beforeafter.json'),self=read('class-same-binary.json');
  assert.equal(boa.workers[0].revision,e.source_revision);assert.equal(before.workers[1].revision,e.baseline_revision);
  assert.equal(self.workers[0].executable_sha256,self.workers[1].executable_sha256);
  assert.equal(boa.workers[0].executable_sha256,self.workers[0].executable_sha256);
  assert.equal(read('boa.json').workers[0].executable_sha256,boa.workers[0].executable_sha256);
  for(const workload of ['class-construct','class-method','class-static','class-definition']){
   const rows=boa.comparisons.filter(r=>r.workload===workload);assert.equal(rows.length,3);
   assert(rows.filter(r=>r.outcome==='Temiya faster beyond threshold').length>=2);
   assert(rows.every(r=>r.outcome!=='Temiya slower beyond threshold'));
  }
  results.push({copy,originals:originals.reports.map(r=>r.outcomes),classes:classes.reports.map(r=>r.outcomes)});
 }
 assert.deepEqual(results,e.results);
 console.log(JSON.stringify({integrity:'pass',source:e.source_revision,results,scope:e.scope}));
} finally {fs.rmSync(temp,{recursive:true,force:true});}
