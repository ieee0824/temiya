import fs from 'node:fs';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import {classManifestSha256,classWorkloads} from '../../scripts/assignment-benchmark-reference.mjs';
const dir=new URL('./boa-oracles-v1/',import.meta.url);
const read=file=>JSON.parse(fs.readFileSync(new URL(file,dir)));
for(const r of read('integrity.json'))assert.equal(crypto.createHash('sha256').update(fs.readFileSync(new URL(r.file,dir))).digest('hex'),r.sha256);
const manifest=JSON.parse(fs.readFileSync(new URL('./workloads-v1/manifest.json',import.meta.url)));
assert.equal(manifest.workloads.length,6);
assert.equal(crypto.createHash('sha256').update(JSON.stringify(manifest,null,2)+'\n').digest('hex'),classManifestSha256);
for(const [i,item] of manifest.workloads.entries()){
 const workload=JSON.parse(fs.readFileSync(new URL('./workloads-v1/'+item.file,import.meta.url)));
 assert.deepEqual({...workload,file_sha256:item.sha256,oracle:item.oracle},classWorkloads[i]);
}
assert.equal(read('boa-build-info.json').omoikane_revision,'007556adccb8fb4329236932ca8175f8e8d119d2');
assert.equal(read('scope.json').nativeQualification,false);
for(const item of manifest.workloads){
 const bytes=fs.readFileSync(new URL('./workloads-v1/'+item.file,import.meta.url));
 assert.equal(crypto.createHash('sha256').update(bytes).digest('hex'),item.sha256);
 for(const engine of ['boa','temiya']){
  const stem=item.name+'.'+engine;
  assert.deepEqual(read(stem+'.exit.json'),{status:0,signal:null,error:null});
  const result=read(stem+'.stdout');
  assert.equal(result.engine,engine==='boa'?'omoikane vendored Boa':'Temiya');
  assert.equal(result.profile,'release');
  assert.equal(result.expected,50005000);
  assert.equal(result.validated_passes,2);
  assert.equal(result.samples_ns.length,1);
 }
}
console.log(JSON.stringify({workloads:6,engines:2,validatedPassesPerInputAndEngine:2,nativeQualification:false}));
