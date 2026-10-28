import fs from 'node:fs';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {oracle,revision,fixtureSha256} from '../../../../scripts/omoikane-benchmark-reference.mjs';
const med=a=>{const s=[...a].sort((a,b)=>a-b),i=s.length>>1;return s.length%2?s[i]:Math.floor((s[i-1]+s[i])/2)};
const reports=[];
for(const file of process.argv.slice(2)){
 const bytes=fs.readFileSync(file),r=JSON.parse(bytes),before=r.comparison_kind==='Temiya before/after';
 assert.equal(r.manifest.revision,revision);assert.equal(r.manifest.fixture_sha256,fixtureSha256);
 assert.deepEqual(r.manifest.workloads.map(w=>[w.name,w.iterations,w.expected]),oracle);
 assert.equal(r.samples,15);assert.equal(r.pairs,3);assert.equal(r.runs.length,66);assert.equal(r.comparisons.length,33);
 assert.equal(r.limits.timeout_ms,180000);assert.equal(r.limits.rss_limit_kib,524288);
 const counts={};
 for(const [name,,expected] of oracle)for(let pair=0;pair<3;pair++){
  const runs=r.workers.map(w=>r.runs.filter(x=>x.workload===name&&x.pair===pair&&x.worker===w.name));
  assert(runs.every(a=>a.length===1));const [a,b]=runs.map(x=>x[0]);
  assert.equal(a.order,pair%2);assert.equal(b.order,1-pair%2);
  for(const run of [a,b]){
   assert.equal(run.status,'pass');assert.deepEqual(run.exit,{code:0,signal:null});
   const m=run.measurement;assert.equal(m.expected,expected);assert.equal(m.validated_passes,16);assert.equal(m.samples_ns.length,15);
   assert(m.samples_ns.every(n=>Number.isSafeInteger(n)&&n>0));
   assert.equal(m.median_ns,med(m.samples_ns));assert.equal(m.mad_ns,med(m.samples_ns.map(n=>Math.abs(n-m.median_ns))));
  }
  const ratio=a.measurement.median_ns/b.measurement.median_ns;
  const gate=Math.max(.1,3*(a.measurement.mad_ns/a.measurement.median_ns+b.measurement.mad_ns/b.measurement.median_ns));
  const outcome=1-ratio>gate?(before?'after faster beyond threshold':'Temiya faster beyond threshold'):ratio-1>gate?(before?'after slower beyond threshold':'Temiya slower beyond threshold'):'within noise threshold';
  const cs=r.comparisons.filter(x=>x.workload===name&&x.pair===pair);assert.equal(cs.length,1);
  const c=cs[0];assert.equal(c.status,'comparable');assert.equal(c.noise_threshold,gate);assert.equal(c.outcome,outcome);
  assert.equal(c[before?'after_to_before_ratio':r.comparison_kind==='Temiya versus SpiderMonkey'?'temiya_to_spidermonkey_ratio':'temiya_to_boa_ratio'],ratio);
  counts[outcome]=(counts[outcome]??0)+1;
 }
 assert.equal(r.all_workloads_valid,true);
 const win=before?'after faster beyond threshold':'Temiya faster beyond threshold';
 assert.equal(r[before?'all_eleven_workloads_after_faster_beyond_threshold':'all_eleven_workloads_faster_beyond_threshold'],counts[win]===33);
 reports.push({file,sha256:createHash('sha256').update(bytes).digest('hex'),runs:66,checked_values:1056,comparisons:33,outcomes:counts});
}
console.log(JSON.stringify({verified_at:new Date().toISOString(),reports},null,2));
