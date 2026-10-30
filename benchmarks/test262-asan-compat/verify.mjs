import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {gunzipSync} from 'node:zlib';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),e=JSON.parse(fs.readFileSync(path.join(root,'evidence.json')));
const hash=b=>createHash('sha256').update(b).digest('hex'),logs=new Map();
for(const row of e.files){
  assert(/^[a-zA-Z0-9_.-]+$/.test(row.file)&&!row.file.includes('..'));
  const packed=fs.readFileSync(path.join(root,row.file));assert.equal(hash(packed),row.sha256);
  const bytes=gunzipSync(packed);assert.equal(bytes.length,row.decoded_bytes);assert.equal(hash(bytes),row.decoded_sha256);
  logs.set(row.file,bytes.toString());
}
assert(logs.get('initial-failure.log.gz').includes('ReserveShadowMemoryRange failed'));
function counts(log){
  const rows=[...log.matchAll(/test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored/g)];
  assert(!log.includes('test result: FAILED'));
  return {passed:rows.reduce((s,r)=>s+Number(r[1]),0),failed:rows.reduce((s,r)=>s+Number(r[2]),0),ignored:rows.reduce((s,r)=>s+Number(r[3]),0),groups:rows.length};
}
assert.deepEqual(counts(logs.get('native-all-features.log.gz')),{passed:731,failed:0,ignored:26,groups:96});
const sanitized=logs.get('asan-40962391808.log.gz');
assert.deepEqual(counts(sanitized),{passed:729,failed:0,ignored:26,groups:95});
assert(sanitized.includes('RUSTFLAGS: -Zsanitizer=address'));
assert(sanitized.includes(e.asan_fix_verification.source_revision));
for(const name of ['stack_and_allocation_exhaustion_are_distinct_and_never_satisfy_negatives','externally_signalled_worker_crashes_do_not_kill_the_suite_or_imply_oom','actual_async_completion_drains_jobs_and_preserves_child_fifo'])assert(sanitized.includes('test '+name+' ... ok'));
console.log(JSON.stringify({integrity:'pass',native:counts(logs.get('native-all-features.log.gz')),asan:counts(sanitized),source:e.asan_fix_verification.source_revision,scope:'Historical actual ASAN resource compatibility coverage; no uninstrumented-worker substitution and no claim of a virtual-address cap in the sanitized profile.'}));
