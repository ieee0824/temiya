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
const captures=new Map([
 ['hosted-native-prototypes-v1',{source:'336a38dbc74242a107761680bed5c1f429b602a1',run:41044280340}],
 ['hosted-delegation-v1',{source:'8b019733ed8e08d3bb8a31319501a4d3c08bf070',run:41054310081,baseline:'2be3d7c82091d27f7d2fae58a346c44144d3666b'}],
 ['hosted-view-index-v1',{source:'2be3d7c82091d27f7d2fae58a346c44144d3666b',run:41049321956}],
]);
const groups=[
 ['original','audit-original-workload-records.mjs',[],['beforeafter','boa','baseline-boa','same-binary-control']],
 ['class','audit-class-workload-records.mjs',[],['class-beforeafter','class-boa','class-baseline-boa','class-same-binary']],
 ['derived','audit-class-workload-records.mjs',['--derived'],['derived-beforeafter','derived-boa','derived-baseline-boa','derived-same-binary']],
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
  for(const copy of [1,2]){
   const capture=temp+'/copy-'+copy,summary={copy,groups:[]};
   assert.equal(fs.readFileSync(capture+'/measured-revision.txt','utf8').trim(),e.source_revision);
   assert.equal(fs.readFileSync(capture+'/baseline-revision.txt','utf8').trim(),e.baseline_revision);
   assert.equal(read(capture+'/hardware-availability.json').target_available,true);
   for(const [group,script,flags,reports] of groups){
    const audit=JSON.parse(execFileSync(process.execPath,[repo+'/scripts/'+script,...flags,...reports.map(n=>capture+'/'+n+'.json')],{encoding:'utf8',cwd:repo,maxBuffer:16*1024*1024}));
    const saved=e.summaries.find(r=>r.copy===copy).groups.find(r=>r.name===group);
    assert.deepEqual(audit.reports.map(r=>({sha256:r.sha256,outcomes:r.outcomes,runs:r.runs,checked_values:r.checked_values,comparisons:r.comparisons})),saved.reports.map(r=>({sha256:r.sha256,outcomes:r.outcomes,runs:r.runs,checked_values:r.checked_values,comparisons:r.comparisons})));
    const candidate=read(capture+'/'+reports[1]+'.json').workers[0];
    assert.equal(candidate.revision,e.source_revision);assert.equal(candidate.profile,'release');assert.equal(candidate.mode,'Auto Script');assert.deepEqual(candidate.features,[]);
    for(let i=0;i<reports.length;i++){
     const r=read(capture+'/'+reports[i]+'.json');assert.equal(r.machine.architecture,'x64');assert.equal(r.machine.platform,'linux');
     assert.equal(r.samples,15);assert.equal(r.pairs,3);assert.equal(r.all_workloads_valid,true);
     const first=r.workers[0];assert.equal(first.revision,i===2?e.baseline_revision:e.source_revision);
     if(i!==2)assert.equal(first.executable_sha256,candidate.executable_sha256);
     if(i===0)assert.equal(r.workers[1].revision,e.baseline_revision);
     if(i===3)assert(r.workers.every(w=>w.executable_sha256===candidate.executable_sha256));
    }
    summary.groups.push({suite:group,outcomes:audit.reports.map(r=>r.outcomes)});
   }
   summaries.push(summary);
  }
  console.log(JSON.stringify({integrity:'pass',capture:name,source:e.source_revision,reports:24,summaries,scope:e.scope}));
 } finally {fs.rmSync(temp,{recursive:true,force:true});}
}
