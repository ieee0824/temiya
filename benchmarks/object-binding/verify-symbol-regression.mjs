import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..');
const dir=root+'/symbol-constructor-regression-v1',read=p=>JSON.parse(fs.readFileSync(p));
const e=read(dir+'/evidence.json'),info=read(dir+'/engine-info.json');
assert.equal(e.source,'283ef0575521a2c402499b1c9de90030a04e9671');
assert.equal(info.source_revision,e.source);assert.equal(info.source_clean,true);
for(const [file,hash]of Object.entries(e.files)){
 assert(!path.isAbsolute(file)&&!file.split('/').includes('..'));
 assert.equal(createHash('sha256').update(fs.readFileSync(dir+'/'+file)).digest('hex'),hash,file);
}
const test='test/built-ins/Symbol/is-constructor.js';
const gate=read(dir+'/original-gate.json');assert.equal(gate.passed,false);
assert.deepEqual(gate.failures.map(r=>[r.path,r.variant,r.reason]),['Sloppy','Strict'].map(v=>[test,v,'known pass regressed to fail']));
const original=read(dir+'/original-raw.json').results.filter(r=>r.path===test);
assert.deepEqual(original.map(r=>[r.variant,r.status]),[['Sloppy','fail'],['Strict','fail']]);
for(const mode of ['ast','auto']){
 execFileSync(process.execPath,[repo+'/scripts/verify-test262-baseline.mjs',dir+'/'+mode],{cwd:repo});
 const c=read(dir+'/'+mode+'/conditions.json');
 assert.equal(c.engine_sha256,info.executable_sha256);assert.equal(c.provided_engine_build_info.source_revision,e.source);
 assert.equal(c.backend_requested,mode);assert.equal(c.timeout_ms,1000);assert.equal(c.memory_mb,512);assert.equal(c.stack_kb,8192);
 assert.deepEqual(read(dir+'/'+mode+'/outcomes.json').map(r=>[r.path,r.variant,r.outcome]),['Sloppy','Strict'].map(v=>[test,v,'pass']));
}
console.log(JSON.stringify({integrity:'pass',original_failures:2,unchanged_original_program_passes:4,scope:e.scope}));
