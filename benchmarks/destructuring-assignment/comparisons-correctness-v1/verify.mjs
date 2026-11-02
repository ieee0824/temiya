import fs from 'node:fs';
import cp from 'node:child_process';
import crypto from 'node:crypto';
import zlib from 'node:zlib';
import assert from 'node:assert/strict';
const root=new URL('./',import.meta.url),c=JSON.parse(fs.readFileSync(new URL('capture.json',root))),logs={},hash=b=>crypto.createHash('sha256').update(b).digest('hex');
const info=JSON.parse(fs.readFileSync(new URL('build-info.json',root)));assert.equal(info.source_revision,c.source);assert.equal(info.source_clean,true);assert.equal(info.source_inputs.length,83);for(const f of info.source_inputs)assert.equal(hash(cp.execFileSync('git',['show',c.source+':'+f.path],{maxBuffer:8*1024*1024})),f.sha256);
for(const [name,r]of Object.entries(c.logs)){const b=zlib.gunzipSync(fs.readFileSync(new URL(r.file,root)));assert.equal(hash(b),r.sha256);logs[name]=b.toString();}
assert.deepEqual(c.miriFlags,['-Zmiri-strict-provenance','-Zmiri-symbolic-alignment-check']);
const results=[...logs.native.matchAll(/test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;/g)];assert(results.length>0);assert(results.every(m=>m[2]==='0'));assert.equal(results.reduce((n,m)=>n+ +m[1],0),1018);assert.equal(results.reduce((n,m)=>n+ +m[3],0),26);assert.match(logs.clippy,/Finished `dev` profile/);
const source=p=>cp.execFileSync('git',['show',c.source+':'+p],{encoding:'utf8'}),testNames=p=>[...source(p).matchAll(/#\[test\]\s*fn (\w+)/g)].map(m=>m[1]);
const expected={
 'miri-instance':testNames('src/runtime/object.rs').filter(n=>n.startsWith('default_instance_')).map(n=>'runtime::object::default_instance_tests::'+n),
 'miri-strict':testNames('tests/bytecode_binary.rs').filter(n=>n.startsWith('strict_equality_preserves_')),
 'miri-operands':testNames('tests/bytecode_operand_storage.rs'),
 'miri-assignment':['tests/bytecode_assignment_gc.rs','tests/bytecode_assignments.rs','tests/destructuring_assignment.rs'].flatMap(testNames),
};assert.deepEqual(Object.values(expected).map(a=>a.length),[2,1,1,15]);
let total=0;for(const [name,names]of Object.entries(expected)){assert.deepEqual([...logs[name].matchAll(/^test (\S+) \.\.\. ok$/gm)].map(m=>m[1]).sort(),names.sort());const parts=[...logs[name].matchAll(/test result: ok\. (\d+) passed; 0 failed; 0 ignored;/g)];assert.equal(parts.reduce((n,m)=>n+ +m[1],0),names.length);total+=names.length;}assert.equal(total,19);
console.log(JSON.stringify({integrity:'pass',source:c.source,native:1018,ignored:26,strictMiri:19,scope:c.scope}));
