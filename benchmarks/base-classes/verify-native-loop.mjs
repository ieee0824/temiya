// Reconstruct recorded observations; a noisy result remains noisy.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)), repo=path.resolve(root,'../..');
const e=JSON.parse(fs.readFileSync(path.join(root,'native-loop-v1/evidence.json')));
const archive=path.join(root,'native-loop-v1/capture.tar.gz'), bytes=fs.readFileSync(archive);
assert.equal(e.schema,1);assert.equal(e.archive.file,'capture.tar.gz');
assert.equal(bytes.length,e.archive.bytes);
assert.equal(createHash('sha256').update(bytes).digest('hex'),e.archive.sha256);
const temp=fs.mkdtempSync(path.join(os.tmpdir(),'temiya-loop-evidence-'));
try {
  const listing=execFileSync('tar',['-tzf',archive],{encoding:'utf8'}).trim().split('\n');
  assert.deepEqual([...listing].sort(),[...e.files].sort());
  assert.equal(new Set(listing).size,listing.length);
  assert(listing.every(n=>/^[a-zA-Z0-9_.-]+$/.test(n)&&n!=='.'&&n!=='..'));
  assert(execFileSync('tar',['-tvzf',archive],{encoding:'utf8'}).trim().split('\n').every(n=>n[0]==='-'));
  execFileSync('tar',['-xzf',archive,'--no-same-owner','--no-same-permissions','-C',temp]);
  const read=n=>JSON.parse(fs.readFileSync(path.join(temp,n)));
  const reports=['class-boa.json','class-beforeafter.json','class-same-binary.json'];
  const audit=JSON.parse(execFileSync(process.execPath,[path.join(repo,'scripts/audit-class-workload-records.mjs'),...reports.map(n=>path.join(temp,n))],{encoding:'utf8'}));
  assert.deepEqual(audit.reports.map(r=>r.outcomes),[
    {'within noise threshold':6,'Temiya faster beyond threshold':6},
    {'after faster beyond threshold':4,'within noise threshold':8},
    {'within noise threshold':12},
  ]);
  for(const name of reports){const r=read(name);assert.equal(r.workers[0].revision,e.source_revision);assert.equal(r.workers[0].executable_sha256,read('worker-info.json').executable_sha256);}
  const counts=name=>{const log=fs.readFileSync(path.join(temp,name),'utf8');const rows=[...log.matchAll(/test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;/g)];assert(rows.every(r=>+r[2]===0));return{passed:rows.reduce((s,r)=>s+ +r[1],0),ignored:rows.reduce((s,r)=>s+ +r[3],0),groups:rows.length};};
  assert.deepEqual(counts('all-tests.log'),e.native);
  const {run,job,...expectedAsan}=e.asan;assert.equal(run,41004741987);assert.equal(job,123896969040);
  assert.deepEqual(counts('asan.log'),expectedAsan);
  const asan=fs.readFileSync(path.join(temp,'asan.log'),'utf8');assert(asan.includes('-Zsanitizer=address'));assert(asan.includes(e.source_revision));
  console.log(JSON.stringify({integrity:'pass',source:e.source_revision,native:e.native,asan:e.asan,outcomes:audit.reports.map(r=>r.outcomes),scope:e.scope}));
} finally {fs.rmSync(temp,{recursive:true,force:true});}
