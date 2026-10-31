// Integrity checks preserve slow/noisy results; they do not establish speedup.
import fs from 'node:fs';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import * as baseClassSuite from './class-benchmark-reference.mjs';

const args = process.argv.slice(2);
const derived = args[0] === '--derived';
const superProperties = args[0] === '--super';
const instanceFields = args[0] === '--fields';
const staticElements = args[0] === '--static';
if (derived || superProperties || instanceFields || staticElements) args.shift();
const {classFixtureVersion, classManifestSha256, classReferenceRevision, classWorkloads} = staticElements ? await import('./static-class-benchmark-reference.mjs') : instanceFields ? await import('./instance-field-benchmark-reference.mjs') : superProperties ? await import('./super-property-benchmark-reference.mjs') : derived
  ? await import('./derived-class-benchmark-reference.mjs') : baseClassSuite;
assert(args.length > 0, 'provide authored class comparison reports');
const sha = b => createHash('sha256').update(b).digest('hex');
const median = values => {const s = [...values].sort((a, b) => a - b), i = s.length >> 1;
  return s.length % 2 ? s[i] : Math.floor((s[i - 1] + s[i]) / 2);};
const reports = [];
for (const file of args) {
  const bytes = fs.readFileSync(file), r = JSON.parse(bytes);
  const before = r.comparison_kind === 'Temiya class before/after or control';
  assert(before || r.comparison_kind === 'Temiya class versus actual Boa');
  assert.equal(sha(JSON.stringify(r.manifest, null, 2) + '\n'), classManifestSha256);
  assert.equal(r.manifest.fixture_kind, staticElements ? 'authored-static-class-elements' : instanceFields ? 'authored-instance-fields' : superProperties ? 'authored-super-properties' : derived ? 'authored-derived-classes' : 'authored-base-classes');
  assert.equal(r.manifest.fixture_version, classFixtureVersion);
  assert.equal(r.manifest.reference_engine_revision, classReferenceRevision);
  assert.deepEqual(r.capture_errors, []);
  assert.equal(r.samples, 15); assert.equal(r.pairs, 3); assert.equal(r.workers.length, 2);
  assert.equal(r.runs.length, classWorkloads.length * 6); assert.equal(r.comparisons.length, classWorkloads.length * 3);
  assert.equal(r.limits.timeout_ms, 180000); assert.equal(r.limits.rss_limit_kib, 524288);
  assert.equal(r.limits.proc_poll_ms, 25);
  assert.equal(r.qualification, 'repeated comparison');
  const identities = new Map();
  for (const row of r.identities) {
    assert.match(row.sha256, /^[a-f0-9]{64}$/);
    if (identities.has(row.file)) assert.equal(identities.get(row.file), row.sha256);
    identities.set(row.file, row.sha256);
  }
  for (const worker of r.workers) {
    const info = worker.build ?? worker;
    assert.equal(identities.get(worker.executable), info.executable_sha256);
    if (worker.build) {assert.equal(info.omoikane_revision, classReferenceRevision); assert.equal(info.checkout_clean, true);}
  }
  const counts = {};
  for (const [index, expected] of classWorkloads.entries()) {
    const entry = r.manifest.workloads[index];
    assert.equal(entry.name, expected.name); assert.equal(entry.iterations, expected.iterations);
    assert.equal(entry.expected, expected.expected); assert.equal(entry.sha256, expected.file_sha256);
    assert.equal(entry.setup_sha256, sha(expected.setup)); assert.equal(entry.invocation_sha256, sha(expected.invocation));
    for (let pair = 0; pair < 3; pair++) {
      const matches = r.workers.map(worker => r.runs.filter(row => row.workload === expected.name && row.pair === pair && row.worker === worker.name));
      assert(matches.every(rows => rows.length === 1)); const runs = matches.map(rows => rows[0]);
      assert.equal(runs[0].order, pair % 2); assert.equal(runs[1].order, 1 - pair % 2);
      for (const [worker, run] of runs.entries()) {
        assert.equal(run.status, 'pass'); assert.deepEqual(run.exit, {code: 0, signal: null});
        assert.equal(run.stderr, '');
        const m = run.measurement, {median_ns, mad_ns, ...raw} = m;
        assert.deepEqual(JSON.parse(run.stdout), raw);
        assert.equal(m.engine, r.workers[worker].engine ?? r.workers[worker].name);
        assert.equal(m.mode, r.workers[worker].mode); assert.equal(m.profile, 'release');
        assert.equal(m.architecture, {arm64: 'aarch64', x64: 'x86_64'}[r.machine.architecture] ?? r.machine.architecture);
        assert.equal(m.scope, 'parse+evaluate invocation; function setup excluded');
        assert.equal(m.expected, expected.expected); assert.equal(m.validated_passes, 16);
        assert.equal(m.samples_ns.length, 15); assert(m.samples_ns.every(n => Number.isSafeInteger(n) && n > 0));
        assert(Number.isSafeInteger(m.warmup_ns) && m.warmup_ns > 0);
        assert.equal(median_ns, median(m.samples_ns));
        assert.equal(mad_ns, median(m.samples_ns.map(n => Math.abs(n - median_ns))));
      }
      const [a, b] = runs.map(run => run.measurement), ratio = a.median_ns / b.median_ns;
      const threshold = Math.max(.1, 3 * (a.mad_ns / a.median_ns + b.mad_ns / b.median_ns));
      const outcome = 1 - ratio > threshold ? before ? 'after faster beyond threshold' : 'Temiya faster beyond threshold' :
        ratio - 1 > threshold ? before ? 'after slower beyond threshold' : 'Temiya slower beyond threshold' : 'within noise threshold';
      const matchesComparison = r.comparisons.filter(c => c.workload === expected.name && c.pair === pair);
      assert.equal(matchesComparison.length, 1); const comparison = matchesComparison[0];
      assert.equal(comparison.status, 'comparable'); assert.equal(comparison.noise_threshold, threshold);
      assert.equal(comparison[before ? 'after_to_before_ratio' : 'temiya_to_boa_ratio'], ratio);
      assert.equal(comparison.outcome, outcome); counts[outcome] = (counts[outcome] ?? 0) + 1;
    }
  }
  assert.equal(r.all_workloads_valid, true);
  assert.equal(r.all_class_workloads_faster_beyond_threshold,
    counts[before ? 'after faster beyond threshold' : 'Temiya faster beyond threshold'] === classWorkloads.length * 3);
  reports.push({file, sha256: sha(bytes), runs: r.runs.length, checked_values: r.runs.length * 16, comparisons: r.comparisons.length, outcomes: counts});
}
console.log(JSON.stringify({integrity: 'pass', reports, scope: 'Recomputed recorded correctness/statistics, including unfavorable results; not an unconditional performance pass.'}, null, 2));
