// Preserve every dependency failure and verify current captures without rerunning JS.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..');
const read=p=>JSON.parse(fs.readFileSync(p)),sha=b=>createHash('sha256').update(b).digest('hex');
const e=read(path.join(root,'native-loop-v1/evidence.json')),a=e.correctness;
assert.equal(a.file,'correctness.tar.gz');
const archive=path.join(root,'native-loop-v1',a.file),bytes=fs.readFileSync(archive);
assert.equal(bytes.length,a.bytes);assert.equal(sha(bytes),a.sha256);
const temp=fs.mkdtempSync(path.join(os.tmpdir(),'temiya-loop-originals-'));
const run=(name,args)=>JSON.parse(execFileSync(process.execPath,[path.join(repo,'scripts',name),...args],{encoding:'utf8',maxBuffer:16*1024*1024,cwd:repo}));
try {
 const names=execFileSync('tar',['-tzf',archive],{encoding:'utf8'}).trim().split('\n');
 assert.equal(new Set(names).size,names.length);
 assert(names.every(n=>!path.isAbsolute(n)&&!n.split('/').some(p=>p==='..'||p==='.')));
 assert(execFileSync('tar',['-tvzf',archive],{encoding:'utf8'}).trim().split('\n').every(n=>n[0]==='d'||n[0]==='-'));
 execFileSync('tar',['-xzf',archive,'--no-same-owner','--no-same-permissions','-C',temp]);
 const info=read(temp+'/engine-info.json');assert.equal(info.source_revision,e.source_revision);assert.equal(info.source_clean,true);
 const expected=read(temp+'/originals/expectations.json'),selection=fs.readFileSync(temp+'/originals/selection.json');
 assert.equal(sha(selection),expected.selection_sha256);assert.equal(JSON.parse(selection).length,149);
 assert.deepEqual(expected,read(path.join(repo,'tests/fixtures/test262-base-classes/expectations.json')));
 for(const row of expected.sources){assert(!path.isAbsolute(row.file)&&!row.file.split('/').includes('..'));assert.equal(sha(fs.readFileSync(path.join(temp,'originals/originals',row.file))),row.sha256);}
 const key=r=>JSON.stringify([r.path,r.variant]),known=new Map(expected.known_failures.map(r=>[key(r),r]));
 const signature=(dir,row)=>{const[file,pointer]=row.raw_record.split('#');assert.match(pointer,/^\/results\/\d+$/);const actual=read(path.join(dir,file)).results[+pointer.split('/')[2]];assert.equal(actual.path,row.path);assert.equal(actual.variant,row.variant);return actual;};
 for(const mode of ['ast','auto']){
  const dir=temp+'/originals/'+mode,v=run('verify-test262-baseline.mjs',[dir]);
  assert.equal(v.files,149);assert.equal(v.variants,294);assert.equal(v.totals.pass,256);assert.equal(v.totals.fail,38);
  assert.equal(read(dir+'/conditions.json').engine_sha256,info.executable_sha256);
  for(const row of read(dir+'/outcomes.json')){if(row.outcome==='pass')continue;assert.equal(row.outcome,'fail');const rule=known.get(key(row));assert(rule);const actual=signature(dir,row);assert.deepEqual(actual.observed_failure,rule.observed_failure);assert.equal(actual.diagnostic,rule.diagnostic);}
  const dirs=[temp+'/before-'+mode,temp+'/for-'+mode];
  for(const d of dirs){const v=run('verify-test262-baseline.mjs',[d]);assert.equal(v.files,385);assert.equal(v.variants,758);}
  const conditions=dirs.map(d=>read(d+'/conditions.json'));
  assert.equal(conditions[1].engine_sha256,info.executable_sha256);
  assert.equal(conditions[0].engine_sha256,'26616f9e4b9b252b7adf8608fc69a4e2daf631451d865d82cca97c2100ab4f8d');
  for(const f of ['timeout_ms','memory_mb','stack_kb','batch_size','corpus_revision','selection_sha256','universe_sha256'])assert.deepEqual(conditions[0][f],conditions[1][f]);
  assert.deepEqual(read(dirs[0]+'/selection.json'),read(dirs[1]+'/selection.json'));
  const rows=dirs.map(d=>read(d+'/outcomes.json'));
  for(let i=0;i<758;i++){assert.equal(key(rows[0][i]),key(rows[1][i]));assert.equal(rows[0][i].outcome,rows[1][i].outcome);const actual=dirs.map((d,j)=>signature(d,rows[j][i]));for(const f of ['status','observed_failure','diagnostic','detail','reason','async_outcome'])assert.deepEqual(actual[0][f]??null,actual[1][f]??null);}
 }
 const reference=temp+'/reference';run('verify-test262-archive.mjs',[path.join(repo,'benchmarks/test262-feature-baseline'),reference]);
 const gate=run('check-test262-regressions.mjs',[reference,temp+'/pr-gate/capture',temp+'/policy.json']);
 assert.deepEqual(gate,read(temp+'/pr-gate/check-test262-regressions.mjs.stdout'));
 assert.equal(gate.passed,true);assert.equal(gate.selected_files,129);assert.equal(gate.observed_variants,216);
 console.log(JSON.stringify({integrity:'pass',source:e.source_revision,class_files:149,class_variants:294,class_pass:256,dependency_failures:38,for_files:385,for_variants_per_backend:758,changed_outcomes:0,changed_signatures:0,pr_variants:216,scope:a.scope}));
} finally {fs.rmSync(temp,{recursive:true,force:true});}
