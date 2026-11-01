// Integrity replay deliberately accepts retained adverse speed observations.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..');
const read=p=>JSON.parse(fs.readFileSync(p)),sha=b=>createHash('sha256').update(b).digest('hex');
const captures=new Map([['hosted-comparisons-v1',{source:'a151aedc4b75cfb5812f881f3644fc0d529fcb4e',run:41307560987,baseline:'38fcaf00e4a39c8e97f31f6cda82b0ae8adc7600'}],['hosted-initial-v1',{source:'d460123c29dfd7a250b390e9c9464552f7b330db',run:41245424047,baseline:'38fcaf00e4a39c8e97f31f6cda82b0ae8adc7600'}],['hosted-vm-v1',{source:'eb29c4169c0fdfd9246546f76b5874097a53ef62',run:41303412892,baseline:'38fcaf00e4a39c8e97f31f6cda82b0ae8adc7600'}]]);
const directGroups=new Set(['elements','bindings','object-bindings','assignments']);
const groups=[
 ['original','audit-original-workload-records.mjs',[],['beforeafter','boa','baseline-boa','same-binary-control']],
 ['class','audit-class-workload-records.mjs',[],['class-beforeafter','class-boa','class-baseline-boa','class-same-binary']],
 ['derived','audit-class-workload-records.mjs',['--derived'],['derived-beforeafter','derived-boa','derived-baseline-boa','derived-same-binary']],
 ['super','audit-class-workload-records.mjs',['--super'],['super-beforeafter','super-boa','super-baseline-boa','super-same-binary']],
 ['fields','audit-class-workload-records.mjs',['--fields'],['fields-beforeafter','fields-boa','fields-baseline-boa','fields-same-binary']],
 ['static','audit-class-workload-records.mjs',['--static'],['static-beforeafter','static-boa','static-baseline-boa','static-same-binary']],
 ['private','audit-class-workload-records.mjs',['--private'],['private-beforeafter','private-boa','private-baseline-boa','private-same-binary']],
 ['elements','audit-class-workload-records.mjs',['--elements'],['elements-boa','elements-same-binary']],
 ['object-bindings','audit-class-workload-records.mjs',['--object-bindings'],['object-bindings-boa','object-bindings-same-binary']],
 ['bindings','audit-class-workload-records.mjs',['--bindings'],['bindings-boa','bindings-same-binary']],
 ['iterator','audit-class-workload-records.mjs',['--iterators'],['iterator-beforeafter','iterator-boa','iterator-baseline-boa','iterator-same-binary']],
];
groups.push(['assignments','audit-class-workload-records.mjs',['--assignments'],['assignments-boa','assignments-same-binary']]);
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
  for(const copy of [1,2]){
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
    const candidate=read(capture+'/'+reports[directGroups.has(group)?0:1]+'.json').workers[0];
    assert.equal(candidate.executable_sha256,identity.same_binary_hash);assert.equal(candidate.revision,e.source_revision);assert.equal(candidate.profile,'release');assert.equal(candidate.mode,'Auto Script');assert.deepEqual(candidate.features,[]);
    for(let i=0;i<reports.length;i++){
     const r=read(capture+'/'+reports[i]+'.json');assert.equal(r.machine.architecture,'x64');assert.equal(r.machine.platform,'linux');
     for(const key of ['platform','release','architecture','cpu_models','logical_cpus','cpu_affinity'])assert.deepEqual(r.machine[key],identity.machine[key],key);
     assert.equal(r.samples,15);assert.equal(r.pairs,3);assert.equal(r.all_workloads_valid,true);
     const first=r.workers[0];assert.equal(first.revision,!directGroups.has(group)&&i===2?e.baseline_revision:e.source_revision);
     if(directGroups.has(group)||i!==2)assert.equal(first.executable_sha256,candidate.executable_sha256);
     if(!directGroups.has(group)&&i===0){assert.equal(r.workers[1].revision,e.baseline_revision);assert.equal(r.workers[1].executable_sha256,baseline.executable_sha256);}
     if(!directGroups.has(group)&&i===2)assert.equal(first.executable_sha256,baseline.executable_sha256);
     if(directGroups.has(group)?i===1:i===3)assert(r.workers.every(w=>w.executable_sha256===candidate.executable_sha256));
    }
    const wins={};for(const comparison of read(capture+'/'+reports[directGroups.has(group)?0:1]+'.json').comparisons)wins[comparison.workload]=(wins[comparison.workload]??0)+(comparison.outcome==='Temiya faster beyond threshold'?1:0);assert.deepEqual(wins,saved.boa_wins_by_workload);assert.equal(Object.values(wins).every(n=>n>=2),saved.minimum_two_of_three);
    summary.groups.push({suite:group,boa_wins_by_workload:wins,outcomes:audit.reports.map(r=>r.outcomes)});
   }
   summaries.push(summary);
  }
  console.log(JSON.stringify({integrity:'pass',capture:name,source:e.source_revision,reports:80,summaries,scope:e.scope}));
 } finally {fs.rmSync(temp,{recursive:true,force:true});}
}
