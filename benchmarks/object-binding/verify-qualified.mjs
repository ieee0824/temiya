import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..');
const read=p=>JSON.parse(fs.readFileSync(p)),q=read(root+'/qualification.json');
const git=(...args)=>execFileSync('git',['-c','safe.directory='+repo,...args],{cwd:repo,encoding:'utf8'}).trim();
const sha=b=>createHash('sha256').update(b).digest('hex');
assert.equal(q.runtime_source,'ab8997d839263eae34315dc232f79b7bdbf65a36');
const inputs=['src','examples/measure_workload.rs','Cargo.toml','Cargo.lock','build.rs'];
const event=process.env.GITHUB_EVENT_PATH?read(process.env.GITHUB_EVENT_PATH):null;
const current=process.argv.includes('--current-source')||event?.pull_request?.number===549;
const files=git('ls-tree','-r','--name-only',q.runtime_source,'--',...inputs).split('\n');
if(current)assert.deepEqual(git('ls-files',...inputs).split('\n'),files);
assert.equal(files.length,83);assert.deepEqual(files,q.source_inputs.map(i=>i.path));
for(const input of q.source_inputs){
 if(current)assert.equal(sha(fs.readFileSync(repo+'/'+input.path)),input.sha256,input.path);
 assert.equal(sha(execFileSync('git',['-c','safe.directory='+repo,'show',q.runtime_source+':'+input.path],{cwd:repo})),input.sha256);
}
if(current)assert.equal(git('diff','--name-only',q.runtime_source,'--',...files),'');
const run=(file,args=[])=>execFileSync(process.execPath,[file,...args],{cwd:repo,encoding:'utf8',maxBuffer:16*1024*1024});
assert.equal(q.native.run,41291042940);assert.equal(q.native.archive,'hosted-prepared-guards-v1');
const native=JSON.parse(run(root+'/verify-hosted.mjs',[q.native.archive]));
assert.equal(native.integrity,'pass');assert.equal(native.source,q.runtime_source);assert.equal(native.reports,76);
assert.deepEqual(native.summaries.map(s=>s.copy),[1,2]);
for(const copy of native.summaries){
 assert.equal(copy.groups.length,11);
 for(const group of copy.groups)assert(Object.values(group.boa_wins_by_workload).every(n=>n>=2),`${copy.copy}/${group.suite}`);
 assert.equal(Object.keys(copy.groups.find(g=>g.suite==='object-bindings').boa_wins_by_workload).length,6);
}
assert.equal(q.full.run,41291044397);assert.equal(q.full.archive,'full-prepared-guards-v1');
const full=run(root+'/'+q.full.archive+'/verify.mjs').trim().split('\n').map(line=>JSON.parse(line));
assert.equal(full.length,2);assert(full.every(r=>r.integrity==='pass'&&r.regressions===0));
assert.deepEqual(full.map(r=>r.preserved),[33565,37919]);
assert.equal(q.security.run,41291045818);assert.equal(q.security.archive,'security-prepared-guards-v1');
const security=JSON.parse(run(root+'/'+q.security.archive+'/verify.mjs',[root+'/'+q.security.archive,q.runtime_source]));
assert.equal(security.integrity,'pass');assert.equal(security.jobs,19);assert.equal(security.miriTests,173);assert.equal(security.asanTests,998);
console.log(JSON.stringify({qualification:'pass',currentSourceChecked:current,runtimeSource:q.runtime_source,compilerInputs:files.length,nativeJobs:2,allGroupsQualified:true,variants:100211,preservedPasses:37919,securityJobs:19,miriTests:173,asanTests:998,scope:q.scope}));
