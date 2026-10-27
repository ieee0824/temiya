// Validate both real public-embedding adapters, not synthetic timing output.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
const workers=process.argv.slice(2);
if(!workers.length)throw Error('usage: check-async-workers.mjs WORKER [WORKER ...]');
const root=fs.mkdtempSync(path.join(os.tmpdir(),'async worker checks '));
const setup='async function seed(value){return value;} var result=-1,calls=0; function record(value){result=value;}';
const invocation='result=-1;seed(42).then(record);result;';
const base={setup,invocation,completion:'result;',expected:42,before_drain_expected:-1};
const cases=[
  {name:'queued-result',input:base,success:true},
  {name:'negative-zero',input:{...base,invocation:'result=-1;seed(-0).then(record);result;',expected:-0},success:true},
  {name:'wrong-completion',input:{...base,expected:41},success:false},
  {name:'wrong-before-checkpoint',input:{...base,before_drain_expected:42},success:false},
  {name:'wrong-after-warmup',input:{...base,invocation:'result=-1;seed(0).then(function(){result=++calls;});result;',expected:1},success:false},
  {name:'callback-throw',input:{...base,invocation:'result=-1;seed(42).then(function(){throw 7;});result;'},success:false},
  {name:'completion-throw',input:{...base,completion:'null.x;'},success:false},
];
const checks=[];
for(const worker of workers)for(const test of cases){
  const file=path.join(root,test.name+'.json');
  let json=JSON.stringify(test.input);
  if(Object.is(test.input.expected,-0))json=json.replace('"expected":0','"expected":-0');
  fs.writeFileSync(file,json);
  const result=spawnSync(worker,[file,'2'],{encoding:'utf8',timeout:30000});
  if(result.error||result.signal||(result.status===0)!==test.success)throw Error(`${worker} ${test.name}: ${result.error??result.stderr??result.status}`);
  if(test.success){
    const m=JSON.parse(result.stdout);
    if(m.validated_passes!==3||m.samples_ns?.length!==2||m.before_drain_expected!==test.input.before_drain_expected||
       m.scope!=='parse+evaluate scheduling; drain jobs; parse+evaluate completion; setup excluded'||
       m.samples_ns.some(n=>!Number.isSafeInteger(n)||n<=0))throw Error('invalid async adapter result');
  }else if(result.stdout!=='')throw Error('failed async worker emitted a timing report');
  checks.push({worker,name:test.name,expected_success:test.success,exit_status:result.status});
}
console.log(JSON.stringify({checks},null,2));
