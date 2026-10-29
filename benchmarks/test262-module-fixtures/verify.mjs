import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {gunzipSync} from 'node:zlib';
const root=new URL('./',import.meta.url);
const manifest=JSON.parse(readFileSync(new URL('evidence.json',root)));
const hash=b=>createHash('sha256').update(b).digest('hex');
const raw=new Map();
for(const entry of manifest.files){
  const compressed=readFileSync(new URL(entry.path,root));
  assert.equal(hash(compressed),entry.sha256,entry.path);
  const bytes=gunzipSync(compressed);
  assert.equal(hash(bytes),entry.uncompressed_sha256,entry.path);
  raw.set(entry.path,bytes);
}
const json=name=>JSON.parse(raw.get('evidence/'+name+'.json.gz'));
function report(name,expected){
  const r=json(name);assert.equal(r.corpus_revision,manifest.corpus);
  const counts={pass:0,fail:0,skip:0};
  const keys=new Set();
  for(const row of r.results){
    assert.ok(Object.hasOwn(counts,row.status));counts[row.status]++;
    const key=JSON.stringify([row.path,row.variant]);assert.ok(!keys.has(key),'duplicate variant');keys.add(key);
  }
  assert.deepEqual(counts,expected,name);
  if(r.counts)assert.deepEqual({...counts,...r.counts},counts,name+' declared counts');
  return r;
}
for(const backend of ['ast','auto']){
  report('focused-before-link-fix-'+backend,{pass:14,fail:4,skip:0});
  report('focused-after-link-fix-'+backend,{pass:16,fail:2,skip:0});
}
report('instantiation-auto',{pass:55,fail:21,skip:0});
const tla=report('tla-auto',{pass:171,fail:82,skip:0});
const paths=json('tla-paths');assert.equal(paths.length,251);
assert.equal(tla.results.length,253);assert.equal(tla.process.terminal,true);
assert.ok(tla.process.records.every(r=>[0,1].includes(r.status)&&r.signal===null));
assert.deepEqual([...new Set(tla.results.map(r=>r.path))].sort(),[...paths].sort());
assert.equal(tla.source,manifest.source);
assert.equal(json('unicode-alias-before').corpus_revision,null);
console.log(JSON.stringify({integrity:'pass',files:manifest.files.length,tla:{files:251,variants:253,pass:171,fail:82,skip:0},qualification:'Functional evidence only; does not qualify performance or full TLA support.'}));
