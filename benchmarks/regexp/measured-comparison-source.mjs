// Isolated, serial comparison of the public embeddings over pinned workloads.
// A failure is retained as a failure; it can never become a speedup.
import { spawn, execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import os from 'node:os';
import {fileURLToPath} from 'node:url';
import { revision, fixtureSha256, oracle } from './omoikane-benchmark-reference.mjs';
import { regexpFixtureVersion, regexpWorkloads } from './regexp-benchmark-reference.mjs';

const args = process.argv.slice(2);
const regexpSuite = args[0] === '--regexp';
if (regexpSuite) args.shift();
const beforeAfter = args[0] === '--temiya-baseline';
if (beforeAfter) args.shift();
const spiderMonkey = args[0] === '--spidermonkey';
let spiderMode;
if (spiderMonkey) {
  args.shift();
  spiderMode = args.shift();
  if (beforeAfter || !['interpreter', 'default'].includes(spiderMode)) throw Error('select one comparison and a valid SpiderMonkey mode');
}
if (regexpSuite && spiderMonkey) throw Error('Authored RegExp comparison currently supports actual Boa and Temiya; RegExp backend modes need separate verification.');
const [directory, temiya, boaInfoFile, samplesText = '15', pairsText = '3'] = args;
const samples = Number(samplesText), pairs = Number(pairsText);
if (!directory || !temiya || !boaInfoFile || !Number.isInteger(samples) || samples < 1 || samples > 1000 ||
    !Number.isInteger(pairs) || pairs < 1 || pairs > 10 || !/^[a-f0-9]{40}$/.test(process.env.TEMIYA_REVISION ?? '')) {
  throw Error('set TEMIYA_REVISION; usage: compare-omoikane-workloads.mjs [--temiya-baseline | --spidermonkey MODE] WORKLOAD_DIR TEMIYA_WORKER REFERENCE_BUILD_INFO [SAMPLES=15] [PAIRS=3]');
}
const hash = value => createHash('sha256').update(value).digest('hex');
if (os.platform() !== 'linux' || !/^VmRSS:/m.test(readFileSync('/proc/self/status', 'utf8'))) {
  throw Error('this reference runner requires Linux /proc for its memory watchdog');
}
const manifest = JSON.parse(readFileSync(join(directory, 'manifest.json')));
const boa = JSON.parse(readFileSync(boaInfoFile));
if (regexpSuite) {
  if (manifest.fixture_kind !== 'authored-regexp' || manifest.fixture_version !== regexpFixtureVersion || manifest.reference_engine_revision !== revision) throw Error('RegExp fixture provenance mismatch');
} else if (manifest.revision !== revision || manifest.fixture_sha256 !== fixtureSha256) throw Error('fixture provenance mismatch');
const selectedSpider = JSON.parse(readFileSync(new URL('../benchmarks/targets/spidermonkey.json', import.meta.url)));
if (spiderMonkey) {
  if (JSON.stringify(boa.target) !== JSON.stringify(selectedSpider) || !boa.source_clean ||
      boa.binary_sha256 !== hash(readFileSync(boa.executable)) ||
      boa.machine?.platform !== os.platform() || boa.machine?.architecture !== os.arch() ||
      boa.modes?.length !== 2 || !boa.modes.every(m => m.observed?.checks === 9 && m.observed.build.debug === false) ||
      !boa.compilers?.clang || !boa.compilers?.rustc ||
      !boa.modes.every(m => JSON.stringify(m.flags) === JSON.stringify(selectedSpider.modes[m.name])) ||
      ['ion.enable', 'baseline.enable', 'blinterp.enable'].some(k => boa.modes.find(m => m.name === 'interpreter')?.observed.jit_options[k] !== 0)) throw Error('SpiderMonkey build provenance mismatch');
} else if (beforeAfter) {
  if (boa.name !== 'Temiya' || !['AST interpreter', 'Auto Script', 'Auto Script+loop'].includes(boa.mode) || boa.profile !== 'release' ||
      !Array.isArray(boa.features) || boa.features.length !== 0 || !/^[a-f0-9]{40}$/.test(boa.revision ?? '') ||
      boa.executable_sha256 !== hash(readFileSync(boa.executable))) throw Error('Temiya baseline provenance mismatch');
} else if (boa.omoikane_revision !== (regexpSuite ? manifest.reference_engine_revision : manifest.revision) || !boa.checkout_clean ||
    boa.executable_sha256 !== hash(readFileSync(boa.executable))) throw Error('Boa build provenance mismatch');
const rustc = execFileSync('rustc', ['-Vv'], { encoding: 'utf8' }).trim();
if (!spiderMonkey && boa.rustc !== rustc) throw Error('use the same Rust compiler for both workers');
const workers = beforeAfter ? [
  { name: 'Temiya after', engine: 'Temiya', executable: temiya, revision: process.env.TEMIYA_REVISION, mode: 'Auto Script',
    executable_sha256: hash(readFileSync(temiya)), features: [], profile: 'release' },
  { ...boa, name: 'Temiya before', engine: 'Temiya' },
] : [
  { name: 'Temiya', executable: temiya, revision: process.env.TEMIYA_REVISION, mode: 'Auto Script',
    executable_sha256: hash(readFileSync(temiya)), features: [], profile: 'release' },
  spiderMonkey ? {
    name: 'SpiderMonkey', executable: boa.executable, revision: boa.target.revision,
    mode: spiderMode, build: boa,
    arguments: [...selectedSpider.modes[spiderMode], fileURLToPath(new URL('./spidermonkey-workload-worker.js', import.meta.url)), spiderMode],
    adapter_sha256: hash(readFileSync(new URL('./spidermonkey-workload-worker.js', import.meta.url))),
  } : { name: 'omoikane vendored Boa', executable: boa.executable, revision: boa.omoikane_revision,
    mode: 'interpreter', build: boa },
];
if (regexpSuite) {
  if (manifest.workloads.length !== regexpWorkloads.length) throw Error('incomplete RegExp workload manifest');
} else if (manifest.workloads.length !== oracle.length || manifest.fixture_version !== 2) throw Error('incomplete workload manifest');
const workloads = manifest.workloads.map((entry, index) => {
  const expected = regexpSuite ? [regexpWorkloads[index].name, regexpWorkloads[index].iterations, regexpWorkloads[index].expected] : oracle[index];
  if (JSON.stringify([entry.name, entry.iterations, entry.expected]) !== JSON.stringify(expected)) throw Error('workload oracle mismatch');
  const path = join(directory, `${entry.name}.json`);
  const workload = JSON.parse(readFileSync(path));
  if (regexpSuite && (JSON.stringify(workload) !== JSON.stringify(regexpWorkloads[index]) || entry.group !== 'regexp' || entry.oracle !== regexpWorkloads[index].oracle)) throw Error('authored RegExp input/oracle changed');
  if (workload.name !== entry.name || workload.iterations !== entry.iterations || workload.expected !== entry.expected ||
      hash(workload.setup) !== entry.setup_sha256 || hash(workload.invocation) !== entry.invocation_sha256) throw Error(`input changed: ${entry.name}`);
  return { ...entry, path };
});
const timeout_ms = 180_000, rss_limit_kib = 512 * 1024;
const report = {
  comparison_kind: beforeAfter ? 'Temiya before/after' : spiderMonkey ? 'Temiya versus SpiderMonkey' : 'Temiya versus omoikane Boa',
  recorded_at: new Date().toISOString(), rustc, manifest, workers, samples, pairs,
  ...(regexpSuite ? {authored_reference_sha256:hash(readFileSync(new URL('./regexp-benchmark-reference.mjs',import.meta.url))),comparison_script_sha256:hash(readFileSync(fileURLToPath(import.meta.url)))} : {}),
  measurement_scope: 'Function/shared setup once per process; each timed pass parses and evaluates the same invocation; startup/setup/warmup excluded from samples',
  qualification: samples >= 15 && pairs >= 3 ? 'repeated comparison' : 'diagnostic only; insufficient samples/pairs for performance acceptance',
  notes: process.env.BENCH_NOTES ?? '',
  machine: { platform: os.platform(), release: os.release(), architecture: os.arch(),
    cpu_models: [...new Set(os.cpus().map(cpu => cpu.model))], logical_cpus: os.cpus().length },
  limits: { timeout_ms, rss_limit_kib, proc_poll_ms: 25,
    note: beforeAfter || spiderMonkey ? 'Same process watchdog for both workers; RSS/HWM sampled on Linux and may overshoot between polls. Neither worker has a cooperative loop/deadline budget.' : 'Same process watchdog for both workers; RSS/HWM sampled on Linux and may overshoot between polls. Boa additionally retains the application sandbox reported by its worker. Temiya has no equivalent cooperative loop/deadline budget.' },
  runs: [], comparisons: [],
};

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

for (const workload of workloads) {
  for (let pair = 0; pair < pairs; pair++) {
    const results = [];
    for (const [order, index] of (pair % 2 ? [1, 0] : [0, 1]).entries()) results[index] = await run(workload, workers[index], pair, order);
    const comparison = { workload: workload.name, pair, status: 'not_comparable' };
    if (results.every(result => result.status === 'pass')) {
      const [t, b] = results.map(result => result.measurement);
      const ratio = t.median_ns / b.median_ns;
      const threshold = Math.max(0.10, 3 * (t.mad_ns / t.median_ns + b.mad_ns / b.median_ns));
      Object.assign(comparison, { status: 'comparable', [beforeAfter ? 'after_to_before_ratio' : spiderMonkey ? 'temiya_to_spidermonkey_ratio' : 'temiya_to_boa_ratio']: ratio,
        noise_threshold: threshold,
        outcome: samples < 15 || pairs < 3 ? 'diagnostic only' : 1 - ratio > threshold ? (beforeAfter ? 'after faster beyond threshold' : 'Temiya faster beyond threshold') : ratio - 1 > threshold ? (beforeAfter ? 'after slower beyond threshold' : 'Temiya slower beyond threshold') : 'within noise threshold' });
    }
    report.comparisons.push(comparison);
  }
}
report.all_workloads_valid = report.runs.every(result => result.status === 'pass');
report[beforeAfter ? (regexpSuite ? 'all_authored_regexp_workloads_after_faster_beyond_threshold' : 'all_eleven_workloads_after_faster_beyond_threshold') : (regexpSuite ? 'all_authored_regexp_workloads_faster_beyond_threshold' : 'all_eleven_workloads_faster_beyond_threshold')] = samples >= 15 && pairs >= 3 && report.all_workloads_valid &&
  report.comparisons.every(result => result.outcome === (beforeAfter ? 'after faster beyond threshold' : 'Temiya faster beyond threshold'));
report.scope_note = beforeAfter ? 'Temiya-only before/after comparison of eleven microbenchmarks; this does not measure Boa or SpiderMonkey or establish complete application compatibility.' : 'Eleven microbenchmarks only; even a passing comparison does not establish complete application compatibility or superiority on arbitrary JavaScript.';
if (regexpSuite) report.scope_note = 'Three authored normal-case RegExp inputs, separate from the original eleven; not full ECMAScript RegExp, GC or application superiority. Construction can include engine caches. No RegExp backend/JIT execution claim.';
console.log(JSON.stringify(report, null, 2));
if (!report.all_workloads_valid) process.exitCode = 2;
