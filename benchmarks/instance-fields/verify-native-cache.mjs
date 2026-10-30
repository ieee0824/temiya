import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..'),dir=root+'/native-cache-v1';
const read=p=>JSON.parse(fs.readFileSync(p));
for(const [file,hash] of Object.entries(read(dir+'/files.json')))assert.equal(createHash('sha256').update(fs.readFileSync(dir+'/'+file)).digest('hex'),hash,file);
const worker=read(dir+'/worker-info.json'),engine=read(dir+'/engine-info.json');
assert.equal(worker.revision,'835b21da13c4ffffb06359790995ce6a0a1a67eb');
assert.equal(engine.source_revision,worker.revision);assert.equal(worker.provenance.source_clean,true);
assert.equal(worker.provenance.source_inputs.length,77);
for(const input of worker.provenance.source_inputs) {
 const bytes=execFileSync('git',['show',worker.revision+':'+input.file],{cwd:repo,maxBuffer:16*1024*1024});
 assert.equal(createHash('sha256').update(bytes).digest('hex'),input.sha256,input.file);
}
const reports=['fields-boa','fields-same-binary','fields-beforeafter','fields-baseline-boa'];
const audit=JSON.parse(execFileSync(process.execPath,[repo+'/scripts/audit-class-workload-records.mjs','--fields',...reports.map(n=>dir+'/'+n+'.json')],{cwd:repo,encoding:'utf8',maxBuffer:16*1024*1024}));
assert.deepEqual(audit.reports.map(r=>r.outcomes),[{'Temiya faster beyond threshold':12,'within noise threshold':3},{'after faster beyond threshold':1,'within noise threshold':13,'after slower beyond threshold':1},{'within noise threshold':15},{'Temiya faster beyond threshold':15}]);
for(const name of reports.slice(0,3)) {
 const r=read(dir+'/'+name+'.json');assert.equal(r.workers[0].revision,worker.revision);assert.equal(r.workers[0].executable_sha256,worker.executable_sha256);
 if(name==='fields-same-binary')assert(r.workers.every(w=>w.executable_sha256===worker.executable_sha256));
}
const native=[...fs.readFileSync(dir+'/native-all-tests.log','utf8').matchAll(/test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored/g)];
assert.equal(native.length,110);assert.equal(native.reduce((n,r)=>n+Number(r[1]),0),828);assert(native.every(r=>Number(r[2])===0));assert.equal(native.reduce((n,r)=>n+Number(r[3]),0),26);
console.log(JSON.stringify({integrity:'pass',source:worker.revision,outcomes:audit.reports.map(r=>r.outcomes),scope:'Historical cache-only ARM capture: arrow0/3significant Boa wins, substantial variance, and adverse identical-executable controls retained. Not speed-qualified.'}));
