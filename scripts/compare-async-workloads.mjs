// Serial public-embedding comparison, including the actual microtask checkpoint.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import {spawn,execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {asyncFixtureVersion,asyncWorkloads} from './async-benchmark-reference.mjs';
import {revision as omoikaneRevision} from './omoikane-benchmark-reference.mjs';
const [directory,temiya,boaFile,samplesText='15',pairsText='3']=process.argv.slice(2);
const samples=Number(samplesText),pairs=Number(pairsText);
if(!directory||!temiya||!boaFile||!Number.isInteger(samples)||samples<1||samples>1000||
   !Number.isInteger(pairs)||pairs<1||pairs>10||!/^[a-f0-9]{40}$/.test(process.env.TEMIYA_REVISION??'')){
  throw Error('set TEMIYA_REVISION; usage: compare-async-workloads.mjs INPUT_DIR TEMIYA_WORKER BOA_BUILD_INFO [SAMPLES=15] [PAIRS=3]');
}
if(os.platform()!=='linux')throw Error('Linux /proc is required for the RSS watchdog');
const hash=value=>createHash('sha256').update(value).digest('hex');
const manifest=JSON.parse(fs.readFileSync(path.join(directory,'manifest.json')));
if(manifest.fixture_version!==asyncFixtureVersion||manifest.workloads?.length!==asyncWorkloads.length)throw Error('async fixture coverage');
const workloads=manifest.workloads.map((entry,i)=>{
  const expected=asyncWorkloads[i],file=path.join(directory,entry.name+'.json');
  const actual=JSON.parse(fs.readFileSync(file));
  if(JSON.stringify(actual)!==JSON.stringify(expected))throw Error('authored input/oracle changed');
  for(const key of ['setup','invocation','completion'])if(entry[key+'_sha256']!==hash(actual[key]))throw Error('source hash mismatch');
  for(const key of ['name','group','iterations','expected','before_drain_expected','oracle'])if(entry[key]!==expected[key])throw Error('manifest/oracle mismatch');
  return {...entry,file};
});
const boa=JSON.parse(fs.readFileSync(boaFile));
const rustc=execFileSync('rustc',['-Vv'],{encoding:'utf8'}).trim();
if(boa.workload_kind!=='async'||boa.omoikane_revision!==omoikaneRevision||!boa.checkout_clean||
   boa.profile!=='release'||boa.rustc!==rustc||boa.executable_sha256!==hash(fs.readFileSync(boa.executable))||
   boa.worker_source_sha256!==hash(fs.readFileSync(new URL('../benchmarks/omoikane-async-worker/main.rs',import.meta.url))))throw Error('actual application async adapter provenance mismatch');
const workers=[
  {name:'Temiya',mode:'Auto Script',revision:process.env.TEMIYA_REVISION,executable:temiya,executable_sha256:hash(fs.readFileSync(temiya)),profile:'release',features:[],source_sha256:hash(fs.readFileSync(new URL('../examples/measure_async_workload.rs',import.meta.url)))},
  {name:'omoikane vendored Boa',mode:'interpreter',revision:omoikaneRevision,executable:boa.executable,build:boa},
];
const scope='parse+evaluate scheduling; drain jobs; parse+evaluate completion; setup excluded';
const limits={timeout_ms:180000,rss_limit_kib:524288,proc_poll_ms:25};
const report={comparison_kind:'Temiya versus actual omoikane Boa async checkpoint',recorded_at:new Date().toISOString(),
  rustc,manifest,workers,samples,pairs,limits,measurement_scope:scope,
  notes:process.env.BENCH_NOTES??'',
  qualification:samples>=15&&pairs>=3?'repeated comparison':'diagnostic only; insufficient samples/pairs for performance acceptance',
  machine:{platform:os.platform(),architecture:os.arch(),release:os.release(),cpu_models:[...new Set(os.cpus().map(c=>c.model))],logical_cpus:os.cpus().length},
  scope_note:'Additional authored async workloads, separate from the unchanged omoikane eleven. Promise allocation/retention and actual checkpoint costs are included; no full async conformance, GC completion or application-wide claim.',
  runs:[],comparisons:[]};
const median=values=>{const v=[...values].sort((a,b)=>a-b),m=Math.floor(v.length/2);return v.length%2?v[m]:Math.floor((v[m-1]+v[m])/2);};
const stats=values=>{const m=median(values);return {median_ns:m,mad_ns:median(values.map(v=>Math.abs(v-m)))};};
async function run(workload,worker,pair,order){
  const row={workload:workload.name,group:'async',worker:worker.name,pair,order,status:'error',sampled_peak_hwm_kib:null,last_observed_rss_kib:null};
  const child=spawn(worker.executable,[workload.file,String(samples)],{stdio:['ignore','pipe','pipe']});
  let stdout='',stderr='',stopped;
  const stop=why=>{stopped??=why;child.kill('SIGKILL');};
  const poll=setInterval(()=>{
    try{
      const status=fs.readFileSync(`/proc/${child.pid}/status`,'utf8');
      const rss=status.match(/^VmRSS:\s+(\d+) kB$/m),hwm=status.match(/^VmHWM:\s+(\d+) kB$/m);
      if(rss)row.last_observed_rss_kib=Number(rss[1]);
      if(hwm)row.sampled_peak_hwm_kib=Math.max(row.sampled_peak_hwm_kib??0,Number(hwm[1]));
      if(row.last_observed_rss_kib>limits.rss_limit_kib)stop('memory_limit');
    }catch{/* Process may have exited between samples. */}
  },limits.proc_poll_ms);
  const timeout=setTimeout(()=>stop('timeout'),limits.timeout_ms);
  child.stdout.on('data',chunk=>{stdout+=chunk;if(stdout.length>1_000_000)stop('output_limit');});
  child.stderr.on('data',chunk=>{if(stderr.length<16384)stderr+=chunk;});
  try{
    row.exit=await new Promise((resolve,reject)=>{child.once('error',reject);child.once('close',(code,signal)=>resolve({code,signal}));});
    if(stopped)row.status=stopped;
    else if(row.exit.code!==0)row.status='failed';
    else{
      const m=JSON.parse(stdout);
      if(m.engine!==worker.name||m.mode!==worker.mode||m.profile!=='release'||m.scope!==scope||
         m.expected!==workload.expected||m.before_drain_expected!==workload.before_drain_expected||
         m.validated_passes!==samples+1||m.samples_ns?.length!==samples||
         !Number.isSafeInteger(m.warmup_ns)||m.warmup_ns<=0||m.samples_ns.some(n=>!Number.isSafeInteger(n)||n<=0))throw Error('invalid checked worker report');
      if(worker.name==='Temiya'&&(!Number.isInteger(m.warmup_drained_jobs)||m.warmup_drained_jobs<=0||m.drained_jobs?.length!==samples||m.drained_jobs.some(n=>!Number.isInteger(n)||n<=0)))throw Error('missing actual queued work');
      row.status='pass';row.measurement={...m,...stats(m.samples_ns)};
    }
    if(row.status!=='pass')row.diagnostic=stderr;
  }catch(error){row.status='error';row.diagnostic=String(error);}
  finally{clearInterval(poll);clearTimeout(timeout);}
  report.runs.push(row);
  console.error(JSON.stringify({workload:row.workload,worker:row.worker,pair,status:row.status,median_ns:row.measurement?.median_ns,rss_kib:row.last_observed_rss_kib}));
  return row;
}
for(const workload of workloads)for(let pair=0;pair<pairs;pair++){
  const rows=[];
  for(const [order,index]of(pair%2?[1,0]:[0,1]).entries())rows[index]=await run(workload,workers[index],pair,order);
  const comparison={workload:workload.name,group:'async',pair,status:'not_comparable'};
  if(rows.every(r=>r.status==='pass')){
    const [a,b]=rows.map(r=>r.measurement),ratio=a.median_ns/b.median_ns;
    const threshold=Math.max(.1,3*(a.mad_ns/a.median_ns+b.mad_ns/b.median_ns));
    Object.assign(comparison,{status:'comparable',temiya_to_boa_ratio:ratio,noise_threshold:threshold,
      outcome:samples<15||pairs<3?'diagnostic only':1-ratio>threshold?'Temiya faster beyond threshold':ratio-1>threshold?'Temiya slower beyond threshold':'within noise threshold'});
  }
  report.comparisons.push(comparison);
}
report.all_workloads_valid=report.runs.every(r=>r.status==='pass');
report.all_authored_async_workloads_faster_beyond_threshold=samples>=15&&pairs>=3&&report.all_workloads_valid&&report.comparisons.every(c=>c.outcome==='Temiya faster beyond threshold');
console.log(JSON.stringify(report,null,2));
if(!report.all_workloads_valid)process.exitCode=2;
