import fs from 'node:fs';
import zlib from 'node:zlib';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
const root=new URL('./',import.meta.url),e=JSON.parse(fs.readFileSync(new URL('evidence.json',root))),hash=b=>crypto.createHash('sha256').update(b).digest('hex');
const cache=new Map();for(const [name,sha]of Object.entries(e.uncompressedSha256)){const b=zlib.gunzipSync(fs.readFileSync(new URL(name,root)));assert.equal(hash(b),sha);cache.set(name.slice(0,-3),b);}
const raw=n=>{const b=cache.get(n);assert(b,n);return b;},read=n=>JSON.parse(raw(n));
const info=read('raw/build-info.json');assert.equal(info.vendorRevision,e.vendorSource);assert.equal(info.vendorClean,true);for(const f of info.adapterInputs)assert.equal(hash(raw('raw/adapter/'+f.path)),f.sha256);
assert(raw('raw/adapter/src/main.rs').toString().includes('Context::default()'));
const temiya=read('temiya-worker-info.json');assert.equal(temiya.source_revision,e.temiyaSource);assert.equal(temiya.source_clean,true);assert.equal(temiya.source_inputs.length,84);for(const f of temiya.source_inputs)assert.equal(hash(execFileSync('git',['show',e.temiyaSource+':'+f.path],{maxBuffer:8*1024*1024})),f.sha256);
const manifest=JSON.parse(fs.readFileSync(new URL('../workloads-v1/manifest.json',root))),summary=read('raw/summary.json');assert.equal(summary.rows.length,12);
const originals=summary.rows.filter(r=>r.kind==='original-frozen');assert.equal(originals.length,8);
const failed=new Set(['parameter-env-var-copy','parameter-env-nested-default','parameter-env-computed-key','parameter-env-rest-copy','parameter-env-constructor']);
for(const w of manifest.workloads){const row=originals.find(r=>r.name===w.name);assert(row);assert.equal(hash(raw('raw/'+w.name+'.input.json')),w.sha256);assert.equal(row.inputSha256,w.sha256);assert.equal(row.status,failed.has(w.name)?1:0);assert.equal(row.signal,null);assert.equal(row.error,null);if(row.status===0)assert.equal(read('raw/'+w.name+'.stdout').validated,true);else assert(raw('raw/'+w.name+'.stderr').toString().includes('Error:'));}
const embedded=read('embedding/summary.json'),t=read('embedding/temiya-summary.json');assert.equal(embedded.rows.length,4);assert.equal(t.rows.length,4);assert.equal(t.source,e.temiyaSource);
const probes=summary.rows.filter(r=>r.kind==='minimal-probe');assert.equal(probes.length,4);
const probeNames=['body-initializer','no-body-var','single-script-var-copy','split-var-copy'];
for(const rows of [probes,embedded.rows,t.rows])assert.deepEqual(rows.map(r=>r.name).sort(),probeNames);
for(const p of probes){const ok=p.name==='no-body-var';assert.equal(p.status,ok?0:1);assert.equal(embedded.rows.find(r=>r.name===p.name).status,p.status);assert.equal(hash(raw('raw/'+p.name+'.input.json')),hash(raw('embedding/'+p.name+'.json')));assert.equal(t.rows.find(r=>r.name===p.name).status,0);const positive=read('embedding/'+p.name+'.temiya.stdout');assert.equal(positive.expected,42);assert.equal(positive.validated_passes,2);if(!ok)assert(raw('raw/'+p.name+'.stderr').toString().includes('ReferenceError: access of uninitialized binding'));}
console.log(JSON.stringify({integrity:'pass',originals:8,rawBoaPass:3,rawBoaFail:5,minimalProbes:4,temiyaProbePass:4,scope:e.scope}));
