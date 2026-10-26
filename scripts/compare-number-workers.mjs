// Compare two release measure_number workers serially on identical input.
// This is an internal before/after measurement, not a cross-engine claim.
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { spawn, execFileSync } from 'node:child_process';
import os from 'node:os';

const [before, after, source, expectedText, samplesText = '15', pairsText = '3'] = process.argv.slice(2);
const samples = Number(samplesText), pairs = Number(pairsText), expected = Number(expectedText);
if (!source || expectedText === undefined || !Number.isFinite(expected) ||
    !Number.isInteger(samples) || samples < 15 || samples > 1000 ||
    !Number.isInteger(pairs) || pairs < 2 || pairs > 10) {
  throw Error('usage: compare-number-workers.mjs BEFORE AFTER FILE EXPECTED [SAMPLES>=15] [PAIRS=3]');
}
const revisions = [process.env.BASELINE_REVISION, process.env.CURRENT_REVISION];
if (revisions.some(revision => !/^[a-f0-9]{40}$/.test(revision ?? ''))) {
  throw Error('set BASELINE_REVISION and CURRENT_REVISION to the exact engine commits');
}
const hash = path => createHash('sha256').update(readFileSync(path)).digest('hex');
const workers = [before, after].map((executable, i) => ({
  executable, sha256: hash(executable), revision: revisions[i],
}));
const report = {
  purpose: 'Temiya before/after only; no Boa or SpiderMonkey superiority claim',
  recorded_at: new Date().toISOString(),
  rustc: execFileSync('rustc', ['-Vv'], { encoding: 'utf8' }).trim(),
  machine: { platform: os.platform(), release: os.release(), architecture: os.arch(),
    cpu_models: [...new Set(os.cpus().map(cpu => cpu.model))], logical_cpus: os.cpus().length },
  features: [], profile: 'release', source, source_sha256: hash(source), expected, samples, pairs,
  memory_note: 'Linux /proc status sampled every 25 ms; peak HWM and last RSS are observations, not a final-process or GC leak proof. Null means unavailable.',
  workers, runs: [], comparisons: [],
};

async function run(worker, pair, order) {
  const child = spawn(worker.executable, [source, expectedText, String(samples)], { stdio: ['ignore', 'pipe', 'pipe'] });
  let stdout = '', stderr = '', sampledHwm = null, lastRss = null;
  const sampleMemory = () => {
    try {
      const status = readFileSync(`/proc/${child.pid}/status`, 'utf8');
      const hwm = status.match(/^VmHWM:\s+(\d+) kB$/m), rss = status.match(/^VmRSS:\s+(\d+) kB$/m);
      if (hwm) sampledHwm = Math.max(sampledHwm ?? 0, Number(hwm[1]));
      if (rss) lastRss = Number(rss[1]);
    } catch { /* Process exited or platform lacks /proc. */ }
  };
  const interval = setInterval(sampleMemory, 25);
  const timeout = setTimeout(() => child.kill('SIGKILL'), 600_000);
  child.stdout.on('data', data => { stdout += data; });
  child.stderr.on('data', data => { stderr += data; });
  try {
    await new Promise((resolve, reject) => {
      child.once('error', reject);
      child.once('close', (code, signal) => code === 0 ? resolve() : reject(Error(`${worker.executable}: ${code}/${signal}: ${stderr}`)));
    });
  } finally {
    clearInterval(interval);
    clearTimeout(timeout);
  }
  const measurement = JSON.parse(stdout);
  if (measurement.engine !== 'Temiya AST (no JIT)' || measurement.profile !== 'release' ||
      measurement.expected !== expected || measurement.validated_passes !== samples + 1 ||
      measurement.samples_ns?.length !== samples ||
      measurement.samples_ns.some(value => !Number.isSafeInteger(value) || value <= 0)) {
    throw Error('worker did not validate the requested workload');
  }
  const result = { pair, order, worker: workers.indexOf(worker), measurement,
    sampled_peak_hwm_kib: sampledHwm, last_observed_rss_kib: lastRss };
  report.runs.push(result);
  console.error(JSON.stringify({ pair, order, revision: worker.revision, median_ns: measurement.median_ns, mad_ns: measurement.mad_ns }));
  return result;
}

for (let pair = 0; pair < pairs; pair++) {
  const order = pair % 2 === 0 ? [0, 1] : [1, 0];
  const results = [];
  for (const [position, index] of order.entries()) results[index] = await run(workers[index], pair, position);
  const [baseline, current] = results.map(result => result.measurement);
  const change = current.median_ns / baseline.median_ns - 1;
  const threshold = Math.max(0.10, 3 * (baseline.mad_ns / baseline.median_ns + current.mad_ns / current.median_ns));
  report.comparisons.push({ pair, median_ratio: current.median_ns / baseline.median_ns,
    change, noise_threshold: threshold, improvement_exceeds_threshold: -change > threshold });
}
console.log(JSON.stringify(report, null, 2));
