import fs from 'node:fs';
import zlib from 'node:zlib';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
const root=new URL('./',import.meta.url),c=JSON.parse(fs.readFileSync(new URL('capture.json',root)));
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
function read(name){const b=zlib.gunzipSync(fs.readFileSync(new URL(name+'.gz',root)));assert.equal(sha(b),c.uncompressedSha256[name+'.gz']);return b;}
const manifest=JSON.parse(fs.readFileSync(new URL('../workloads-v1/manifest.json',root)));
assert.equal(manifest.workloads.length,6);
const summaries={};
for(const label of ['before','after']){
 const info=JSON.parse(read(label+'/worker-info.json'));
 assert.equal(info.source_revision,c[label+'Source']);assert.equal(info.source_clean,true);
 assert.equal(info.source_inputs.length,83);
 for(const f of info.source_inputs){assert.equal(sha(execFileSync('git',['show',info.source_revision+':'+f.path],{maxBuffer:8*1024*1024})),f.sha256);}
 const s=JSON.parse(read(label+'/summary.json'));assert.equal(s.source,info.source_revision);assert.equal(s.rows.length,6);
 for(const [i,w] of manifest.workloads.entries()){
  const input=fs.readFileSync(new URL('../workloads-v1/'+w.file,root));assert.equal(sha(input),w.sha256);
  const result=JSON.parse(read(label+'/'+w.name+'.stdout'));
  assert.equal(result.expected,50005000);assert.equal(result.validated_passes,2);
  assert.equal(result.architecture,'aarch64');assert.equal(result.samples_ns.length,1);
  if(label==='after'){assert.equal(result.setup_backend,'Bytecode');assert.equal(result.invocation_backend,'Bytecode');}
  const profile=read(label+'/'+w.name+'.callgrind').toString();const total=Number(profile.match(/^summary: (\d+)$/m)[1]);
  assert.equal(Number(profile.match(/^totals: (\d+)$/m)[1]),total);
  assert.deepEqual(s.rows[i],{name:w.name,instructions:total,inputSha256:w.sha256});
  assert.match(read(label+'/'+w.name+'.stderr').toString(),/Callgrind/);
 }
 summaries[label]=s;
}
console.log(JSON.stringify({scope:c.scope,rows:summaries.after.rows.map((r,i)=>({name:r.name,before:summaries.before.rows[i].instructions,after:r.instructions,changePercent:100*(r.instructions/summaries.before.rows[i].instructions-1)}))},null,2));
