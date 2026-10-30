import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..');
const read=p=>JSON.parse(fs.readFileSync(p));
const e=read(root+'/field-capture-v1/evidence.json');
const archive=root+'/field-capture-v1/capture.tar.gz',bytes=fs.readFileSync(archive);
assert.equal(bytes.length,e.archive.bytes);
assert.equal(createHash('sha256').update(bytes).digest('hex'),e.archive.sha256);
const temp=fs.mkdtempSync(path.join(os.tmpdir(),'temiya-field-capture-'));
const options={encoding:'utf8',maxBuffer:16*1024*1024};
const run=(script,args)=>JSON.parse(execFileSync(process.execPath,[repo+'/scripts/'+script,...args],{...options,cwd:repo}));
try {
 const names=execFileSync('tar',['-tzf',archive],options).trim().split('\n');
 assert.equal(new Set(names).size,names.length);
 assert(names.every(n=>!path.isAbsolute(n)&&!n.split('/').some(p=>p==='.'||p==='..')));
 assert(execFileSync('tar',['-tvzf',archive],options).trim().split('\n').every(n=>n[0]==='d'||n[0]==='-'));
 execFileSync('tar',['-xzf',archive,'--no-same-owner','--no-same-permissions','-C',temp]);
 const worker=read(temp+'/worker-info.json'),engine=read(temp+'/engine-info.json');
 assert.equal(worker.revision,e.source_revision);assert.equal(engine.source_revision,e.source_revision);
 assert.equal(worker.executable_sha256,e.worker_sha256);assert.equal(engine.executable_sha256,e.engine_sha256);
 assert.equal(worker.provenance.source_tree,e.source_tree);assert.equal(worker.provenance.source_clean,true);
 for(const mode of ['ast','auto']) {
  const dir=temp+'/fields-'+mode,v=run('verify-test262-baseline.mjs',[dir]);
  assert.equal(v.files,2057);assert.equal(v.variants,4098);
  for(const [key,value] of Object.entries({pass:1008,fail:2294,timeout:796,skip:0,not_run:0,unreported:0}))assert.equal(v.totals[key],value);
  assert.equal(read(dir+'/conditions.json').engine_sha256,e.engine_sha256);
  assert.deepEqual(read(dir+'/selection.json'),read(root+'/selection.json'));
 }
 assert.deepEqual(read(temp+'/fields-ast/outcomes.json'),read(temp+'/fields-auto/outcomes.json'));
 const audit=run('audit-class-workload-records.mjs',['--fields',...['fields-boa','fields-beforeafter','fields-same-binary','fields-baseline-boa'].map(n=>temp+'/'+n+'.json')]);
 assert.deepEqual(audit.reports.map(r=>r.outcomes),e.outcomes);
 for(const name of ['fields-boa','fields-same-binary','fields-beforeafter']) {
  const r=read(temp+'/'+name+'.json');assert.equal(r.workers[0].revision,e.source_revision);assert.equal(r.workers[0].executable_sha256,e.worker_sha256);
  if(name==='fields-same-binary')assert(r.workers.every(w=>w.executable_sha256===e.worker_sha256));
 }
 const native=[...fs.readFileSync(root+'/field-capture-v1/native-all-tests.log','utf8').matchAll(/test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored/g)];
 assert.equal(native.length,110);
 for(const [i,key] of ['pass','fail','ignored'].entries())assert.equal(native.reduce((n,r)=>n+Number(r[i+1]),0),e.native[key]);
 console.log(JSON.stringify({integrity:'pass',source:e.source_revision,outcomes:e.outcomes,scope:e.scope}));
} finally {fs.rmSync(temp,{recursive:true,force:true});}
