import fs from 'node:fs';
import {spawn, execFileSync} from 'node:child_process';
import crypto from 'node:crypto';
import os from 'node:os';
import assert from 'node:assert/strict';

const hash = path => crypto.createHash('sha256').update(fs.readFileSync(path)).digest('hex');
const probe = '/tmp/temiya-native-startup/examples/realm_startup_probe.rs';
assert.equal(hash(probe), hash('/tmp/temiya-native-startup-baseline/examples/realm_startup_probe.rs'));
const workers = [
  {name:'after',revision:'9c9e6c59ce675f2c81e788230e4871c61fb1b5d8',executable:'/tmp/temiya-native-startup-target/release/examples/realm_startup_probe'},
  {name:'before',revision:'d2d12825f349cd37933d65c0dbde96b29971c105',executable:'/tmp/temiya-native-startup-baseline-target/release/examples/realm_startup_probe'},
].map(w=>({...w,executable_sha256:hash(w.executable)}));
const report = {recorded_at:new Date().toISOString(),rustc:execFileSync('rustc',['-Vv'],{encoding:'utf8'}).trim(),profile:'release',features:[],probe_sha256:hash(probe),workers,
  machine:{architecture:os.arch(),logical_cpus:os.cpus().length,cpu_models:[...new Set(os.cpus().map(c=>c.model))]},
  samples:50,pairs:3,limits:{timeout_ms:180000,rss_limit_kib:512*1024,poll_ms:25},
  scope:'Repeated Engine+Realm initialization inside isolated processes, after explicit allocation-profile warmups. Not first cold process startup, parse/evaluate invocation, or a Boa comparison. Both release builds finished before all timing.',
  baseline_probe_added_for_measurement:true,runs:[],comparisons:[]};
const median=a=>{a=[...a].sort((a,b)=>a-b);return a.length%2?a[Math.floor(a.length/2)]:Math.floor((a[a.length/2-1]+a[a.length/2])/2);};
async function run(worker,pair,order) {
  const result={worker:worker.name,pair,order,status:'error',sampled_peak_hwm_kib:0};
  const child=spawn(worker.executable,['50'],{stdio:['ignore','pipe','pipe']});
  let output='',errors='',stopped;
  const stop=why=>{stopped??=why;child.kill('SIGKILL');};
  const timer=setTimeout(()=>stop('timeout'),report.limits.timeout_ms);
  const watch=setInterval(()=>{try{const s=fs.readFileSync(`/proc/${child.pid}/status`,'utf8');const m=s.match(/^VmHWM:\s+(\d+) kB$/m),r=s.match(/^VmRSS:\s+(\d+) kB$/m);if(m)result.sampled_peak_hwm_kib=Math.max(result.sampled_peak_hwm_kib,Number(m[1]));if(r&&Number(r[1])>report.limits.rss_limit_kib)stop('memory_limit');}catch{}},25);
  child.stdout.on('data',b=>{output+=b;if(output.length>1000000)stop('output_limit');});
  child.stderr.on('data',b=>{if(errors.length<16384)errors+=b;});
  try {
    result.exit=await new Promise((resolve,reject)=>{child.once('error',reject);child.once('close',(code,signal)=>resolve({code,signal}));});
    if(stopped)throw Error(stopped);
    assert.equal(result.exit.code,0,errors);
    const measurement=JSON.parse(output);
    assert.equal(measurement.profile,'release');assert.equal(measurement.startup_samples_ns.length,50);
    assert(measurement.startup_samples_ns.every(n=>Number.isSafeInteger(n)&&n>0));
    assert.equal(measurement.allocation_profiles.length,3);
    for(const p of measurement.allocation_profiles.slice(1))assert.equal(p.validated_metadata,true);
    const m=median(measurement.startup_samples_ns);
    result.measurement=measurement;result.median_ns=m;result.mad_ns=median(measurement.startup_samples_ns.map(n=>Math.abs(n-m)));result.status='pass';
  } catch(e) {result.diagnostic=String(e);}
  finally {clearInterval(watch);clearTimeout(timer);}
  report.runs.push(result);console.error(JSON.stringify({...result,measurement:undefined}));
  return result;
}
for(let pair=0;pair<3;pair++) {
  const rows=[];
  for(const [order,index] of (pair%2?[1,0]:[0,1]).entries())rows[index]=await run(workers[index],pair,order);
  if(rows.every(r=>r.status==='pass')) {
    const [a,b]=rows,ratio=a.median_ns/b.median_ns,threshold=Math.max(.1,3*(a.mad_ns/a.median_ns+b.mad_ns/b.median_ns));
    report.comparisons.push({pair,after_to_before_ratio:ratio,noise_threshold:threshold,outcome:1-ratio>threshold?'after faster beyond threshold':ratio-1>threshold?'after slower beyond threshold':'within noise threshold'});
  }
}
report.all_runs_valid=report.runs.length===6&&report.runs.every(r=>r.status==='pass');
report.all_three_startup_pairs_faster_beyond_threshold=report.all_runs_valid&&report.comparisons.every(c=>c.outcome==='after faster beyond threshold');
fs.writeFileSync('/tmp/temiya-native-startup-comparison.json',JSON.stringify(report,null,2)+'\n');
console.log(JSON.stringify({valid:report.all_runs_valid,gate:report.all_three_startup_pairs_faster_beyond_threshold,comparisons:report.comparisons}));
if(!report.all_runs_valid)process.exitCode=2;
