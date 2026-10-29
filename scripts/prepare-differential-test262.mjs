// Read immutable originals and the real CLI's metadata; never rewrite assertions.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
const [engine, corpus, destination] = process.argv.slice(2);
if (process.argv.length !== 5)
  throw Error('usage: node scripts/prepare-differential-test262.mjs TEST262_CLI CORPUS NEW_CASES_JSON');
assert(!fs.existsSync(destination) && !fs.existsSync(destination + '.metadata.json'), 'output must be new');
const manifest = JSON.parse(fs.readFileSync(new URL('../test262/manifest.json', import.meta.url)));
const selected = JSON.parse(fs.readFileSync(new URL('../testing/differential/test262-selection.json', import.meta.url)));
const raw = execFileSync(path.resolve(engine), ['--corpus', path.resolve(corpus), '--pinned',
  '--metadata-only', ...selected], {maxBuffer: 16 * 1024 * 1024});
const metadata = JSON.parse(raw);
assert.equal(metadata.kind, 'test262-metadata');
assert.equal(metadata.engine_executed, false);
assert.equal(metadata.corpus_revision, manifest.revision);
assert.equal(metadata.manifest_revision, manifest.revision);
assert.deepEqual(metadata.cases.map(test => test.path), selected);
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const source = name => {
  assert(typeof name === 'string' && !path.posix.isAbsolute(name) &&
    !name.split(/[\\/]/).some(part => !part || part === '.' || part === '..'));
  return execFileSync('git', ['-c', 'safe.directory=' + path.resolve(corpus), '-C', path.resolve(corpus),
    'show', manifest.revision + ':' + name], {maxBuffer: 16 * 1024 * 1024});
};
const cases = [];
for (const [index, test] of metadata.cases.entries()) {
  assert.equal(test.parsed, true);
  assert.equal(test.engine_executed, false);
  const harness = test.flags.includes('raw') ? [] :
    ['harness/sta.js', 'harness/assert.js', ...test.includes.map(name => 'harness/' + name)];
  const names = [...new Set(harness), test.path];
  const parts = names.map(name => ({name, bytes: source(name)}));
  for (const variant of test.variants) {
    assert(['Sloppy', 'Strict'].includes(variant), 'this adapter selection requires Script variants');
    const directive = variant === 'Strict' ? "'use strict';\n" : '';
    cases.push({id: 'original-' + index + '-' + variant.toLowerCase(),
      source: directive + parts.map(part => {
        const text = part.bytes.toString('utf8');
        assert(Buffer.from(text).equals(part.bytes), 'original source must be valid UTF-8');
        return text;
      }).join('\n'),
      provenance: {corpus_revision: manifest.revision, path: test.path, variant,
        metadata: test, strict_directive_added: variant === 'Strict',
        original_parts: parts.map(part => ({path: part.name, sha256: sha(part.bytes), bytes: part.bytes.length})),
        scope: 'Original harness and body bytes retained; only Test262 strict directive and separating newlines added. No body trailer or assertion replacement.'}});
  }
}
fs.writeFileSync(destination + '.metadata.json', raw, {flag: 'wx'});
fs.writeFileSync(destination, JSON.stringify(cases, null, 2) + '\n', {flag: 'wx'});
console.log(JSON.stringify({cases: cases.length, files: selected.length, corpus_revision: manifest.revision,
  engine_executed_by_preparation: false}));
