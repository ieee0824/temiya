import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..');
const read=p=>JSON.parse(fs.readFileSync(p)),sha=b=>createHash('sha256').update(b).digest('hex');
const v=read(root+'/validation.json'),freeze=read(root+'/workload-freeze.json');
assert.equal(v.freeze_revision,'75d6b2a13962e0a78c3076eb30e4e2027fc0fa6c');
assert.equal(v.reference_revision,'007556adccb8fb4329236932ca8175f8e8d119d2');
for(const file of ['selection.json','selection-freeze.json','workload-freeze.json',...freeze.inputs.map(x=>'workloads-v1/'+x.file)]){
 const original=execFileSync('git',['-c','safe.directory='+repo,'show',v.freeze_revision+':benchmarks/object-binding/'+file],{cwd:repo});
 assert.equal(sha(fs.readFileSync(root+'/'+file)),sha(original));
}
const selection=read(root+'/selection-freeze.json');assert.equal(selection.files,8807);assert.equal(selection.variants,17415);
assert.equal(sha(fs.readFileSync(root+'/selection.json')),selection.selection_sha256);
assert.equal(new Set(read(root+'/selection.json')).size,8807);
assert.equal(freeze.inputs.length,6);
for(const [file,hash]of Object.entries(v.files)){
 assert.equal(path.basename(file),file);assert.equal(sha(fs.readFileSync(root+'/boa-oracles-v1/'+file)),hash);
}
assert.equal(read(root+'/boa-oracles-v1/build-info.json').omoikane_revision,v.reference_revision);
for(const input of freeze.inputs){
 const bytes=fs.readFileSync(root+'/workloads-v1/'+input.file);assert.equal(sha(bytes),input.sha256);
 const workload=JSON.parse(bytes),output=read(root+'/boa-oracles-v1/'+input.file);
 assert.equal(workload.iterations,10000);assert.equal(workload.expected,10000*10001/2);
 assert.equal(output.expected,workload.expected);assert.equal(output.validated_passes,2);
}
console.log(JSON.stringify({integrity:'pass',files:8807,variants:17415,workloads:6,validated_passes:12,scope:v.scope}));
