// Replay the initial, deliberately unqualified field implementation and controls.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..');
const read=p=>JSON.parse(fs.readFileSync(p)),sha=b=>createHash('sha256').update(b).digest('hex');
const run=(script,args)=>JSON.parse(execFileSync(process.execPath,[repo+'/scripts/'+script,...args],{encoding:'utf8',cwd:repo,maxBuffer:16*1024*1024}));
const e=read(root+'/initial-v1/evidence.json'),archive=root+'/initial-v1/capture.tar.gz',bytes=fs.readFileSync(archive);
assert.equal(e.source_revision,'3f86f2467a0bf25753a0ada2d10ed46fbc5a5053');assert.equal(e.baseline_revision,'5fc8f42092f186d196f325b68187fcd83c8c7c88');
assert.equal(bytes.length,e.archive.bytes);assert.equal(sha(bytes),e.archive.sha256);
assert.equal(sha(fs.readFileSync(root+'/selection.json')),read(root+'/selection-freeze.json').selection_sha256);
const temp=fs.mkdtempSync(path.join(os.tmpdir(),'temiya-initial-fields-'));
try{
 const options={encoding:'utf8',maxBuffer:16*1024*1024};
 const names=execFileSync('tar',['-tzf',archive],options).trim().split('\n');assert.equal(new Set(names).size,names.length);assert(names.every(n=>!path.isAbsolute(n)&&!n.split('/').some(p=>p==='.'||p==='..')));
 assert(execFileSync('tar',['-tvzf',archive],options).trim().split('\n').every(n=>n[0]==='d'||n[0]==='-'));
 execFileSync('tar',['-xzf',archive,'--no-same-owner','--no-same-permissions','-C',temp]);
 const engine=read(temp+'/engine-info.json'),worker=read(temp+'/worker-info.json'),baseline=read(temp+'/baseline-engine-info.json');
 assert.equal(engine.source_revision,e.source_revision);assert.equal(engine.executable_sha256,e.engine_sha256);assert.equal(engine.source_clean,true);assert.equal(worker.revision,e.source_revision);assert.equal(worker.executable_sha256,e.worker_sha256);assert.equal(baseline.source_revision,e.baseline_revision);
 const modes=[];
 for(const mode of ['ast','auto']){
  for(const [prefix,expected,hash]of [['before-',e.before,baseline.executable_sha256],['fields-',e.after,e.engine_sha256]]){const dir=temp+'/'+prefix+mode,v=run('verify-test262-baseline.mjs',[dir]);assert.equal(v.files,2057);assert.equal(v.variants,4098);for(const k of ['pass','fail','timeout'])assert.equal(v.totals[k],expected[k]);for(const k of ['skip','not_run','unreported'])assert.equal(v.totals[k],0);assert.equal(read(dir+'/conditions.json').engine_sha256,hash);assert.deepEqual(read(dir+'/selection.json'),read(root+'/selection.json'));}
  const before=read(temp+'/before-'+mode+'/outcomes.json'),after=read(temp+'/fields-'+mode+'/outcomes.json');assert.deepEqual(after.map(x=>[x.path,x.variant]),before.map(x=>[x.path,x.variant]));let added=0,preserved=0;const transitions={};for(let i=0;i<after.length;i++){const a=after[i],b=before[i];transitions[b.outcome+' -> '+a.outcome]=(transitions[b.outcome+' -> '+a.outcome]??0)+1;if(b.outcome==='pass'){preserved++;assert.equal(a.outcome,'pass',a.path);}else if(a.outcome==='pass')added++;}assert.equal(preserved,579);assert.equal(added,429);assert.deepEqual(transitions,read(temp+'/fields-'+mode+'-before-comparison.json').transitions);modes.push({mode,variants:4098,preserved,added,transitions});
 }
 assert.deepEqual(read(temp+'/fields-ast/outcomes.json'),read(temp+'/fields-auto/outcomes.json'));
 const audit=run('audit-class-workload-records.mjs',['--fields',temp+'/fields-boa.json',temp+'/fields-same-binary.json']);assert.deepEqual(audit.reports.map(x=>x.outcomes),e.outcomes);
 for(const name of ['fields-boa','fields-same-binary']){const r=read(temp+'/'+name+'.json');assert.equal(r.workers[0].revision,e.source_revision);assert.equal(r.workers[0].executable_sha256,e.worker_sha256);if(name==='fields-same-binary')assert(r.workers.every(w=>w.executable_sha256===e.worker_sha256));}
 const native=[...fs.readFileSync(temp+'/native-all-tests.log','utf8').matchAll(/test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored/g)];assert.equal(native.length,110);assert.equal(native.reduce((n,r)=>n+ +r[1],0),820);assert(native.every(r=>+r[2]===0));assert.equal(native.reduce((n,r)=>n+ +r[3],0),26);
 console.log(JSON.stringify({integrity:'pass',source:e.source_revision,modes,outcomes:e.outcomes,merge_qualified:false,scope:e.scope}));
}finally{fs.rmSync(temp,{recursive:true,force:true});}
