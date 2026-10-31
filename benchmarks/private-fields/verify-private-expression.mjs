// Profiled instruction counts are diagnostic, not native speed qualification.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {gunzipSync} from 'node:zlib';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..'),dir=root+'/private-expression-v1';
const read=name=>JSON.parse(fs.readFileSync(dir+'/'+name));
const sha=bytes=>createHash('sha256').update(bytes).digest('hex');
const evidence=read('evidence.json');
assert.equal(evidence.source_revision,'e6722cb08955d588ef36bc9997a3679d89590125');
assert.equal(evidence.baseline_revision,'74823117133fd6713ced719f6976399f8d5b9507');
for(const [file,hash]of Object.entries(evidence.files))assert.equal(sha(fs.readFileSync(dir+'/'+file)),hash,file);
for(const [name,revision,count]of [['worker-info.json',evidence.source_revision,80],['baseline-worker-info.json',evidence.baseline_revision,79]]) {
  const info=read(name);assert.equal(info.revision,revision);assert.equal(info.provenance.source_clean,true);assert.equal(info.provenance.source_inputs.length,count);
  for(const input of info.provenance.source_inputs)assert.equal(sha(execFileSync('git',['show',revision+':'+input.file],{cwd:repo})),input.sha256,input.file);
}
const tests=[...fs.readFileSync(dir+'/native-tests.log','utf8').matchAll(/test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored/g)];
assert.equal(tests.reduce((n,r)=>n+Number(r[1]),0),856);assert(tests.every(r=>Number(r[2])===0));assert.equal(tests.reduce((n,r)=>n+Number(r[3]),0),26);
assert.match(fs.readFileSync(dir+'/clippy.log','utf8'),/Finished `dev` profile/);
for(const [label,count]of [['before',195821712],['after',130138634]]) {
  const profile=gunzipSync(fs.readFileSync(dir+'/callgrind-'+label+'.out.gz')).toString();
  assert.equal(Number(profile.match(/^summary: (\d+)$/m)?.[1]),count);assert.equal(Number(profile.match(/^totals: (\d+)$/m)?.[1]),count);
  const worker=read('callgrind-'+label+'-worker.json');assert.equal(worker.expected,50005000);assert.equal(worker.validated_passes,2);
}
console.log(JSON.stringify({integrity:'pass',source:evidence.source_revision,native_tests:856,compiled_inputs:80,instructions:{before:195821712,after:130138634},scope:evidence.scope}));
