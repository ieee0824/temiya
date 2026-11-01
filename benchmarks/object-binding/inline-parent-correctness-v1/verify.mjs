import fs from 'node:fs';
import crypto from 'node:crypto';
import zlib from 'node:zlib';
import cp from 'node:child_process';
import assert from 'node:assert/strict';
const dir=new URL('./',import.meta.url);
const capture=JSON.parse(fs.readFileSync(new URL('capture.json',dir)));
const source=cp.execFileSync('git',['show',`${capture.source}:${capture.testFile}`],{encoding:'utf8'});
const names=[...source.matchAll(/#\[test\]\s*fn (\w+)/g)].map(m=>m[1]).sort();
assert.equal(names.length,11);
assert.deepEqual(capture.tests,names);
const logs={};
for(const [name,record] of Object.entries(capture.logs)){
 const raw=zlib.gunzipSync(fs.readFileSync(new URL(record.file,dir)));
 assert.equal(crypto.createHash('sha256').update(raw).digest('hex'),record.sha256);
 logs[name]=raw.toString();
}
assert.deepEqual([...logs['miri-object'].matchAll(/^test (\w+) \.\.\. ok$/gm)].map(m=>m[1]).sort(),names);
assert.match(logs['miri-object'],/test result: ok\. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out/);
assert.match(logs.doc,/test result: ok\. 1 passed; 0 failed/);
for(const name of ['clippy','i686'])assert.match(logs[name],/Finished/);
const results=[...logs['all-targets'].matchAll(/test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored/g)];
assert.ok(results.length>0);
assert.equal(results.reduce((n,m)=>n+Number(m[1]),0),947);
assert.ok(results.every(m=>m[2]==='0'));
console.log(JSON.stringify({source:capture.source,passed:947,miriTests:names.length,docTests:1,scope:capture.scope}));
