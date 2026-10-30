// Historical intermediate data: integrity is independent of speed acceptance.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {gunzipSync} from 'node:zlib';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..');
const source=path.join(root,'lexical-v1'),e=JSON.parse(fs.readFileSync(path.join(source,'evidence.json')));
const hash=b=>createHash('sha256').update(b).digest('hex');
const temporary=fs.mkdtempSync(path.join(os.tmpdir(),'temiya-lexical-class-evidence-'));
const read=p=>JSON.parse(fs.readFileSync(p));
const audit=(script,args)=>JSON.parse(execFileSync(process.execPath,[path.join(repo,'scripts',script),...args],{encoding:'utf8'}));
try {
  for(const row of e.files){
    for(const p of [row.file,row.decoded_file])assert(/^[a-zA-Z0-9_.-]+$/.test(p)&&!p.includes('..'));
    const packed=fs.readFileSync(path.join(source,row.file));assert.equal(hash(packed),row.sha256);
    const bytes=gunzipSync(packed);assert.equal(hash(bytes),row.decoded_sha256);
    fs.writeFileSync(path.join(temporary,row.decoded_file),bytes);
  }
  const reports=['class-boa.json','class-beforeafter.json','class-same-binary.json'];
  const classes=audit('audit-class-workload-records.mjs',reports.map(n=>path.join(temporary,n)));
  assert.deepEqual(classes.reports.map(r=>r.outcomes),[
    {'within noise threshold':10,'Temiya faster beyond threshold':2},
    {'within noise threshold':9,'after faster beyond threshold':3},
    {'within noise threshold':12},
  ]);
  for(const n of reports)assert.equal(read(path.join(temporary,n)).workers[0].executable_sha256,e.worker_sha256);
  assert.equal(read(path.join(temporary,'class-same-binary.json')).workers[1].executable_sha256,e.worker_sha256);
  assert.equal(read(path.join(temporary,'class-beforeafter.json')).workers[1].executable_sha256,'dff5aebe046b7b7344bf318877ea8ddd165a3e3a361823fd7d4ee25975ee856f');
  const archive=path.join(source,e.correctness.file),bytes=fs.readFileSync(archive);
  assert.equal(bytes.length,e.correctness.bytes);assert.equal(hash(bytes),e.correctness.sha256);
  const names=execFileSync('tar',['-tzf',archive],{encoding:'utf8'}).trim().split('\n');
  assert(names.every(n=>!path.isAbsolute(n)&&!n.split('/').some(p=>p==='..'||p==='.')));
  assert(execFileSync('tar',['-tvzf',archive],{encoding:'utf8'}).trim().split('\n').every(n=>n[0]==='d'||n[0]==='-'));
  execFileSync('tar',['-xzf',archive,'-C',temporary]);
  const info=read(path.join(temporary,'engine-info.json'));
  assert.equal(info.source_revision,e.source_revision);assert.equal(info.source_clean,true);assert.equal(info.executable_sha256,e.engine_sha256);
  for(const backend of ['ast','auto']){
    const dir=path.join(temporary,'originals',backend),result=audit('verify-test262-baseline.mjs',[dir]);
    assert.equal(result.files,149);assert.equal(result.variants,294);
    assert.equal(result.totals.pass,256);assert.equal(result.totals.fail,38);
    assert.equal(read(path.join(dir,'conditions.json')).engine_sha256,e.engine_sha256);
  }
  const gate=path.join(temporary,'pr-gate/capture'),pr=audit('verify-test262-baseline.mjs',[gate]);
  assert.equal(pr.variants,214);assert.equal(pr.totals.pass,214);assert.equal(pr.totals.fail,0);
  assert.equal(read(path.join(gate,'conditions.json')).engine_sha256,e.engine_sha256);
  const loops=path.join(temporary,'temiya-154-lexical-for-originals');
  audit('audit-lexical-loop-originals.mjs',[loops]);
  const inputs=read(path.join(loops,'original-inputs.json'));assert.equal(inputs.files.length,391);
  const selection=read(path.join(loops,'selection-info.json'));
  const hashes=new Map(inputs.files.map(r=>[r.path,r.sha256]));
  for(const r of selection.inputs)assert.equal(hashes.get(r.path),r.sha256);
  for(const r of inputs.files){assert(!path.isAbsolute(r.path)&&!r.path.split('/').includes('..'));assert.equal(hash(fs.readFileSync(path.join(loops,'originals',r.path))),r.sha256);}
  const log=fs.readFileSync(path.join(temporary,'temiya-154-lexical-all-tests.log'),'utf8');
  const rows=[...log.matchAll(/test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored/g)];
  assert.equal(rows.length,96);assert.equal(rows.reduce((s,r)=>s+Number(r[1]),0),731);
  assert.equal(rows.reduce((s,r)=>s+Number(r[2]),0),0);assert.equal(rows.reduce((s,r)=>s+Number(r[3]),0),26);
  console.log(JSON.stringify({integrity:'pass',source:e.source_revision,class_reports:3,original_for_files:385,variants_per_capture:758,scope:e.scope}));
} finally {fs.rmSync(temporary,{recursive:true,force:true});}
