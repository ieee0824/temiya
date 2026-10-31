import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..');
const read=p=>JSON.parse(fs.readFileSync(p)),sha=b=>createHash('sha256').update(b).digest('hex');
const e=read(root+'/integrated-correctness-v1/evidence.json'),archive=root+'/integrated-correctness-v1/capture.tar.gz',bytes=fs.readFileSync(archive);
assert.equal(bytes.length,e.archive.bytes);assert.equal(sha(bytes),e.archive.sha256);
assert.equal(sha(fs.readFileSync(root+'/selection.json')),read(root+'/selection-freeze.json').selection_sha256);
const temp=fs.mkdtempSync(path.join(os.tmpdir(),'temiya-static-initial-'));
const options={encoding:'utf8',maxBuffer:16*1024*1024};
try {
 const names=execFileSync('tar',['-tzf',archive],options).trim().split('\n');
 assert.equal(new Set(names).size,names.length);
 assert(names.every(n=>!path.isAbsolute(n)&&!n.split('/').some(p=>p==='.'||p==='..')));
 assert(execFileSync('tar',['-tvzf',archive],options).trim().split('\n').every(n=>n[0]==='d'||n[0]==='-'));
 execFileSync('tar',['-xzf',archive,'--no-same-owner','--no-same-permissions','-C',temp]);
 const engine=read(temp+'/engine-info.json'),baseline=read(temp+'/baseline-engine-info.json');
 assert.equal(engine.source_revision,e.source_revision);assert.equal(baseline.source_revision,e.baseline_revision);
 const modes=[];
 for(const mode of ['ast','auto']) {
  for(const [prefix,expected,hash]of [['before-',e.before,baseline.executable_sha256],['static-',e.after,engine.executable_sha256]]) {
   const dir=temp+'/'+prefix+mode;
   const v=JSON.parse(execFileSync(process.execPath,[repo+'/scripts/verify-test262-baseline.mjs',dir],{...options,cwd:repo}));
   assert.equal(v.files,276);assert.equal(v.variants,547);
   for(const [key,value]of Object.entries({...expected,skip:0,not_run:0,unreported:0}))assert.equal(v.totals[key],value);
   assert.equal(read(dir+'/conditions.json').engine_sha256,hash);
   assert.deepEqual(read(dir+'/selection.json'),read(root+'/selection.json'));
  }
  const before=read(temp+'/before-'+mode+'/outcomes.json'),after=read(temp+'/static-'+mode+'/outcomes.json');
  assert.deepEqual(after.map(x=>[x.path,x.variant]),before.map(x=>[x.path,x.variant]));
  let preserved=0,added=0;const transitions={};
  for(let i=0;i<after.length;i++) {
   const a=after[i],b=before[i];transitions[b.outcome+' -> '+a.outcome]=(transitions[b.outcome+' -> '+a.outcome]??0)+1;
   if(b.outcome==='pass'){preserved++;assert.equal(a.outcome,'pass',a.path);}else if(a.outcome==='pass')added++;
  }
  assert.equal(preserved,463);assert.equal(added,0);
  assert.deepEqual(transitions,read(temp+'/'+mode+'-before-comparison.json').transitions);
  modes.push({mode,preserved,added,transitions});
 }
 assert.deepEqual(read(temp+'/static-ast/outcomes.json'),read(temp+'/static-auto/outcomes.json'));
 const native=[...fs.readFileSync(temp+'/native-all-tests.log','utf8').matchAll(/test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored/g)];
 assert.equal(native.length,111);assert.equal(native.reduce((n,r)=>n+Number(r[1]),0),844);assert(native.every(r=>Number(r[2])===0));assert.equal(native.reduce((n,r)=>n+Number(r[3]),0),26);
 console.log(JSON.stringify({integrity:'pass',source:e.source_revision,modes,scope:e.scope}));
} finally {fs.rmSync(temp,{recursive:true,force:true});}
