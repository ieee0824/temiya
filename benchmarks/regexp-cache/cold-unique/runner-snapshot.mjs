import {spawn} from 'node:child_process';
import {readFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import vm from 'node:vm';
const hash=b=>createHash('sha256').update(b).digest('hex');
const frozen=JSON.parse(readFileSync('/tmp/temiya-regexp-cache-captures-boa.json'));
const workers=[frozen.workers[0],JSON.parse(readFileSync('/tmp/temiya-typed-inline-current-info.json')),frozen.workers[1]].map(w=>({...w,...(w.build??{}),engine:w.engine??(w.name==='omoikane vendored Boa'?'omoikane vendored Boa':'Temiya')}));
workers[0].name='Temiya candidate';workers[1].name='Temiya baseline';
const path='/tmp/temiya-regexp-cold-unique.json',workload={...JSON.parse(readFileSync(path)),path};
const context=vm.createContext({});vm.runInContext(workload.setup,context);for(let i=0;i<16;i++)if(vm.runInContext(workload.invocation,context)!==2048*(8+5+2))throw Error('independent Node oracle failed');
for(const w of workers)if(hash(readFileSync(w.executable))!==w.executable_sha256)throw Error('frozen hash mismatch');
const samples=15,timeout_ms=180000,rss_limit_kib=524288;
const report={qualification:'Cold/unique diagnostic, separate from fixed authored performance gate. No repeated pattern text within a process. First checked pass retained as warmup_ns; all 15 subsequent passes also use fresh pattern text. RSS/HWM sampled at 25ms, not a hard memory bound.',input_sha256:hash(readFileSync(path)),runner_source_sha256:hash(readFileSync('/workspace/scripts/compare-omoikane-workloads.mjs')),workers,samples,pairs:3,runs:[],comparisons:[]};
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

for(let pair=0;pair<3;pair++){for(let order=0;order<3;order++){const w=workers[(pair+order)%3];const row=await run(workload,w,pair,order);if(row.status!=='pass')throw Error(JSON.stringify(row));}}
for(let pair=0;pair<3;pair++){const a=report.runs.find(r=>r.pair===pair&&r.worker===workers[0].name).measurement;for(let j=1;j<3;j++){const b=report.runs.find(r=>r.pair===pair&&r.worker===workers[j].name).measurement;const ratio=a.median_ns/b.median_ns,threshold=Math.max(.1,3*(a.mad_ns/a.median_ns+b.mad_ns/b.median_ns));report.comparisons.push({pair,reference:workers[j].name,ratio,threshold,outcome:1-ratio>threshold?'candidate faster':ratio-1>threshold?'candidate slower':'within noise'});}}
console.log(JSON.stringify(report,null,2));
