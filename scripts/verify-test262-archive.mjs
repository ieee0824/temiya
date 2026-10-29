// Decode a preserved capture and independently reconstruct its actual outcomes.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';

if (process.argv.length < 3 || process.argv.length > 4)
  throw Error('usage: node scripts/verify-test262-archive.mjs ARCHIVE_DIR [NEW_DESTINATION]');
const root = path.resolve(process.argv[2]);
const descriptor = JSON.parse(fs.readFileSync(path.join(root, 'capture.json')));
assert.equal(descriptor.schema, 1);
assert.equal(descriptor.kind, 'test262-complete-capture-archive');
const canonical = name => typeof name === 'string' && /^[a-zA-Z0-9_./-]+$/.test(name) &&
  !path.isAbsolute(name) && !name.split('/').some(s => !s || s === '.' || s === '..');
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
assert(canonical(descriptor.archive.file) && !descriptor.archive.file.includes('/'));
const archive = path.join(root, descriptor.archive.file);
const archiveBytes = fs.readFileSync(archive);
assert.equal(archiveBytes.length, descriptor.archive.bytes);
assert.equal(sha(archiveBytes), descriptor.archive.sha256, 'compressed archive identity');
const expected = new Map();
for (const row of descriptor.files) {
  assert(canonical(row.file));
  assert(!expected.has(row.file), 'duplicate declared capture file');
  expected.set(row.file, row);
}
const listing = execFileSync('tar', ['-tzf', archive], {encoding: 'utf8', maxBuffer: 32 * 1024 * 1024})
  .trimEnd().split('\n');
const verbose = execFileSync('tar', ['-tvzf', archive], {encoding: 'utf8', maxBuffer: 32 * 1024 * 1024})
  .trimEnd().split('\n');
assert.equal(listing.length, verbose.length);
const listed = new Set();
for (const [index, entry] of listing.entries()) {
  assert(['-', 'd'].includes(verbose[index][0]), 'capture archive contains a non-regular entry');
  if (entry === './') continue;
  const name = entry.replace(/^\.\//, '').replace(/\/$/, '');
  assert(canonical(name), 'noncanonical archive entry');
  if (verbose[index][0] === 'd') continue;
  assert(expected.has(name) && !listed.has(name), 'unexpected or duplicate archive file');
  listed.add(name);
}
assert.equal(listed.size, expected.size, 'archive omitted a declared file');
const persistent = process.argv[3] !== undefined;
const destination = persistent ? path.resolve(process.argv[3]) : fs.mkdtempSync(path.join(os.tmpdir(), 'temiya-capture-verify-'));
if (persistent) {
  assert(!fs.existsSync(destination), 'destination must be new');
  fs.mkdirSync(destination, {recursive: true});
}
try {
  execFileSync('tar', ['-xzf', archive, '--no-same-owner', '--no-same-permissions', '-C', destination]);
  for (const [name, row] of expected) {
    const bytes = fs.readFileSync(path.join(destination, name));
    assert.equal(bytes.length, row.bytes, name);
    assert.equal(sha(bytes), row.sha256, name);
  }
  assert.equal(sha(fs.readFileSync(path.join(destination, 'baseline.json'))), descriptor.baseline_sha256);
  const verifier = fileURLToPath(new URL('./verify-test262-baseline.mjs', import.meta.url));
  const report = JSON.parse(execFileSync(process.execPath, [verifier, destination],
    {encoding: 'utf8', maxBuffer: 16 * 1024 * 1024}));
  assert.deepEqual(report, descriptor.verification);
  assert.deepEqual(report, descriptor.roundtrip_verification);
  assert.equal(report.corpus_revision, descriptor.corpus_revision);
  console.log(JSON.stringify({archive_integrity: 'pass', decoded_files: expected.size,
    ...(persistent ? {destination} : {}), verification: report}));
} finally {
  if (!persistent) fs.rmSync(destination, {recursive: true, force: true});
}
