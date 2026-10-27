// Exact cold/unique workload and serial actual-embedding comparison.
import {spawn, execFileSync} from 'node:child_process';
import {readFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import os from 'node:os';
import vm from 'node:vm';
import {fileURLToPath} from 'node:url';
const hash=b=>createHash('sha256').update(b).digest('hex');
const need=(v,s)=>{if(!v)throw Error(s);};
const [candidateInfo,baselineInfo,boaInfo]=process.argv.slice(2);
need(candidateInfo&&baselineInfo&&boaInfo&&process.argv.length===5,'usage: measure-regexp-cold-diagnostic.mjs CANDIDATE_INFO BASELINE_INFO ACTUAL_BOA_INFO');
need(os.platform()==='linux','Linux /proc watchdog required');
const path=fileURLToPath(new URL('../benchmarks/regexp-cache/cold-unique/input.json',import.meta.url));
const bytes=readFileSync(path),workload={...JSON.parse(bytes),path};
need(hash(bytes)==='66911cc92b9cf12141fa42a66f21176457ec148cd3a09aa829eba22f4b228ce9','cold input changed');
need(workload.iterations===2048&&workload.expected===2048*(8+5+2),'independent oracle');
const context=vm.createContext({});vm.runInContext(workload.setup,context);
for(let i=0;i<16;i++)need(vm.runInContext(workload.invocation,context)===30720,'Node oracle');
need(vm.runInContext('uniqueSerial;',context)===32768,'all patterns fresh across passes');
const candidate=JSON.parse(readFileSync(candidateInfo)),baseline=JSON.parse(readFileSync(baselineInfo)),boa=JSON.parse(readFileSync(boaInfo));
const rustc=execFileSync('rustc',['-Vv'],{encoding:'utf8'}).trim();
for(const info of [candidate,baseline])need(info.profile==='release'&&info.mode==='Auto Script'&&Array.isArray(info.features)&&info.features.length===0&&/^[a-f0-9]{40}$/.test(info.revision??'')&&info.rustc===rustc&&info.executable_sha256===hash(readFileSync(info.executable)),'Temiya build/source/compiler/binary provenance');
const canonical=hash(readFileSync(new URL('../benchmarks/omoikane-worker/main.rs',import.meta.url)));
const historical=boa.worker_source_sha256==='031ee68a56d3c61ec45ca8f7069526dae3fa6eda6adeb79e3f39e6c3539041a8';
need(boa.worker_source_sha256===canonical||historical,'actual adapter identity');
need(boa.omoikane_revision==='007556adccb8fb4329236932ca8175f8e8d119d2'&&boa.checkout_clean&&boa.profile==='release'&&boa.rustflags===''&&boa.rustc===rustc&&boa.original_lock_sha256==='f3988c4d9b0ecc28560df18df1cae7760c7c55a016318ea3e31f3ebbfcaea211'&&boa.executable_sha256===hash(readFileSync(boa.executable)),'actual pinned application provenance');
need((boa.workload_kind==='sync'||historical&&boa.workload_kind===undefined)&&boa.execution_mode==='interpreter; baseline-jit feature absent'&&!boa.features.boa_engine.includes('baseline-jit')&&boa.features.regress.includes('utf16'),'actual fork/features/mode');
const workers=[{...candidate,name:'Temiya candidate',engine:'Temiya'},{...baseline,name:'Temiya baseline',engine:'Temiya'},{...boa,name:'omoikane vendored Boa',engine:'omoikane vendored Boa',mode:'interpreter',revision:boa.omoikane_revision}];
const samples=15,pairs=3,timeout_ms=180000,rss_limit_kib=524288;
const report={comparison_kind:'exact-cold-unique-regexp',recorded_at:new Date().toISOString(),qualification:'Fresh pattern text throughout first checked pass and all 15 samples. Serial three-worker rotations; first pass retained as warmup_ns, no native-speed inference from instrumentation.',input_sha256:hash(bytes),runner_sha256:hash(readFileSync(new URL(import.meta.url))),rustc,workers,samples,pairs,machine:{platform:os.platform(),release:os.release(),architecture:os.arch(),cpu_models:[...new Set(os.cpus().map(c=>c.model))],logical_cpus:os.cpus().length,cpu_affinity:readFileSync('/proc/self/status','utf8').match(/^Cpus_allowed_list:\s+(.+)$/m)?.[1]},limits:{timeout_ms,rss_limit_kib,proc_poll_ms:25,note:'Sampled RSS/HWM, may overshoot between polls; not a hard retained-heap bound. Actual Boa retains its real sandbox.'},runs:[],comparisons:[]};
const median = values => {
  const sorted = [...values].sort((a, b) => a - b), mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[mid] : Math.floor((sorted[mid - 1] + sorted[mid]) / 2);
};
const summary = values => {
  const median_ns = median(values);
  return { median_ns, mad_ns: median(values.map(value => Math.abs(value - median_ns))) };
};

async function run(workload, worker, pair, order) {
  const result = { workload: workload.name, worker: worker.name, pair, order, status: 'error',
    sampled_peak_hwm_kib: null, last_observed_rss_kib: null };
  const child = spawn(worker.executable, [...(worker.arguments ?? []), workload.path, String(samples)], { stdio: ['ignore', 'pipe', 'pipe'] });
  let stdout = '', stderr = '', stopped;
  const stop = reason => { stopped ??= reason; child.kill('SIGKILL'); };
  const interval = setInterval(() => {
    try {
      const status = readFileSync(`/proc/${child.pid}/status`, 'utf8');
      const hwm = status.match(/^VmHWM:\s+(\d+) kB$/m), rss = status.match(/^VmRSS:\s+(\d+) kB$/m);
      if (hwm) result.sampled_peak_hwm_kib = Math.max(result.sampled_peak_hwm_kib ?? 0, Number(hwm[1]));
      if (rss) result.last_observed_rss_kib = Number(rss[1]);
      if (result.last_observed_rss_kib > rss_limit_kib) stop('memory_limit');
    } catch { /* Process exited or /proc is unavailable. */ }
  }, 25);
  const timeout = setTimeout(() => stop('timeout'), timeout_ms);
  child.stdout.on('data', data => { stdout += data; if (stdout.length > 1_000_000) stop('output_limit'); });
  child.stderr.on('data', data => { if (stderr.length < 16_384) stderr += data; });
  try {
    const exit = await new Promise((resolve, reject) => {
      child.once('error', reject);
      child.once('close', (code, signal) => resolve({ code, signal }));
    });
    result.exit = exit;
    if (stopped) result.status = stopped;
    else if (exit.code !== 0) result.status = 'failed';
    else {
      const measurement = JSON.parse(stdout);
      if (measurement.engine !== (worker.engine ?? worker.name) || measurement.mode !== worker.mode ||
          measurement.scope !== 'parse+evaluate invocation; function setup excluded' ||
          measurement.profile !== 'release' || measurement.expected !== workload.expected ||
          measurement.validated_passes !== samples + 1 || measurement.samples_ns?.length !== samples ||
          measurement.samples_ns.some(value => !Number.isSafeInteger(value) || value <= 0)) {
        throw Error('invalid worker result or timing boundary');
      }
      result.status = 'pass';
      result.measurement = { ...measurement, ...summary(measurement.samples_ns) };
    }
    if (result.status !== 'pass') result.diagnostic = stderr;
  } catch (error) {
    result.status = 'error'; result.diagnostic = String(error);
  } finally {
    clearInterval(interval); clearTimeout(timeout);
  }
  report.runs.push(result);
  console.error(JSON.stringify({ workload: workload.name, worker: worker.name, pair, status: result.status,
    median_ns: result.measurement?.median_ns, rss_kib: result.last_observed_rss_kib }));
  return result;
}

for(let pair=0;pair<pairs;pair++)for(let order=0;order<workers.length;order++){
 const worker=workers[(pair+order)%workers.length],row=await run(workload,worker,pair,order);
 if(row.status==='pass'&&worker.engine==='Temiya'&&(row.measurement.setup_backend!=='Bytecode'||row.measurement.invocation_backend!=='Bytecode')){row.status='error';row.diagnostic='Default Engine did not use Bytecode';}
}
for(let pair=0;pair<pairs;pair++)for(let j=1;j<workers.length;j++){
 const [a,b]=[workers[0],workers[j]].map(w=>report.runs.find(r=>r.worker===w.name&&r.pair===pair));
 if(a.status!=='pass'||b.status!=='pass'){report.comparisons.push({pair,reference:workers[j].name,status:'not comparable',outcome:'failed'});continue;}
 const ratio=a.measurement.median_ns/b.measurement.median_ns,threshold=Math.max(.1,3*(a.measurement.mad_ns/a.measurement.median_ns+b.measurement.mad_ns/b.measurement.median_ns));
 report.comparisons.push({pair,reference:workers[j].name,status:'comparable',ratio,threshold,outcome:1-ratio>threshold?'candidate faster':ratio-1>threshold?'candidate slower':'within noise'});
}
report.all_workloads_valid=report.runs.every(r=>r.status==='pass');
report.all_cold_comparisons_faster_than_actual_boa=report.all_workloads_valid&&report.comparisons.filter(c=>c.reference===workers[2].name).length===3&&report.comparisons.filter(c=>c.reference===workers[2].name).every(c=>c.outcome==='candidate faster');
console.log(JSON.stringify(report,null,2));
if(!report.all_workloads_valid)process.exitCode=1;
