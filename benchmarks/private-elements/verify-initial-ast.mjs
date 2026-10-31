// A complete Ast capture is retained without implying Auto or newer-source qualification.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..'),dir=root+'/initial-ast-v1';
const read=p=>JSON.parse(fs.readFileSync(p)),sha=b=>createHash('sha256').update(b).digest('hex');
const e=read(dir+'/evidence.json');assert.equal(e.source,'6e3cbf46f58a68d702a60090fc137c998e670aa5');assert.equal(e.baseline,'eaeaf7118139839862b15dfabf05ee95ea7526ca');assert.equal(e.backend,'ast');
const freeze=read(root+'/selection-freeze.json');assert.equal(sha(fs.readFileSync(root+'/selection.json')),freeze.selection_sha256);
const archive=dir+'/capture.tar.gz',bytes=fs.readFileSync(archive);assert.equal(bytes.length,e.archive.bytes);assert.equal(sha(bytes),e.archive.sha256);
const tmp=fs.mkdtempSync(path.join(os.tmpdir(),'temiya-private-elements-ast-')),options={encoding:'utf8',maxBuffer:32*1024*1024};
try {
 const names=execFileSync('tar',['-tzf',archive],options).trim().split('\n');
 assert.equal(names.length,new Set(names).size);assert(names.every(n=>!path.isAbsolute(n)&&!n.split('/').some(p=>p==='.'||p==='..')));
 assert(execFileSync('tar',['-tvzf',archive],options).trim().split('\n').every(n=>n[0]==='d'||n[0]==='-'));
 execFileSync('tar',['-xzf',archive,'--no-same-owner','--no-same-permissions','-C',tmp]);
 const summaries=[];
 for(const [prefix,revision,infofile,totals]of [['before',e.baseline,'baseline-engine-info.json',{pass:2108,fail:4108,timeout:2272}],['after',e.source,'engine-info.json',{pass:3954,fail:2506,timeout:2028}]]){
  const capture=tmp+'/'+prefix+'-ast',info=read(tmp+'/'+infofile),conditions=read(capture+'/conditions.json');
  const verification=JSON.parse(execFileSync(process.execPath,[repo+'/scripts/verify-test262-baseline.mjs',capture],{...options,cwd:repo}));
  assert.equal(verification.files,4273);assert.equal(verification.variants,8488);assert.equal(verification.corpus_revision,freeze.corpus_revision);
  for(const [key,value]of Object.entries({...totals,skip:0,not_run:0,unreported:0}))assert.equal(verification.totals[key],value);
  assert.equal(info.source_revision,revision);assert.equal(conditions.provided_engine_build_info.source_revision,revision);assert.equal(conditions.engine_sha256,info.executable_sha256);
  assert.equal(conditions.backend_requested,'ast');assert.equal(conditions.timeout_ms,1000);assert.equal(conditions.memory_mb,512);assert.equal(conditions.stack_kb,8192);
  assert.deepEqual(read(capture+'/selection.json'),read(root+'/selection.json'));summaries.push(verification.totals);
 }
 const a=read(tmp+'/before-ast/outcomes.json'),b=read(tmp+'/after-ast/outcomes.json');assert.deepEqual(a.map(r=>[r.path,r.variant]),b.map(r=>[r.path,r.variant]));let preserved=0,added=0;
 for(let i=0;i<a.length;i++){if(a[i].outcome==='pass'){assert.equal(b[i].outcome,'pass',a[i].path);preserved++;}else if(b[i].outcome==='pass')added++;}
 assert.equal(preserved,2108);assert.equal(added,1846);assert.equal(e.preserved,preserved);assert.equal(e.added,added);assert.equal(e.regressions,0);
 const comparison=read(dir+'/comparison.json');assert.deepEqual(comparison.regressions,[]);assert.equal(comparison.preserved,preserved);assert.equal(comparison.added,added);assert.deepEqual(comparison.baseline_totals,summaries[0]);assert.deepEqual(comparison.candidate_totals,summaries[1]);
 const worker=read(tmp+'/worker-info.json');assert.equal(worker.revision,e.source);assert.equal(worker.provenance.source_inputs.length,80);
 for(const input of worker.provenance.source_inputs)assert.equal(sha(execFileSync('git',['show',e.source+':'+input.file],{cwd:repo})),input.sha256,input.file);
 const native=fs.readFileSync(tmp+'/native-tests.log','utf8');assert(!native.includes('test result: FAILED'));const tests=[...native.matchAll(/test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored/g)];
 assert.equal(tests.reduce((n,r)=>n+Number(r[1]),0),867);assert(tests.every(r=>Number(r[2])===0));assert.equal(tests.reduce((n,r)=>n+Number(r[3]),0),26);
 console.log(JSON.stringify({integrity:'pass',source:e.source,backend:'ast',files:4273,variants:8488,preserved,added,remaining:{fail:2506,timeout:2028},scope:e.scope}));
} finally {fs.rmSync(tmp,{recursive:true,force:true});}
