import fs from 'node:fs';
import cp from 'node:child_process';
import crypto from 'node:crypto';
import zlib from 'node:zlib';
import assert from 'node:assert/strict';
const root=new URL('./',import.meta.url),c=JSON.parse(fs.readFileSync(new URL('capture.json',root))),cache=new Map();
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
for(const [name,sha]of Object.entries(c.uncompressedSha256)){const b=zlib.gunzipSync(fs.readFileSync(new URL(name,root)));assert.equal(hash(b),sha);cache.set(name.slice(0,-3),b.toString());}
const read=n=>JSON.parse(cache.get(n)),info=read('worker-info.json');assert.equal(info.source_revision,c.source);assert.equal(info.source_clean,true);assert.equal(info.source_inputs.length,84);for(const f of info.source_inputs)assert.equal(hash(cp.execFileSync('git',['show',c.source+':'+f.path],{maxBuffer:8*1024*1024})),f.sha256);
assert.deepEqual(c.miriFlags,['-Zmiri-strict-provenance','-Zmiri-symbolic-alignment-check']);
const source=cp.execFileSync('git',['show',c.source+':tests/parameter_environments.rs'],{encoding:'utf8'}),names=[...source.matchAll(/#\[test\]\s*fn (\w+)/g)].map(m=>m[1]).sort();assert.equal(names.length,8);
const miri=cache.get('miri.log');assert.deepEqual([...miri.matchAll(/^test (\w+) \.\.\. ok$/gm)].map(m=>m[1]).sort(),names);assert.match(miri,/test result: ok\. 8 passed; 0 failed; 0 ignored;/);
const native=cache.get('all-targets.log'),results=[...native.matchAll(/test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;/g)];assert(results.length>0);assert(results.every(m=>m[2]==='0'));assert.equal(results.reduce((n,m)=>n+ +m[1],0),1032);assert.equal(results.reduce((n,m)=>n+ +m[3],0),26);for(const n of names)assert(native.includes('test '+n+' ... ok'));assert.match(cache.get('clippy.log'),/Finished `dev` profile/);
const summary=read('validation-summary.json'),manifest=JSON.parse(fs.readFileSync(new URL('../workloads-v1/manifest.json',root)));assert.equal(summary.source,c.source);assert.equal(summary.rows.length,8);assert.equal(manifest.workloads.length,8);
for(const [i,w]of manifest.workloads.entries()){assert.equal(hash(fs.readFileSync(new URL('../workloads-v1/'+w.file,root))),w.sha256);assert.deepEqual(summary.rows[i],{name:w.name,inputSha256:w.sha256,status:0,signal:null,validated:true,validatedPasses:2});const exit=read(w.name+'.exit.json');assert.equal(exit.status,0);assert.equal(exit.signal,null);const result=read(w.name+'.stdout');assert.equal(result.expected,50005000);assert.equal(result.validated_passes,2);}
console.log(JSON.stringify({integrity:'pass',source:c.source,native:1032,ignored:26,strictMiri:8,frozenInputsValidated:8,scope:c.scope}));
