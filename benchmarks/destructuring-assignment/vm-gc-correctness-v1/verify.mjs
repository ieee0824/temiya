import fs from 'node:fs';
import cp from 'node:child_process';
import crypto from 'node:crypto';
import zlib from 'node:zlib';
import assert from 'node:assert/strict';
const root=new URL('./',import.meta.url),c=JSON.parse(fs.readFileSync(new URL('capture.json',root)));
assert.equal(cp.execFileSync('git',['diff',c.runtimeSource,c.source,'--','src','Cargo.toml','Cargo.lock','build.rs','examples/measure_workload.rs'],{encoding:'utf8'}),'');
assert.deepEqual(c.miriFlags,['-Zmiri-strict-provenance','-Zmiri-symbolic-alignment-check']);
const source=cp.execFileSync('git',['show',c.source+':tests/bytecode_assignment_gc.rs'],{encoding:'utf8'});
assert.match(source,/assert_eq!\(script.backend\(\), ScriptBackend::Bytecode\)/);
assert.match(source,/engine.collect_garbage\(\).unwrap\(\).retained, 0/);
for(const [name,r]of Object.entries(c.logs)){
 const b=zlib.gunzipSync(fs.readFileSync(new URL(r.file,root)));assert.equal(crypto.createHash('sha256').update(b).digest('hex'),r.sha256);
 const text=b.toString();
 if(name==='clippy')assert.match(text,/Finished `dev` profile/);
 else {assert(text.includes('test '+c.test+' ... ok'));assert.match(text,/test result: ok\. 1 passed; 0 failed; 0 ignored;/);}
}
console.log(JSON.stringify({source:c.source,runtimeSource:c.runtimeSource,native:1,strictMiri:1,scope:c.scope}));
