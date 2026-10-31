// Replay recorded outcomes without treating failures or timeouts as passes.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';

const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..');
const read=file=>JSON.parse(fs.readFileSync(file));
const sha=bytes=>createHash('sha256').update(bytes).digest('hex');
const evidence=read(root+'/initial-v1/evidence.json');
assert.equal(evidence.source_revision,'74823117133fd6713ced719f6976399f8d5b9507');
assert.equal(evidence.baseline_revision,'612d7599f2a09598bb834b75c30f2ffc4e987006');
assert.equal(sha(fs.readFileSync(root+'/selection.json')),read(root+'/selection-freeze.json').selection_sha256);
const archive=root+'/initial-v1/capture.tar.gz',bytes=fs.readFileSync(archive);
assert.equal(bytes.length,evidence.archive.bytes);assert.equal(sha(bytes),evidence.archive.sha256);
const temp=fs.mkdtempSync(path.join(os.tmpdir(),'temiya-private-replay-'));
const options={encoding:'utf8',maxBuffer:16*1024*1024};
try {
  const names=execFileSync('tar',['-tzf',archive],options).trim().split('\n');
  assert.equal(new Set(names).size,names.length);
  assert(names.every(name=>!path.isAbsolute(name)&&!name.split('/').some(p=>p==='.'||p==='..')));
  assert(execFileSync('tar',['-tvzf',archive],options).trim().split('\n').every(row=>row[0]==='d'||row[0]==='-'));
  execFileSync('tar',['-xzf',archive,'--no-same-owner','--no-same-permissions','-C',temp]);
  const before=read(temp+'/baseline-engine-info.json'),after=read(temp+'/engine-info.json');
  assert.equal(before.source_revision,evidence.baseline_revision);assert.equal(after.source_revision,evidence.source_revision);
  const summaries=[];
  for(const mode of ['ast','auto']) {
    for(const [prefix,info,totals] of [['before',before,{pass:752,fail:1286,timeout:218}],['after',after,{pass:1407,fail:697,timeout:152}]]) {
      const dir=temp+'/'+prefix+'-'+mode;
      const result=JSON.parse(execFileSync(process.execPath,[repo+'/scripts/verify-test262-baseline.mjs',dir],{...options,cwd:repo}));
      assert.equal(result.files,1138);assert.equal(result.variants,2256);
      for(const [key,value] of Object.entries({...totals,skip:0,not_run:0,unreported:0}))assert.equal(result.totals[key],value);
      const conditions=read(dir+'/conditions.json');
      assert.equal(conditions.engine_sha256,info.executable_sha256);
      assert.equal(conditions.provided_engine_build_info.source_revision,info.source_revision);
      assert.equal(conditions.timeout_ms,1000);assert.equal(conditions.memory_mb,512);assert.equal(conditions.stack_kb,8192);
      assert.deepEqual(read(dir+'/selection.json'),read(root+'/selection.json'));
    }
    const previous=read(temp+'/before-'+mode+'/outcomes.json'),current=read(temp+'/after-'+mode+'/outcomes.json');
    assert.deepEqual(current.map(r=>[r.path,r.variant]),previous.map(r=>[r.path,r.variant]));
    let preserved=0,added=0;
    for(let i=0;i<current.length;i++) {
      if(previous[i].outcome==='pass'){assert.equal(current[i].outcome,'pass',current[i].path);preserved++;}
      else if(current[i].outcome==='pass')added++;
    }
    assert.equal(preserved,752);assert.equal(added,655);summaries.push({mode,preserved,added});
  }
  const outcomes=mode=>read(temp+'/after-'+mode+'/outcomes.json').map(r=>[r.path,r.variant,r.outcome]);
  assert.deepEqual(outcomes('ast'),outcomes('auto'));
  const worker=read(temp+'/worker-info.json');assert.equal(worker.revision,evidence.source_revision);assert.equal(worker.provenance.source_inputs.length,79);
  for(const input of worker.provenance.source_inputs)assert.equal(sha(execFileSync('git',['show',evidence.source_revision+':'+input.file],{cwd:repo})),input.sha256,input.file);
  const tests=[...fs.readFileSync(temp+'/native-tests.log','utf8').matchAll(/test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored/g)];
  assert.equal(tests.reduce((n,r)=>n+Number(r[1]),0),855);assert(tests.every(r=>Number(r[2])===0));assert.equal(tests.reduce((n,r)=>n+Number(r[3]),0),26);
  console.log(JSON.stringify({integrity:'pass',source:evidence.source_revision,summaries,remaining_each_backend:{fail:697,timeout:152},scope:'Fixed recorded original outcomes only; performance and full conformance remain separate.'}));
} finally {fs.rmSync(temp,{recursive:true,force:true});}
