// Profiled instruction counts are diagnostic, not native speed qualification.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {gunzipSync} from 'node:zlib';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..'),name=process.argv[2]??'private-expression-v1',dir=root+'/'+name;
const variants=new Map([['local-bindings-diagnostic-v1',{source:'31fa20e3d3577c289d7981dfd35cd30da6097f7f',baseline:'9ed3bc7c535ae2d153b2974d5c75e4e566c82aa3',baselineInputs:80,native:861,before:490862016,after:481149396}],['identifier-values-diagnostic-v1',{source:'03d3724c7c827b73bd4c813eeb180e1e3cef32b0',baseline:'31fa20e3d3577c289d7981dfd35cd30da6097f7f',baselineInputs:80,native:862,before:481149396,after:431811114}],['bound-constructor-diagnostic-v1',{source:'9ed3bc7c535ae2d153b2974d5c75e4e566c82aa3',baseline:'a65fa12bb7123b1af0e0a2b2254411ee7170c3e8',baselineInputs:80,native:861,before:512342212,after:490862016}],['private-expression-v1',{source:'e6722cb08955d588ef36bc9997a3679d89590125',baseline:'74823117133fd6713ced719f6976399f8d5b9507',baselineInputs:79,native:856,before:195821712,after:130138634}],['bound-private-diagnostic-v1',{source:'a65fa12bb7123b1af0e0a2b2254411ee7170c3e8',baseline:'eaeaf7118139839862b15dfabf05ee95ea7526ca',baselineInputs:80,native:861,before:536133532,after:512342212}]]);
assert(variants.has(name));const expected=variants.get(name);
const read=name=>JSON.parse(fs.readFileSync(dir+'/'+name));
const sha=bytes=>createHash('sha256').update(bytes).digest('hex');
const evidence=read('evidence.json');
assert.equal(evidence.source_revision,expected.source);
assert.equal(evidence.baseline_revision,expected.baseline);
for(const [file,hash]of Object.entries(evidence.files))assert.equal(sha(fs.readFileSync(dir+'/'+file)),hash,file);
for(const [name,revision,count]of [['worker-info.json',evidence.source_revision,80],['baseline-worker-info.json',evidence.baseline_revision,expected.baselineInputs]]) {
  const info=read(name);assert.equal(info.revision,revision);assert.equal(info.provenance.source_clean,true);assert.equal(info.provenance.source_inputs.length,count);
  for(const input of info.provenance.source_inputs)assert.equal(sha(execFileSync('git',['show',revision+':'+input.file],{cwd:repo})),input.sha256,input.file);
}
const tests=[...fs.readFileSync(dir+'/native-tests.log','utf8').matchAll(/test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored/g)];
assert.equal(tests.reduce((n,r)=>n+Number(r[1]),0),expected.native);assert(tests.every(r=>Number(r[2])===0));assert.equal(tests.reduce((n,r)=>n+Number(r[3]),0),26);
assert.match(fs.readFileSync(dir+'/clippy.log','utf8'),/Finished `dev` profile/);
for(const [label,count]of [['before',expected.before],['after',expected.after]]) {
  const profile=gunzipSync(fs.readFileSync(dir+'/callgrind-'+label+'.out.gz')).toString();
  assert.equal(Number(profile.match(/^summary: (\d+)$/m)?.[1]),count);assert.equal(Number(profile.match(/^totals: (\d+)$/m)?.[1]),count);
  const worker=read('callgrind-'+label+'-worker.json');assert.equal(worker.expected,50005000);assert.equal(worker.validated_passes,2);
}
console.log(JSON.stringify({integrity:'pass',source:evidence.source_revision,native_tests:expected.native,compiled_inputs:80,instructions:{before:expected.before,after:expected.after},scope:evidence.scope}));
