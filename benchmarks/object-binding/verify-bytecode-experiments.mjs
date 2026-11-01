import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import zlib from 'node:zlib';
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';

const directory = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(directory, '../..');
const root = path.join(directory, 'bytecode-experiments-v1');
const manifest = JSON.parse(fs.readFileSync(path.join(root, 'evidence.json')));
const workloads = JSON.parse(fs.readFileSync(path.join(directory, 'workloads-v1/manifest.json'))).workloads;
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const git = (...args) => execFileSync('git', ['-c', `safe.directory=${repo}`, ...args], {cwd: repo});
const summaries = [];
let rejected = 0;
assert.equal(manifest.stages.length, 31);
for (const stage of manifest.stages) {
  const dir = path.join(root, stage.name);
  assert.deepEqual(fs.readdirSync(dir).sort(), Object.keys(stage.files).sort());
  for (const [file, expected] of Object.entries(stage.files)) {
    assert.equal(hash(fs.readFileSync(path.join(dir, file))), expected, `${stage.name}/${file}`);
  }
  if (stage.rejected) {
    const rejection = JSON.parse(fs.readFileSync(path.join(dir, 'INVALID-PROVENANCE.json')));
    assert.equal(rejection.valid, false);
    assert.match(rejection.reason, /stale/);
    rejected++;
    continue;
  }
  const info = JSON.parse(fs.readFileSync(path.join(dir, 'worker-info.json')));
  const summary = JSON.parse(fs.readFileSync(path.join(dir, 'summary.json')));
  assert.equal(info.source_revision, stage.source);
  assert.equal(summary.source, stage.source);
  assert.equal(info.source_clean, true);
  assert.equal(info.profile, 'release');
  assert.deepEqual(info.features, []);
  assert.equal(info.source_tree, git('rev-parse', `${stage.source}^{tree}`).toString().trim());
  const inputs = git('ls-tree', '-r', '--name-only', stage.source, '--', 'src',
    'examples/measure_workload.rs', 'Cargo.toml', 'Cargo.lock', 'build.rs').toString().trim().split('\n');
  assert.equal(inputs.length, 83);
  assert.deepEqual(info.source_inputs.map(row => row.path), inputs);
  for (const input of info.source_inputs) {
    assert.equal(hash(git('show', `${stage.source}:${input.path}`)), input.sha256);
  }
  assert.equal(summary.rows.length, workloads.length);
  for (const [index, workload] of workloads.entries()) {
    assert.equal(hash(fs.readFileSync(path.join(directory, 'workloads-v1', workload.file))), workload.sha256);
    const row = summary.rows[index];
    assert.equal(row.name, workload.name);
    assert.equal(row.inputSha256, workload.sha256);
    const profile = zlib.gunzipSync(fs.readFileSync(path.join(dir, `${workload.name}.callgrind.gz`))).toString();
    const instructions = Number(profile.match(/^summary: (\d+)/m)[1]);
    assert.equal(instructions, Number(profile.match(/^totals: (\d+)/m)[1]));
    assert.equal(row.instructions, instructions);
    const output = JSON.parse(fs.readFileSync(path.join(dir, `${workload.name}.stdout`)));
    assert.equal(output.expected, workload.expected);
    assert.equal(output.validated_passes, 2);
    assert.equal(output.samples_ns.length, 1);
  }
  summaries.push({source: stage.source, rows: summary.rows});
}
assert.equal(rejected, 2);
assert.equal(summaries.length, 29);
const correctness = path.join(root, 'correctness');
const evidence = JSON.parse(fs.readFileSync(path.join(correctness, 'evidence.json')));
assert(summaries.some(stage => stage.source === evidence.source));
assert.equal(evidence.nativePassed, 971);
assert.equal(evidence.currentMiriTests, 2);
for (const [file, expected] of Object.entries(evidence.files)) {
  assert.equal(hash(fs.readFileSync(path.join(correctness, file))), expected);
}
const tests = fs.readFileSync(path.join(correctness, 'all-targets.log'), 'utf8');
let passed = 0, ignored = 0;
for (const result of tests.matchAll(/test result: ok\. (\d+) passed; 0 failed; (\d+) ignored;/g)) {
  passed += Number(result[1]); ignored += Number(result[2]);
}
assert.equal(passed, 971);
assert.equal(ignored, 26);
assert(!tests.includes('test result: FAILED'));
assert.match(fs.readFileSync(path.join(correctness, 'clippy.log'), 'utf8'), /Finished `dev` profile/);
assert.match(fs.readFileSync(path.join(correctness, 'release.log'), 'utf8'), /Finished `release` profile/);
for (const file of ['verifier-miri.log', 'gc-miri.log']) {
  assert.match(fs.readFileSync(path.join(correctness, file), 'utf8'), /test result: ok\. 1 passed; 0 failed; 0 ignored;/);
}
assert.match(fs.readFileSync(path.join(correctness, 'previous-7997-direct-miri.log'), 'utf8'), /test result: ok\. 4 passed; 0 failed; 0 ignored;/);
execFileSync(process.execPath, ['scripts/check-bytecode-loop-audit.mjs',
  path.join(correctness, 'loop-audit.json'), '101'], {cwd: repo});
const laterRoot = path.join(root, 'later-correctness');
const later = JSON.parse(fs.readFileSync(path.join(laterRoot, 'evidence.json')));
assert.equal(later.captures.length, 24);
for (const capture of later.captures) {
  const dir = path.join(laterRoot, capture.name);
  assert.deepEqual(fs.readdirSync(dir).sort(), Object.keys(capture.files).sort());
  for (const [file, expected] of Object.entries(capture.files)) {
    assert.equal(hash(fs.readFileSync(path.join(dir, file))), expected);
  }
  const info = JSON.parse(fs.readFileSync(path.join(dir, 'worker-info.json')));
  assert.equal(info.source_revision, capture.source);
  assert.equal(info.source_clean, true);
  assert.equal(info.profile, 'release');
  assert.deepEqual(info.features, []);
  assert.equal(info.source_tree, git('rev-parse', `${capture.source}^{tree}`).toString().trim());
  const inputs = git('ls-tree', '-r', '--name-only', capture.source, '--', 'src',
    'examples/measure_workload.rs', 'Cargo.toml', 'Cargo.lock', 'build.rs').toString().trim().split('\n');
  assert.equal(inputs.length, 83);
  assert.deepEqual(info.source_inputs.map(row => row.path), inputs);
  for (const input of info.source_inputs) {
    assert.equal(hash(git('show', `${capture.source}:${input.path}`)), input.sha256);
  }
  const log = fs.readFileSync(path.join(dir, 'all-targets.log'), 'utf8');
  assert(!log.includes('test result: FAILED'));
  const counts = [...log.matchAll(/test result: ok\. (\d+) passed; 0 failed; (\d+) ignored;/g)];
  assert.equal(counts.reduce((n, row) => n + Number(row[1]), 0), capture.nativePassed);
  assert.equal(counts.reduce((n, row) => n + Number(row[2]), 0), capture.ignored);
  assert.match(fs.readFileSync(path.join(dir, 'clippy.log'), 'utf8'), /Finished `dev` profile/);
  assert.match(fs.readFileSync(path.join(dir, 'release.log'), 'utf8'), /Finished `release` profile/);
  for (const [file, count] of capture.miri) {
    assert(fs.readFileSync(path.join(dir, `${file}.log`), 'utf8').includes(
      `test result: ok. ${count} passed; 0 failed; 0 ignored;`));
  }
  if (capture.name === 'literal-projection') {
    assert.match(fs.readFileSync(path.join(dir, 'supplemental-native.log'), 'utf8'), /test result: ok\. 4 passed; 0 failed;/);
    assert.match(fs.readFileSync(path.join(dir, 'supplemental-miri.log'), 'utf8'), /test result: ok\. 1 passed; 0 failed; 0 ignored;/);
  }
  if (capture.name === 'pinned-context') {
    assert.equal(capture.beforeCompilerSource, 'dc9bd56ca7ff473774b62218d0cbb4b8de5aeaf9');
    assert.match(capture.beforeSource, /^[a-f0-9]{40}$/);
    assert.equal(git('diff', '--name-only', capture.beforeSource, capture.beforeCompilerSource,
      '--', 'src', 'examples/measure_workload.rs', 'Cargo.toml', 'Cargo.lock', 'build.rs').length, 0);
    const before = fs.readFileSync(path.join(dir, 'before-failure.log'), 'utf8');
    assert(before.includes('Ast: function F(){this.answer=42;let read=()=>'));
    assert(before.includes('test result: FAILED. 0 passed; 1 failed;'));
  }
  if (capture.name === 'empty-loop') {
    assert.match(fs.readFileSync(path.join(dir, 'before-failure.log'), 'utf8'), /loop environment has no parent/);
  }
}
console.log(JSON.stringify({integrity: 'pass', validStages: summaries.length,
  validProfiles: summaries.length * workloads.length, rejectedStages: rejected,
  historicalNativeTests: passed, historicalFocusedMiri: 2,
  laterCorrectnessCaptures: later.captures.length,
  scope: 'Pinned inputs, source manifests and raw instruction counts only; no native speed qualification'}));
