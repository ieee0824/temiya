import fs from 'node:fs';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
const dir=new URL('./',import.meta.url),read=f=>JSON.parse(fs.readFileSync(new URL(f,dir)));
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
for(const r of read('integrity.json')){assert.ok(!r.file.includes('/'));assert.equal(sha(fs.readFileSync(new URL(r.file,dir))),r.sha256);}
const m=read('manifest.json'),s=read('scope.json');assert.equal(m.workloads.length,8);assert.equal(s.rows.length,16);assert.equal(s.nativeQualification,false);
assert.equal(read('boa-build-info.json').omoikane_revision,'007556adccb8fb4329236932ca8175f8e8d119d2');
let temiya=0,boa=0;
for(const w of m.workloads){
 const bytes=fs.readFileSync(new URL('../workloads-v1/'+w.file,dir));assert.equal(sha(bytes),w.sha256);
 for(const engine of ['temiya','boa']){
  const stem=w.name+'.'+engine,exit=read(stem+'.exit.json');
  const row=s.rows.filter(r=>r.name===w.name&&r.engine===engine);assert.equal(row.length,1);
  assert.equal(exit.status,row[0].status);assert.equal(exit.signal,null);assert.equal(exit.error,null);
  if(exit.status===0){const result=read(stem+'.stdout');assert.equal(result.engine,engine==='temiya'?'Temiya':'omoikane vendored Boa');assert.equal(result.expected,50005000);assert.equal(result.validated_passes,2);assert.equal(row[0].valid,true);if(engine==='temiya')temiya++;else boa++;}
  else {assert.equal(engine,'boa');assert.equal(exit.status,1);assert.equal(row[0].valid,false);assert.match(fs.readFileSync(new URL(stem+'.stderr',dir),'utf8'),w.name==='parameter-env-nested-default'?/invocation: 42/:/ReferenceError: access of uninitialized binding/);}
 }
}
assert.equal(temiya,8);assert.equal(boa,3);
console.log(JSON.stringify({temiyaValid:temiya,boaValid:boa,cases:8,boaFailures:5,nativeQualification:false,scope:'Retained adverse reference observations; integrity success is not speed qualification.'}));
