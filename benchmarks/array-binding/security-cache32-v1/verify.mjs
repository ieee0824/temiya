import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {gunzipSync} from 'node:zlib';
import {createHash} from 'node:crypto';
const [dir,source]=process.argv.slice(2);
assert(dir&&/^[a-f0-9]{40}$/.test(source??''),'usage: node verify.mjs CAPTURE_DIR EXPECTED_SOURCE');
const read=f=>JSON.parse(fs.readFileSync(path.join(dir,f)));
const evidence=read('evidence.json');
for(const [file,sha] of Object.entries(evidence.files)){
 assert(!path.isAbsolute(file)&&!file.split('/').includes('..'));
 assert.equal(createHash('sha256').update(fs.readFileSync(path.join(dir,file))).digest('hex'),sha,file);
}
const run=read('run.json'),jobs=read('jobs.json').jobs,audit=read('audit.json');
assert.equal(run.head_sha,source);assert.equal(run.status,'completed');assert.equal(run.conclusion,'success');assert.equal(jobs.length,19);
const all=[],plans=[];let asanTests=0;
for(const job of jobs){
 assert.equal(job.run_id,run.id);assert.equal(job.status,'completed');assert.equal(job.conclusion,'success');
 const raw=gunzipSync(fs.readFileSync(path.join(dir,'job-'+job.id+'.log.gz'))).toString();
 const objects=raw.split('\n').flatMap(line=>{const i=line.indexOf('{');if(i<0)return [];try{return [JSON.parse(line.slice(i))]}catch{return []}});
 const match=job.name.match(/^Miri library tests \(shard (\d+)\)$/);
 if(match){
  assert(raw.includes('MIRIFLAGS: -Zmiri-strict-provenance -Zmiri-symbolic-alignment-check'));
  const shard=Number(match[1]),plan=objects.find(p=>p.shard===shard&&p.count===16&&Array.isArray(p.selected));assert(plan);
  const exact=[...raw.matchAll(/Running exact Miri test: ([^\r\n]+)/g)].map(m=>m[1]);assert.deepEqual(exact,plan.selected);
  for(const name of exact)assert(raw.includes('test '+name+' ... ok'),name);
  assert.equal([...raw.matchAll(/test result: ok\. 1 passed; 0 failed; 0 ignored;/g)].length,exact.length);
  assert(objects.some(p=>p.shard===shard&&p.passed===exact.length&&p.skipped===0));plans.push(plan);all.push(...exact);
 }else if(job.name==='AddressSanitizer integration tests'){
  assert(raw.includes('RUSTFLAGS: -Zsanitizer=address'));assert(raw.includes('ASAN_OPTIONS: halt_on_error=1:detect_leaks=0'));
  const results=[...raw.matchAll(/test result: ok\. (\d+) passed; 0 failed; (\d+) ignored;/g)];assert(results.length>0);asanTests=results.reduce((n,m)=>n+Number(m[1]),0);assert(asanTests>0);
 }else if(job.name==='Dependency audit'){assert(raw.includes('cargo +stable audit --deny warnings'));}
 else {assert.equal(job.name,'Miri library tests');assert(raw.includes('MIRI_RESULT: success'));}
}
assert.equal(plans.length,16);assert.equal(new Set(all).size,all.length);
const sorted=[...all].sort();for(const plan of plans){assert.equal(plan.available,all.length);assert.deepEqual(plan.selected,sorted.filter((_,i)=>i%16===plan.shard));}
assert.equal(audit.uniqueExecutedExactTests,all.length);assert.equal(audit.source,source);assert.equal(audit.run,run.id);
console.log(JSON.stringify({integrity:'pass',source,run:run.id,jobs:jobs.length,miriTests:all.length,asanTests,scope:'Recorded source security only; no native speed qualification'}));
