import fs from 'node:fs';import assert from 'node:assert/strict';import {createHash} from 'node:crypto';
import {fileURLToPath} from 'node:url';import path from 'node:path';
import {verifyParameterCorrectionReceipt,correctionSha256} from '../../../scripts/omoikane-parameter-correction.mjs';
import {classManifestSha256,classWorkloads} from '../../../scripts/parameter-benchmark-reference.mjs';
const root=path.dirname(fileURLToPath(import.meta.url)),out=root+'/release-validation';
const hash=b=>createHash('sha256').update(b).digest('hex'),read=f=>JSON.parse(fs.readFileSync(out+'/'+f));
for(const [f,sha]of Object.entries(read('evidence.json').files)){assert.equal(path.basename(f),f);assert.equal(hash(fs.readFileSync(out+'/'+f)),sha);}
assert.equal(hash(fs.readFileSync(root+'/correction.patch')),correctionSha256);
const info=read('build-info.json');verifyParameterCorrectionReceipt(info);assert.equal(hash(fs.readFileSync(out+'/Cargo.lock')),info.adapter_lock_sha256);
const inputRoot=root+'/../workloads-v1';assert.equal(hash(fs.readFileSync(inputRoot+'/manifest.json')),classManifestSha256);
const results=read('results.json');assert.equal(results.length,8);assert.equal(classWorkloads.length,8);
for(const [i,fixed]of classWorkloads.entries()){
 const r=results[i];assert.equal(r.name,fixed.name);assert.equal(r.status,0);assert.equal(r.signal,null);assert.equal(r.error,null);assert.equal(r.input_sha256,fixed.file_sha256);assert.equal(hash(fs.readFileSync(inputRoot+'/'+fixed.name+'.json')),fixed.file_sha256);
 assert.equal(fs.readFileSync(out+'/'+fixed.name+'.stderr','utf8'),'');const m=read(fixed.name+'.stdout');assert.equal(m.engine,'omoikane vendored Boa');assert.equal(m.mode,'interpreter');assert.equal(m.profile,'release');assert.equal(m.expected,fixed.expected);assert.equal(m.validated_passes,2);assert.equal(m.samples_ns.length,1);
}
console.log(JSON.stringify({integrity:'pass',reference:'explicit corrected pinned Boa',inputs:8,validatedPassesPerInput:2,nativeSpeedQualified:false}));
