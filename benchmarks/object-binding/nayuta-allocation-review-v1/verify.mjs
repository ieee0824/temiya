import fs from 'node:fs';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
const dir=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(dir,'../../..');
const e=JSON.parse(fs.readFileSync(dir+'/evidence.json')),sha=b=>createHash('sha256').update(b).digest('hex');
assert.equal(e.source,'af728ab1f8d39a2b79391169e2cdc6ff93b02f40');
assert.equal(e.tool.tag,'v0.3.3');
assert.equal(e.tool.sourceRevision,'a693764b9ee4a771254a2fde7d05e7235dc417fc');
assert.equal(e.tool.executableSha256,'41d4694a46c77e07e625abde36c65d61f95d3cff0731473e317f3f125877c3f3');
assert.deepEqual(e.inputs.map(i=>i.path),['src/runtime/managed.rs','src/runtime/environment.rs','src/bytecode/vm.rs']);
for(const input of e.inputs) assert.equal(sha(execFileSync('git',['show',e.source+':'+input.path],{cwd:repo})),input.sha256);
for(const data of [e.findings,e.buildLog]){
 assert.equal(path.basename(data.file),data.file);
 assert.equal(sha(fs.readFileSync(dir+'/'+data.file)),data.sha256);
}
const lines=fs.readFileSync(dir+'/'+e.findings.file,'utf8').trim().split('\n');
assert.equal(lines.length,e.findings.count);assert.equal(lines.length,63);
assert(lines.every(line=>line.includes(': complexity: loop')));
assert(fs.readFileSync(dir+'/'+e.buildLog.file,'utf8').includes('Compiling nayuta v0.3.3'));
console.log(JSON.stringify({integrity:'pass',tool:e.tool.tag,source:e.source,findings:lines.length,scope:e.interpretation}));
