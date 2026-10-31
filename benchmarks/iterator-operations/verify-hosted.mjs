// Integrity replay deliberately accepts retained adverse speed observations.
import fs from 'node:fs';
import {gunzipSync} from 'node:zlib';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..');
const read=p=>JSON.parse(fs.readFileSync(p)),sha=b=>createHash('sha256').update(b).digest('hex');
const captures=new Map([['hosted-catch-records-partial-v1',{source:'ff67c89dc586cdf0d982e49aa1c72205ca5c4e66',run:41211990414,baseline:'bb0d3268d7bfbb9b6910b3b86db3a5cf1abf49c7',copies:[2]}],['hosted-hot-sites-v1',{source:'bb0d3268d7bfbb9b6910b3b86db3a5cf1abf49c7',run:41208962674,baseline:'702ee76165c04990ac517a993c3e194c279cf4a0'}],['hosted-literal-accessors-v1',{source:'702ee76165c04990ac517a993c3e194c279cf4a0',run:41204129150,baseline:'7eba69e8d8e5927b5f5c73dbdd57b30749e16442'}],['hosted-weak-environments-v1',{source:'fd4ad7b521537dc0266424c40bd59e7dc6bb3c59',run:41200250693,baseline:'7eba69e8d8e5927b5f5c73dbdd57b30749e16442'}],['hosted-owned-sites-v1',{source:'767add1afe9c03059cba1ebff05f7397d9a3f67e',run:41198352870,baseline:'7eba69e8d8e5927b5f5c73dbdd57b30749e16442'}],['hosted-outlined-strings-v1',{source:'375ad2dce49b04ce37d7f8c8015d2230dc248d82',run:41196855251,baseline:'7eba69e8d8e5927b5f5c73dbdd57b30749e16442'}],['hosted-inline-auxiliary-v1',{source:'d9906db3c26e0b6b3663206fe0617fa5dbd7bbfc',run:41192021887,baseline:'7eba69e8d8e5927b5f5c73dbdd57b30749e16442'}],['hosted-empty-frame-v1',{source:'4a4c103ade6b0acae5f5a16cfa68220f24c2300e',run:41186655959,baseline:'7eba69e8d8e5927b5f5c73dbdd57b30749e16442'}],['hosted-native-step-v1',{source:'1979ac3cb1454a8903d32d4886f71b86324f972c',run:41181490909,baseline:'7eba69e8d8e5927b5f5c73dbdd57b30749e16442'}],['hosted-definition-cache-v1',{source:'09766bba7308eaede7b76059b88b73dd2bbb4efe',run:41176605076,baseline:'7eba69e8d8e5927b5f5c73dbdd57b30749e16442'}],['hosted-initial-v1',{source:'7eba69e8d8e5927b5f5c73dbdd57b30749e16442',run:41169970601,baseline:'622bc975af697c49298d2fb5b71b575aeed6cdb5'}]]);
const groups=[
 ['original','audit-original-workload-records.mjs',[],['beforeafter','boa','baseline-boa','same-binary-control']],
 ['class','audit-class-workload-records.mjs',[],['class-beforeafter','class-boa','class-baseline-boa','class-same-binary']],
 ['derived','audit-class-workload-records.mjs',['--derived'],['derived-beforeafter','derived-boa','derived-baseline-boa','derived-same-binary']],
 ['super','audit-class-workload-records.mjs',['--super'],['super-beforeafter','super-boa','super-baseline-boa','super-same-binary']],
 ['fields','audit-class-workload-records.mjs',['--fields'],['fields-beforeafter','fields-boa','fields-baseline-boa','fields-same-binary']],
 ['static','audit-class-workload-records.mjs',['--static'],['static-beforeafter','static-boa','static-baseline-boa','static-same-binary']],
 ['private','audit-class-workload-records.mjs',['--private'],['private-beforeafter','private-boa','private-baseline-boa','private-same-binary']],
 ['elements','audit-class-workload-records.mjs',['--elements'],['elements-boa','elements-same-binary']],
 ['iterator','audit-class-workload-records.mjs',['--iterators'],['iterator-beforeafter','iterator-boa','iterator-baseline-boa','iterator-same-binary']],
];
const selected=process.argv.slice(2);if(selected.length===0)selected.push(...captures.keys());
assert.equal(new Set(selected).size,selected.length);
for(const name of selected){
 assert(captures.has(name));const dir=root+'/'+name,e=read(dir+'/evidence.json'),expected=captures.get(name);
 assert.equal(e.source_revision,expected.source);assert.equal(e.run,expected.run);
 assert.equal(e.baseline_revision,expected.baseline ?? 'f1ed7c0ab3260de78cbf169651e5093ca8239aa0');
 assert.equal(e.archive.file,'capture.tar.gz');const archive=dir+'/'+e.archive.file,bytes=fs.readFileSync(archive);
 assert.equal(bytes.length,e.archive.bytes);assert.equal(sha(bytes),e.archive.sha256);
 const temp=fs.mkdtempSync(path.join(os.tmpdir(),'temiya-derived-hosted-'));
 try {
  const names=execFileSync('tar',['-tzf',archive],{encoding:'utf8'}).trim().split('\n');
  assert.equal(new Set(names).size,names.length);assert(names.every(n=>!path.isAbsolute(n)&&!n.split('/').some(p=>p==='..'||p==='.')));
  assert(execFileSync('tar',['-tvzf',archive],{encoding:'utf8'}).trim().split('\n').every(n=>n[0]==='d'||n[0]==='-'));
  execFileSync('tar',['-xzf',archive,'--no-same-owner','--no-same-permissions','-C',temp]);
  const summaries=[];
  for(const copy of (expected.copies ?? [1,2])){
   const capture=temp+'/copy-'+copy,summary={copy,groups:[]};
   assert.equal(fs.readFileSync(capture+'/measured-revision.txt','utf8').trim(),e.source_revision);
   assert.equal(fs.readFileSync(capture+'/baseline-revision.txt','utf8').trim(),e.baseline_revision);
   assert.equal(read(capture+'/hardware-availability.json').target_available,true);
   const identity=JSON.parse(execFileSync(process.execPath,[repo+'/scripts/audit-omoikane-hosted-identities.mjs',capture,e.source_revision,e.baseline_revision],{encoding:'utf8',cwd:repo}));
   const baseline=read(capture+'/baseline-build-info.json');
   for(const [group,script,flags,reports] of groups){
    const audit=JSON.parse(execFileSync(process.execPath,[repo+'/scripts/'+script,...flags,...reports.map(n=>capture+'/'+n+'.json')],{encoding:'utf8',cwd:repo,maxBuffer:16*1024*1024}));
    const saved=e.summaries.find(r=>r.copy===copy).groups.find(r=>r.name===group);
    assert.deepEqual(audit.reports.map(r=>({sha256:r.sha256,outcomes:r.outcomes,runs:r.runs,checked_values:r.checked_values,comparisons:r.comparisons})),saved.reports.map(r=>({sha256:r.sha256,outcomes:r.outcomes,runs:r.runs,checked_values:r.checked_values,comparisons:r.comparisons})));
    const candidate=read(capture+'/'+reports[group==='elements'?0:1]+'.json').workers[0];
    assert.equal(candidate.executable_sha256,identity.same_binary_hash);assert.equal(candidate.revision,e.source_revision);assert.equal(candidate.profile,'release');assert.equal(candidate.mode,'Auto Script');assert.deepEqual(candidate.features,[]);
    for(let i=0;i<reports.length;i++){
     const r=read(capture+'/'+reports[i]+'.json');assert.equal(r.machine.architecture,'x64');assert.equal(r.machine.platform,'linux');
     for(const key of ['platform','release','architecture','cpu_models','logical_cpus','cpu_affinity'])assert.deepEqual(r.machine[key],identity.machine[key],key);
     assert.equal(r.samples,15);assert.equal(r.pairs,3);assert.equal(r.all_workloads_valid,true);
     const first=r.workers[0];assert.equal(first.revision,group!=='elements'&&i===2?e.baseline_revision:e.source_revision);
     if(group==='elements'||i!==2)assert.equal(first.executable_sha256,candidate.executable_sha256);
     if(group!=='elements'&&i===0){assert.equal(r.workers[1].revision,e.baseline_revision);assert.equal(r.workers[1].executable_sha256,baseline.executable_sha256);}
     if(group!=='elements'&&i===2)assert.equal(first.executable_sha256,baseline.executable_sha256);
     if(group==='elements'?i===1:i===3)assert(r.workers.every(w=>w.executable_sha256===candidate.executable_sha256));
    }
    const wins={};for(const comparison of read(capture+'/'+reports[group==='elements'?0:1]+'.json').comparisons)wins[comparison.workload]=(wins[comparison.workload]??0)+(comparison.outcome==='Temiya faster beyond threshold'?1:0);assert.deepEqual(wins,saved.boa_wins_by_workload);assert.equal(Object.values(wins).every(n=>n>=2),saved.minimum_two_of_three);
    summary.groups.push({suite:group,boa_wins_by_workload:wins,outcomes:audit.reports.map(r=>r.outcomes)});
   }
   summaries.push(summary);
  }
  console.log(JSON.stringify({integrity:'pass',capture:name,source:e.source_revision,reports:34*(expected.copies ?? [1,2]).length,summaries,scope:e.scope}));
 } finally {fs.rmSync(temp,{recursive:true,force:true});}
}

// Infrastructure failure precedes fixture measurement; never count it as speed evidence.
{
 const dir=root+'/hosted-catch-records-build-failure-v1',e=read(dir+'/evidence.json');
 assert.equal(e.run,41211990414);assert.equal(e.job,124573094646);assert.equal(e.copy,1);
 assert.equal(e.source,'ff67c89dc586cdf0d982e49aa1c72205ca5c4e66');
 assert.equal(e.compiled_runtime,'71009f3ea4f70a5ca471d35ca6c2857f8226dec4');
 assert.equal(e.baseline,'bb0d3268d7bfbb9b6910b3b86db3a5cf1abf49c7');assert.equal(e.workload_reports,0);
 assert.equal(e.free_gib,10.9);assert.equal(e.required_gib,14);
 for(const [file,hash]of Object.entries(e.files)){assert(!file.includes('/'));assert.equal(sha(fs.readFileSync(dir+'/'+file)),hash);}
 assert.equal(Object.keys(e.files).length,8);
 assert.equal(e.cleanup_validation.workflow_revision,'839a17a237e7e5cedf84b246168f20e2d3065bad');
 const cleanup=read(dir+'/'+e.cleanup_validation.file);
 assert.equal(cleanup.workflow_sha256,sha(execFileSync('git',['-c','safe.directory='+repo,'show',e.cleanup_validation.workflow_revision+':.github/workflows/omoikane-benchmark.yml'],{cwd:repo})));
 assert.equal(cleanup.cache_removal,true);assert.equal(cleanup.worker_sha_preserved,true);
 assert.equal(cleanup.hosted_sdk_cleanup_exercised,false);assert.equal(cleanup.worker_probes.length,2);
 for(const worker of cleanup.worker_probes){assert.equal(worker.probe.expected,42);assert.equal(worker.probe.validated_passes,2);}
 const log=gunzipSync(fs.readFileSync(dir+'/job.log.gz')).toString();
 assert(log.includes('cargo-space-guard: insufficient space before build: free=10.9 GiB, required=14.0 GiB'));
 assert.equal(fs.readFileSync(dir+'/measured-revision.txt','utf8').trim(),e.source);
 assert.equal(fs.readFileSync(dir+'/baseline-revision.txt','utf8').trim(),e.baseline);
 const current=read(dir+'/same-binary-build-info.json'),baseline=read(dir+'/baseline-build-info.json');
 assert.equal(current.revision,e.source);assert.equal(baseline.revision,e.baseline);
 for(const info of [current,baseline]){assert.match(info.executable_sha256,/^[a-f0-9]{64}$/);assert.equal(info.mode_probe.expected,42);assert.equal(info.mode_probe.validated_passes,2);}
 console.log(JSON.stringify({integrity:'pass',unavailable_attempt:e.run,copy:1,workload_reports:0,scope:e.scope}));
}
