// Authored base class suite; the original-eleven runner remains unchanged.
import {spawn, execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {readFileSync} from 'node:fs';
import {join} from 'node:path';
import os from 'node:os';
import {fileURLToPath} from 'node:url';
import * as baseClassSuite from './class-benchmark-reference.mjs';

const args = process.argv.slice(2);
const derived = args[0] === '--derived';
const superProperties = args[0] === '--super';
const instanceFields = args[0] === '--fields';
const staticElements = args[0] === '--static';
const privateFields = args[0] === '--private';
const privateElements = args[0] === '--elements';
const iterators = args[0] === '--iterators';
const arrayBindings = args[0] === '--bindings';
const assignments = args[0] === '--assignments';
const objectBindings = args[0] === '--object-bindings';
if (derived || superProperties || instanceFields || staticElements || privateFields || privateElements || iterators || arrayBindings || objectBindings || assignments) args.shift();
const {classFixtureVersion, classManifestSha256, classReferenceRevision, classWorkloads} = assignments ? await import('./assignment-benchmark-reference.mjs') : objectBindings ? await import('./object-binding-benchmark-reference.mjs') : arrayBindings ? await import('./array-binding-benchmark-reference.mjs') : iterators ? await import('./iterator-benchmark-reference.mjs') : privateElements ? await import('./private-element-benchmark-reference.mjs') : privateFields ? await import('./private-field-benchmark-reference.mjs') : staticElements ? await import('./static-class-benchmark-reference.mjs') : instanceFields ? await import('./instance-field-benchmark-reference.mjs') : superProperties ? await import('./super-property-benchmark-reference.mjs') : derived
  ? await import('./derived-class-benchmark-reference.mjs') : baseClassSuite;
const fixtureKind = assignments ? 'authored-destructuring-assignment' : objectBindings ? 'authored-object-binding' : arrayBindings ? 'authored-array-binding' : iterators ? 'authored-sync-iterator-operations' : privateElements ? 'authored-private-class-elements' : privateFields ? 'authored-private-instance-fields' : staticElements ? 'authored-static-class-elements' : instanceFields ? 'authored-instance-fields' : superProperties ? 'authored-super-properties' : derived ? 'authored-derived-classes' : 'authored-base-classes';
const beforeAfter = args[0] === '--temiya-baseline';
if (beforeAfter) args.shift();
const [directory, temiya, referenceFile, samplesText = '15', pairsText = '3'] = args;
const samples = Number(samplesText), pairs = Number(pairsText), spiderMonkey = false;
if (args.length < 3 || args.length > 5 || !Number.isInteger(samples) || samples < 1 || samples > 1000 ||
    !Number.isInteger(pairs) || pairs < 1 || pairs > 10 || !/^[a-f0-9]{40}$/.test(process.env.TEMIYA_REVISION ?? ''))
  throw Error('set TEMIYA_REVISION; usage: compare-class-workloads.mjs [--derived|--super|--fields|--static|--private|--elements|--iterators|--bindings|--object-bindings|--assignments] [--temiya-baseline] WORKLOAD_DIR TEMIYA_WORKER REFERENCE_INFO [SAMPLES=15] [PAIRS=3]');
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
if (os.platform() !== 'linux') throw Error('Linux /proc watchdog required');
const manifestPath = join(directory, 'manifest.json'), manifestBytes = readFileSync(manifestPath);
if (hash(manifestBytes) !== classManifestSha256) throw Error('frozen class manifest changed');
const manifest = JSON.parse(manifestBytes), boa = JSON.parse(readFileSync(referenceFile));
if (manifest.fixture_kind !== fixtureKind || manifest.fixture_version !== classFixtureVersion ||
    manifest.reference_engine_revision !== classReferenceRevision || manifest.workloads.length !== classWorkloads.length)
  throw Error('incomplete or changed class suite');
const rustc = execFileSync('rustc', ['-Vv'], {encoding: 'utf8'}).trim();
if (boa.rustc !== rustc || boa.executable_sha256 !== hash(readFileSync(boa.executable))) throw Error('reference binary/compiler mismatch');
if (beforeAfter) {
  if (boa.name !== 'Temiya' || boa.mode !== 'Auto Script' || boa.profile !== 'release' ||
      !Array.isArray(boa.features) || boa.features.length !== 0) throw Error('release Temiya reference required');
} else if (boa.omoikane_revision !== classReferenceRevision || !boa.checkout_clean || boa.profile !== 'release') {
  throw Error('pinned actual Boa reference required');
}
const workers = [
  {name: beforeAfter ? 'Temiya after' : 'Temiya', engine: 'Temiya', executable: temiya,
    revision: process.env.TEMIYA_REVISION, mode: 'Auto Script', profile: 'release', features: [],
    executable_sha256: hash(readFileSync(temiya))},
  beforeAfter ? {...boa, name: 'Temiya before', engine: 'Temiya'} :
    {name: 'omoikane vendored Boa', executable: boa.executable, revision: boa.omoikane_revision,
      mode: 'interpreter', build: boa},
];
const workloads = manifest.workloads.map((entry, index) => {
  const fixed = classWorkloads[index], path = join(directory, fixed.name + '.json'), bytes = readFileSync(path);
  if (hash(bytes) !== fixed.file_sha256 || entry.sha256 !== fixed.file_sha256 || entry.name !== fixed.name ||
      entry.iterations !== fixed.iterations || entry.expected !== fixed.expected || entry.oracle !== fixed.oracle)
    throw Error('class input identity/oracle changed');
  const workload = JSON.parse(bytes);
  const expected = (instanceFields || staticElements || privateFields || privateElements || iterators || arrayBindings || objectBindings || assignments) ? fixed.iterations * (fixed.iterations + 1) / 2 : superProperties
    ? fixed.oracle === 'half-n-times42-plus-half-n-times84'
      ? (fixed.iterations / 2) * 42 + (fixed.iterations / 2) * 84
      : fixed.iterations * (fixed.iterations + 1) / 2
    : fixed.oracle === 'n' ? fixed.iterations : fixed.iterations * (fixed.iterations - 1) / 2;
  if (!Number.isSafeInteger(expected) || workload.expected !== expected || workload.setup !== fixed.setup ||
      workload.invocation !== fixed.invocation || hash(workload.setup) !== entry.setup_sha256 ||
      hash(workload.invocation) !== entry.invocation_sha256) throw Error('class source/oracle mismatch');
  return {...entry, path};
});
const timeout_ms = 180000, rss_limit_kib = 512 * 1024;
const identities = [manifestPath, ...workloads.map(row => row.path), ...workers.map(row => row.executable),
  fileURLToPath(import.meta.url), fileURLToPath(new URL(assignments ? './assignment-benchmark-reference.mjs' : objectBindings ? './object-binding-benchmark-reference.mjs' : arrayBindings ? './array-binding-benchmark-reference.mjs' : iterators ? './iterator-benchmark-reference.mjs' : privateElements ? './private-element-benchmark-reference.mjs' : privateFields ? './private-field-benchmark-reference.mjs' : staticElements ? './static-class-benchmark-reference.mjs' : instanceFields ? './instance-field-benchmark-reference.mjs' : superProperties ? './super-property-benchmark-reference.mjs' : './class-benchmark-reference.mjs', import.meta.url))]
  .map(file => ({file, sha256: hash(readFileSync(file))}));
const report = {comparison_kind: beforeAfter ? 'Temiya class before/after or control' : 'Temiya class versus actual Boa',
  recorded_at: new Date().toISOString(), rustc, manifest, workers, samples, pairs, identities,
  measurement_scope: 'Function/shared setup once per process; each timed pass parses and evaluates the same invocation; startup/setup/warmup excluded from samples',
  qualification: samples >= 15 && pairs >= 3 ? 'repeated comparison' : 'diagnostic only; insufficient samples/pairs for performance acceptance',
  notes: process.env.BENCH_NOTES ?? '',
  machine: {platform: os.platform(), release: os.release(), architecture: os.arch(),
    cpu_models: [...new Set(os.cpus().map(cpu => cpu.model))], logical_cpus: os.cpus().length,
    cpu_affinity: readFileSync('/proc/self/status', 'utf8').match(/^Cpus_allowed_list:\s*(.+)$/m)?.[1] ?? 'unknown'},
  limits: {timeout_ms, rss_limit_kib, proc_poll_ms: 25,
    note: 'Same external watchdog for both workers; RSS may overshoot between polls. Actual Boa retains its recorded application sandbox.'},
  runs: [], comparisons: []};

// Same worker/watchdog/statistics kernel as the original-eleven runner.
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
  result.stdout = stdout; result.stderr = stderr;
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
report.capture_errors = identities.flatMap(row => {try {return hash(readFileSync(row.file)) === row.sha256 ? [] : [row.file + ' changed during capture'];} catch {return [row.file + ' unavailable after capture'];}});
report.all_workloads_valid = report.capture_errors.length === 0 && report.runs.every(row => row.status === 'pass');
report.all_class_workloads_faster_beyond_threshold = samples >= 15 && pairs >= 3 && report.all_workloads_valid && report.comparisons.every(row => row.outcome === (beforeAfter ? 'after faster beyond threshold' : 'Temiya faster beyond threshold'));
report.scope_note = 'Four fixed authored base class microbenchmarks, separate from the unchanged original eleven; no whole-class, whole-corpus, application or general engine superiority claim.';
console.log(JSON.stringify(report, null, 2));
if (!report.all_workloads_valid) process.exitCode = 2;
