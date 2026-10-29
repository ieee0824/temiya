import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {validateObservation, canonicalObservation} from '../../scripts/differential-observation.mjs';

const root = path.dirname(fileURLToPath(import.meta.url));
const evidence = JSON.parse(fs.readFileSync(path.join(root, 'evidence.json')));
const sha = b => createHash('sha256').update(b).digest('hex');
const canonical = n => typeof n === 'string' && /^[a-zA-Z0-9_./-]+$/.test(n) &&
  !path.isAbsolute(n) && !n.split('/').some(p => !p || p === '.' || p === '..');
assert.equal(evidence.schema, 1);
assert(canonical(evidence.archive.file) && !evidence.archive.file.includes('/'));
const archive = path.join(root, evidence.archive.file), compressed = fs.readFileSync(archive);
assert.equal(compressed.length, evidence.archive.bytes); assert.equal(sha(compressed), evidence.archive.sha256);
const expected = new Map(evidence.files.map(r => [r.file, r]));
assert.equal(expected.size, evidence.files.length);
const listing = execFileSync('tar', ['-tzf', archive], {encoding: 'utf8'}).trimEnd().split('\n');
const verbose = execFileSync('tar', ['-tvzf', archive], {encoding: 'utf8'}).trimEnd().split('\n');
assert.equal(listing.length, verbose.length);
const seen = new Set();
for (const [i, entry] of listing.entries()) {
  assert(['d', '-'].includes(verbose[i][0])); if (entry === './') continue;
  const name = entry.replace(/^\.\//, '').replace(/\/$/, ''); assert(canonical(name));
  if (verbose[i][0] === 'd') continue;
  assert(expected.has(name) && !seen.has(name)); seen.add(name);
}
assert.equal(seen.size, expected.size);
const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'temiya-native-evidence-'));
let observations = 0;
try {
  execFileSync('tar', ['-xzf', archive, '--no-same-owner', '--no-same-permissions', '-C', temporary]);
  const bytes = n => {assert(canonical(n)); return fs.readFileSync(path.join(temporary, n));};
  const read = n => JSON.parse(bytes(n));
  for (const row of evidence.files) {const b = bytes(row.file); assert.equal(b.length, row.bytes); assert.equal(sha(b), row.sha256);}
  for (const declared of evidence.reports) {
    const report = read(declared.file), dir = path.posix.dirname(declared.file);
    assert.equal(report.kind, 'native-engine-differential'); assert.deepEqual(report.capture_errors, []);
    assert.equal(report.provided_temiya_build_info.source_revision, evidence.source_revision);
    assert.equal(report.provided_temiya_build_info.executable_sha256, report.temiya_sha256);
    assert.equal(report.provided_spidermonkey_build_info.binary_sha256, report.spidermonkey_sha256);
    const caseBytes = bytes(dir + '/cases.json'); assert.equal(sha(caseBytes), report.cases_sha256);
    const cases = new Map(JSON.parse(caseBytes).map(row => [row.id, row]));
    assert.equal(cases.size, report.results.length);
    if (report.known_policy_sha256) assert.equal(sha(bytes(dir + '/known-differences.json')), report.known_policy_sha256);
    const counts = {};
    for (const row of report.results) {
      assert.equal(sha(cases.get(row.id).source), row.source_sha256);
      assert.equal(sha(bytes(dir + '/' + row.id + '/reproducer.js')), row.source_sha256);
      const projections = [];
      for (const engine of ['temiya', 'spidermonkey']) {
        const result = row[engine], record = result.record;
        for (const stream of ['stdout', 'stderr']) assert.equal(sha(bytes(dir + '/' + record[stream])), record[stream + '_sha256']);
        if (result.observation) {
          assert.equal(record.exit_code, 0); assert.equal(record.signal, null); assert.equal(record.error, null);
          assert.deepEqual(read(dir + '/' + record.stdout), result.observation);
          projections.push(validateObservation(result.observation)); observations++;
          if (engine === 'spidermonkey' && report.mode === 'interpreter')
            for (const key of ['ion.enable', 'baseline.enable', 'blinterp.enable']) assert.equal(result.observation.jit_options[key], 0);
        } else projections.push(null);
      }
      let classification = 'infrastructure-failure', knownId = null, signature = null;
      if (projections.every(Boolean)) {
        classification = 'unsupported-observation';
        if (row.temiya.observation.comparison_eligible && row.spidermonkey.observation.comparison_eligible) {
          signature = sha(canonicalObservation(projections));
          classification = canonicalObservation(projections[0]) === canonicalObservation(projections[1]) ? 'match' : 'mismatch';
          if (classification === 'mismatch') {
            const rule = report.known_differences.find(r => r.source_sha256 === row.source_sha256 && r.observation_sha256 === signature);
            if (rule) {assert(rule.reason.trim() && rule.issue.trim()); classification = 'known-difference'; knownId = rule.id;}
          }
        }
      }
      assert.equal(row.classification, classification); assert.equal(row.observation_sha256, signature); assert.equal(row.known_id, knownId);
      counts[classification] = (counts[classification] ?? 0) + 1;
    }
    assert.deepEqual(counts, report.counts); assert.deepEqual(counts, declared.counts);
  }
} finally {fs.rmSync(temporary, {recursive: true, force: true});}
console.log(JSON.stringify({integrity: 'pass', run: evidence.run, files: expected.size,
  reports: evidence.reports.length, observations, scope: evidence.scope}));
