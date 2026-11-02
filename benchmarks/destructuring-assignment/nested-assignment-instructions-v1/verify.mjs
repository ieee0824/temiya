import fs from 'node:fs';
import zlib from 'node:zlib';
import crypto from 'node:crypto';
import cp from 'node:child_process';
import assert from 'node:assert/strict';
const root=new URL('./',import.meta.url),capture=JSON.parse(fs.readFileSync(new URL('capture.json',root))),raw={};
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
for(const [name,entry] of Object.entries(capture.files)){
 const b=zlib.gunzipSync(fs.readFileSync(new URL(name+'.gz',root)));
 assert.equal(hash(b),entry.sha256);assert.equal(b.length,entry.bytes);raw[name]=b;
}
const info=JSON.parse(raw['worker-info.json']);assert.equal(info.source_revision,capture.source);assert.equal(info.source_clean,true);assert.equal(info.source_inputs.length,83);
for(const entry of info.source_inputs)assert.equal(hash(cp.execFileSync('git',['show',capture.source+':'+entry.path],{maxBuffer:8*1024*1024})),entry.sha256);
const manifest=JSON.parse(cp.execFileSync('git',['show',capture.source+':benchmarks/destructuring-assignment/workloads-v1/manifest.json']));
const summary=JSON.parse(raw['summary.json']);assert.equal(summary.source,capture.source);assert.equal(summary.rows.length,6);assert.equal(manifest.workloads.length,6);
for(const workload of manifest.workloads){
 const row=summary.rows.find(r=>r.name===workload.name);assert(row);assert.equal(row.inputSha256,workload.sha256);
 const profile=raw[workload.name+'.callgrind'].toString();assert.equal(row.instructions,Number(profile.match(/^summary: (\d+)/m)[1]));assert.equal(row.instructions,Number(profile.match(/^totals: (\d+)/m)[1]));
 const result=JSON.parse(raw[workload.name+'.stdout']);assert.equal(result.expected,50005000);assert.equal(result.validated_passes,2);
}
console.log(JSON.stringify({source:capture.source,integrity:'pass',captures:6,qualification:false,scope:capture.scope}));
