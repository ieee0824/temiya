// Native qualification is stricter than integrity replay of adverse captures.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..'),dir=root+'/hosted-nested-assignment-v1',e=JSON.parse(fs.readFileSync(dir+'/evidence.json'));
assert(process.argv.length===2||(process.argv.length===3&&process.argv[2]==='--current-source'));
assert.equal(e.source_revision,'eb7f054d7db7256c3d228849a424758fac45f04e');assert.equal(e.run,41362299598);
const replay=JSON.parse(execFileSync(process.execPath,[root+'/verify-hosted.mjs','hosted-nested-assignment-v1'],{cwd:repo,encoding:'utf8',maxBuffer:8*1024*1024}));assert.equal(replay.integrity,'pass');assert.equal(replay.reports,80);
const archived=name=>JSON.parse(execFileSync('tar',['-xOzf',dir+'/capture.tar.gz',name],{encoding:'utf8',maxBuffer:8*1024*1024}));
const run=archived('run.json'),jobs=archived('jobs.json').jobs;assert.equal(run.id,e.run);assert.equal(run.head_sha,e.source_revision);assert.equal(run.status,'completed');assert.equal(run.conclusion,'success');assert.equal(run.run_attempt,1);assert.equal(run.event,'workflow_dispatch');assert.equal(jobs.length,2);assert.equal(new Set(jobs.map(j=>j.id)).size,2);assert(jobs.every(j=>j.run_id===e.run&&j.run_attempt===1&&j.status==='completed'&&j.conclusion==='success'));
assert.deepEqual(jobs.map(j=>j.name).sort(),['Record comparisons, copy 1 (speed gates separate)','Record comparisons, copy 2 (speed gates separate)']);
const groups=['original','class','derived','super','fields','static','private','elements','object-bindings','bindings','iterator','assignments'];assert.equal(e.summaries.length,2);let cases=0,wins=0;
for(const copy of[1,2]){
 const host=e.summaries.find(h=>h.copy===copy);assert(host);assert.deepEqual(host.groups.map(g=>g.name),groups);
 const availability=archived('copy-'+copy+'/hardware-availability.json');assert.equal(availability.required_cpu_model,'');assert.equal(availability.target_available,true);assert.deepEqual(availability.observed_cpu_models,host.machine.cpu_models);assert.deepEqual(host.machine.cpu_models,['AMD EPYC 7763 64-Core Processor']);
 let hostCases=0;for(const group of host.groups){assert.equal(group.minimum_two_of_three,true);const values=Object.values(group.boa_wins_by_workload);assert(values.length);assert(values.every(n=>Number.isInteger(n)&&n>=2&&n<=3));hostCases+=values.length;wins+=values.reduce((n,v)=>n+v,0);}assert.equal(hostCases,70);cases+=hostCases;
}
assert.equal(cases,140);assert.equal(wins,418);
const git=(...args)=>execFileSync('git',args,{cwd:repo,maxBuffer:8*1024*1024}),hash=b=>crypto.createHash('sha256').update(b).digest('hex');assert.equal(e.compiler_inputs.length,83);const spec=['src','examples/measure_workload.rs','Cargo.toml','Cargo.lock','build.rs'];const paths=git('ls-tree','-r','--name-only',e.source_revision,'--',...spec).toString().trim().split('\n');assert.deepEqual(e.compiler_inputs.map(f=>f.path),paths);for(const input of e.compiler_inputs)assert.equal(hash(git('show',e.source_revision+':'+input.path)),input.sha256);
if(process.argv[2]==='--current-source'){
 assert.deepEqual(git('ls-files',...spec).toString().trim().split('\n'),paths);
 for(const input of e.compiler_inputs){assert.equal(hash(git('show','HEAD:'+input.path)),input.sha256,input.path);assert.equal(hash(fs.readFileSync(repo+'/'+input.path)),input.sha256,input.path);}
}
console.log(JSON.stringify({nativeQualification:'pass',source:e.source_revision,run:e.run,reports:80,hosts:2,groupsPerHost:12,workloadsPerHost:70,pairedFaster:wins,pairedComparisons:420,minimumTwoOfThree:'all140host-workload decisions pass',compilerInputs:83,currentSourceChecked:process.argv[2]==='--current-source',scope:e.scope}));
