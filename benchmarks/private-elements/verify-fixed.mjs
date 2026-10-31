import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..'),dir=root+'/fixed-integrated-bindings-v1';
const read=p=>JSON.parse(fs.readFileSync(p)),sha=b=>createHash('sha256').update(b).digest('hex');
const e=read(dir+'/evidence.json'),freeze=read(root+'/selection-freeze.json');
assert.equal(e.source,'f60a02caa0224a6a78e466d2e784b0bd55d96fac');assert.equal(e.baseline_source,'eaeaf7118139839862b15dfabf05ee95ea7526ca');
assert.equal(sha(fs.readFileSync(root+'/selection.json')),freeze.selection_sha256);
const bytes=fs.readFileSync(dir+'/capture.tar.gz');assert.equal(bytes.length,e.archive.bytes);assert.equal(sha(bytes),e.archive.sha256);
const temp=fs.mkdtempSync(path.join(os.tmpdir(),'temiya-elements-fixed-')),options={encoding:'utf8',maxBuffer:16*1024*1024};
try {
 const names=execFileSync('tar',['-tzf',dir+'/capture.tar.gz'],options).trim().split('\n');assert.equal(names.length,new Set(names).size);assert(names.every(n=>!path.isAbsolute(n)&&!n.split('/').some(p=>p==='.'||p==='..')));
 assert(execFileSync('tar',['-tvzf',dir+'/capture.tar.gz'],options).trim().split('\n').every(n=>n[0]==='d'||n[0]==='-'));
 execFileSync('tar',['-xzf',dir+'/capture.tar.gz','--no-same-owner','--no-same-permissions','-C',temp]);
 const rows={};for(const label of ['before','after']){rows[label]={};for(const mode of ['ast','auto']){
  const capture=temp+'/'+label+'-'+mode,v=JSON.parse(execFileSync(process.execPath,[repo+'/scripts/verify-test262-baseline.mjs',capture],{...options,cwd:repo})),c=read(capture+'/conditions.json');
  assert.equal(v.files,4273);assert.equal(v.variants,8488);assert.equal(v.corpus_revision,freeze.corpus_revision);
  assert.equal(c.provided_engine_build_info.source_revision,label==='before'?e.baseline_source:e.source);assert.equal(c.engine_sha256,c.provided_engine_build_info.executable_sha256);assert.equal(c.engine_frozen_during_run,true);
  assert.equal(c.backend_requested,mode);assert.equal(c.timeout_ms,1000);assert.equal(c.memory_mb,512);assert.equal(c.stack_kb,8192);assert.deepEqual(read(capture+'/selection.json'),read(root+'/selection.json'));
  const expected=label==='before'?{pass:2108,fail:4108,timeout:2272}:mode==='ast'?{pass:3954,fail:2506,timeout:2028}:{pass:3950,fail:2506,timeout:2032};
  for(const [k,n]of Object.entries({...expected,skip:0,not_run:0,unreported:0}))assert.equal(v.totals[k],n);rows[label][mode]=read(capture+'/outcomes.json');
 }}
 const key=r=>[r.path,r.variant],summary=[];
 for(const mode of ['ast','auto']){const a=rows.before[mode],b=rows.after[mode];assert.deepEqual(a.map(key),b.map(key));const regressions=a.filter((r,i)=>r.outcome==='pass'&&b[i].outcome!=='pass').map(r=>({path:r.path,variant:r.variant,before:'pass',after:'timeout'}));summary.push({backend:mode,preserved:a.filter((r,i)=>r.outcome==='pass'&&b[i].outcome==='pass').length,added:a.filter((r,i)=>r.outcome!=='pass'&&b[i].outcome==='pass').length,regressions});}
 assert.deepEqual(summary,read(temp+'/baseline-comparison.json'));assert.equal(summary[0].regressions.length,0);assert.equal(summary[1].regressions.length,4);
 const a=rows.after.ast,b=rows.after.auto;assert.deepEqual(a.map(key),b.map(key));const difference=a.flatMap((r,i)=>r.outcome!==b[i].outcome?[{path:r.path,variant:r.variant,ast:r.outcome,auto:b[i].outcome,raw_record:b[i].raw_record}]:[]),record=read(temp+'/ast-auto-full-comparison.json');assert.equal(difference.length,4);assert(difference.every(r=>r.ast==='pass'&&r.auto==='timeout'));assert.deepEqual(difference,record.differences.map(({hosted_full_auto,...r})=>r));assert(record.differences.every(r=>r.hosted_full_auto==='pass'));
 const diagnostic=read(temp+'/timeout-diagnostics/results.json');assert.deepEqual(diagnostic.counts,{fail:0,pass:4,skip:0});assert.equal(diagnostic.backend,'Auto');assert.equal(diagnostic.corpus_revision,freeze.corpus_revision);assert.deepEqual(diagnostic.results.map(key).sort(),difference.map(key).sort());assert(diagnostic.results.every(r=>r.status==='pass'&&r.resources.timeout_ms===1000&&r.resources.memory_limit_bytes===536870912&&r.resources.stack_limit_bytes===8388608));
 const info=read(temp+'/worker-info.json');assert.equal(info.revision,e.source);assert.equal(info.provenance.source_inputs.length,80);for(const input of info.provenance.source_inputs)assert.equal(sha(execFileSync('git',['show',e.source+':'+input.file],{cwd:repo})),input.sha256);
 const tests=[...fs.readFileSync(temp+'/native-tests.log','utf8').matchAll(/test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored/g)];assert.equal(tests.reduce((n,r)=>n+Number(r[1]),0),871);assert(tests.every(r=>Number(r[2])===0));assert.equal(tests.reduce((n,r)=>n+Number(r[3]),0),26);
 console.log(JSON.stringify({integrity:'pass',source:e.source,files:4273,variants:8488,recorded_auto_pass_to_timeout:4,diagnostic_passes:4,scope:e.scope}));
} finally {fs.rmSync(temp,{recursive:true,force:true});}
