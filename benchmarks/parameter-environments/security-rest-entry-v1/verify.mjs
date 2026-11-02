import path from 'node:path';
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const dir=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(dir,'../../..');
const result=JSON.parse(execFileSync(process.execPath,[repo+'/benchmarks/object-binding/security-prepared-guards-v1/verify.mjs',dir,'9b6d8ee94b88f91d7a44d124781fba9f3fd7eb12'],{encoding:'utf8'}));
assert.equal(result.run,41390394610);assert.equal(result.jobs,19);assert.equal(result.miriTests,181);assert.equal(result.asanTests,1042);
console.log(JSON.stringify(result));
