import fs from 'node:fs';
import {spawn,execFileSync} from 'node:child_process';
import crypto from 'node:crypto';
import os from 'node:os';
import assert from 'node:assert/strict';
const hash=p=>crypto.createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const fixture='/tmp/temiya-startup-only.json';
fs.writeFileSync(fixture,JSON.stringify({setup:'',invocation:'42;',expected:42})+'\n',{flag:'wx'});
const workers=[
 {name:'after',revision:'9c9e6c59ce675f2c81e788230e4871c61fb1b5d8',executable:'/tmp/temiya-native-startup-target/release/examples/measure_workload'},
 {name:'before',revision:'d2d12825f349cd37933d65c0dbde96b29971c105',executable:'/tmp/temiya-native-startup-baseline-target/release/examples/measure_workload'},
].map(w=>({...w,executable_sha256:hash(w.executable)}));
const median=a=>{a=[...a].sort((x,y)=>x-y);return a[Math.floor(a.length/2)];};
const report={recorded_at:new Date().toISOString(),rustc:execFileSync('rustc',['-Vv'],{encoding:'utf8'}).trim(),machine:{architecture:os.arch(),logical_cpus:os.cpus().length,cpu_models:[...new Set(os.cpus().map(c=>c.model))]},workers,
 scope:'First Engine::default plus create_realm per fresh measure_workload process; 25 process observations per worker in each of three alternating groups. Empty setup and 42 invocation only validate worker completion; invocation timings are not used. Separate diagnostic, not the original eleven workloads or a Boa comparison.',
 fixture:JSON.parse(fs.readFileSync(fixture)),fixture_sha256:hash(fixture),samples:25,pairs:3,limits:{timeout_ms:180000,rss_limit_kib:524288,poll_ms:1,note:'Sampled HWM may be unavailable for short-lived workers; null means unobserved, not zero or a measured resource bound.'},runs:[],comparisons:[]};
async function run(worker,pair,sample,order){
 const r={worker:worker.name,pair,sample,order,status:'error',sampled_peak_hwm_kib:null};
 const child=spawn(worker.executable,[fixture,'1'],{stdio:['ignore','pipe','pipe']});let out='',err='',stopped;
 const stop=why=>{stopped??=why;child.kill('SIGKILL');};
 const poll=()=>{try{const status=fs.readFileSync(`/proc/${child.pid}/status`,'utf8'),h=status.match(/^VmHWM:\s+(\d+) kB$/m),rss=status.match(/^VmRSS:\s+(\d+) kB$/m);if(h&&Number(h[1])>0)r.sampled_peak_hwm_kib=Math.max(r.sampled_peak_hwm_kib??0,Number(h[1]));if(rss&&Number(rss[1])>524288)stop('memory_limit');}catch{}};
 poll();const watch=setInterval(poll,1),timer=setTimeout(()=>stop('timeout'),180000);
 child.stdout.on('data',b=>{out+=b;if(out.length>1000000)stop('output_limit');});child.stderr.on('data',b=>{if(err.length<16000)err+=b;});
 try{
  r.exit=await new Promise((resolve,reject)=>{child.once('error',reject);child.once('close',(code,signal)=>resolve({code,signal}));});
  assert(!stopped,stopped);assert.equal(r.exit.code,0,err);const m=JSON.parse(out);
  assert.equal(m.engine,'Temiya');assert.equal(m.mode,'AST interpreter');assert.equal(m.profile,'release');assert.equal(m.expected,42);assert.equal(m.validated_passes,2);assert.equal(m.samples_ns.length,1);assert(Number.isSafeInteger(m.startup_ns)&&m.startup_ns>0);
  r.measurement=m;r.status='pass';
 }catch(e){r.diagnostic=String(e);}finally{clearInterval(watch);clearTimeout(timer);}
 report.runs.push(r);return r;
}
for(let pair=0;pair<3;pair++){
 for(let sample=0;sample<25;sample++)for(const [order,index] of ((sample+pair)%2?[1,0]:[0,1]).entries())await run(workers[index],pair,sample,order);
 const summary={};for(const w of workers){const rows=report.runs.filter(r=>r.pair===pair&&r.worker===w.name);if(rows.length===25&&rows.every(r=>r.status==='pass')){const values=rows.map(r=>r.measurement.startup_ns),m=median(values);summary[w.name]={median_ns:m,mad_ns:median(values.map(v=>Math.abs(v-m)))};}}
 if(summary.after&&summary.before){const a=summary.after,b=summary.before,ratio=a.median_ns/b.median_ns,threshold=Math.max(.1,3*(a.mad_ns/a.median_ns+b.mad_ns/b.median_ns));report.comparisons.push({pair,...summary,after_to_before_ratio:ratio,noise_threshold:threshold,outcome:1-ratio>threshold?'after faster beyond threshold':ratio-1>threshold?'after slower beyond threshold':'within noise threshold'});}
}
report.all_runs_valid=report.runs.length===150&&report.runs.every(r=>r.status==='pass');
report.all_three_startup_pairs_faster_beyond_threshold=report.all_runs_valid&&report.comparisons.length===3&&report.comparisons.every(c=>c.outcome==='after faster beyond threshold');
fs.writeFileSync('/tmp/temiya-native-cold-startup-comparison.json',JSON.stringify(report,null,2)+'\n');
console.log(JSON.stringify({valid:report.all_runs_valid,gate:report.all_three_startup_pairs_faster_beyond_threshold,comparisons:report.comparisons}));
if(!report.all_runs_valid)process.exitCode=2;
