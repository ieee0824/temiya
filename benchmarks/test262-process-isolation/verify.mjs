import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {gunzipSync} from 'node:zlib';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const directory=new URL('./',import.meta.url),repository=fileURLToPath(new URL('../../',directory));
const manifest=JSON.parse(readFileSync(new URL('evidence.json',directory)));
const hash=b=>createHash('sha256').update(b).digest('hex');
const raw=new Map();
for(const entry of manifest.files){
  const compressed=readFileSync(new URL(entry.path,directory));assert.equal(hash(compressed),entry.sha256,entry.path);
  const bytes=gunzipSync(compressed);assert.equal(hash(bytes),entry.uncompressed_sha256,entry.path);raw.set(entry.path,bytes);
}
for(const [path,expected]of Object.entries(manifest.source_inputs)){
  const bytes=execFileSync('git',['-c','safe.directory='+repository,'-C',repository,'show',manifest.source+':'+path]);
  assert.equal(hash(bytes),expected,path);
}
const text=name=>raw.get('evidence/'+name+'.gz').toString();
function suite(name,expected){
  const matches=[...text(name).matchAll(/test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;/g)];
  assert.deepEqual({passed:matches.reduce((s,m)=>s+Number(m[1]),0),failed:matches.reduce((s,m)=>s+Number(m[2]),0),ignored:matches.reduce((s,m)=>s+Number(m[3]),0),groups:matches.length},expected,name);
}
suite('native.log',{passed:705,failed:0,ignored:26,groups:92});
suite('msrv.log',{passed:705,failed:0,ignored:26,groups:92});
for(const log of ['isolation-tests.log','isolation-kernel-limits-tests.log'])suite(log,{passed:6,failed:0,ignored:0,groups:1});
assert.ok(text('clippy.log').includes('Finished `dev` profile'));
assert.ok(text('c-abi.log').includes('Finished `release` profile'));
assert.equal(text('nayuta-final.err'),'nayuta: scanned 6 Rust file(s), found 24 issue(s)\n');
assert.equal(text('nayuta-final.out').trim().split('\n').length,24);
const ci=JSON.parse(text('ci-status.json'));
assert.equal(ci.headSha,manifest.source);assert.equal(ci.conclusion,'success');assert.equal(ci.status,'completed');
assert.equal(ci.jobs.length,4);assert.ok(ci.jobs.every(j=>j.conclusion==='success'));
const log=text('pinned-original.log'),reports=[];
for(let i=0;i<log.length;i++){
  if(log[i]!=='{')continue;
  let depth=0,string=false,escape=false,end=i;
  for(;end<log.length;end++){
    const c=log[end];if(string){if(escape)escape=false;else if(c==='\\')escape=true;else if(c==='"')string=false;}
    else if(c==='"')string=true;else if(c==='{')depth++;else if(c==='}'&&--depth===0){end++;break;}
  }
  try{const r=JSON.parse(log.slice(i,end));if(r.schema===1&&r.counts&&Array.isArray(r.results))reports.push(r);i=end-1;}catch{}
}
const total={pass:0,fail:0,skip:0};
for(const r of reports){
  assert.equal(r.corpus_revision,manifest.corpus);
  const counts={pass:0,fail:0,skip:0};for(const row of r.results){assert.ok(Object.hasOwn(counts,row.status));counts[row.status]++;}
  assert.deepEqual(counts,r.counts);for(const key of Object.keys(total))total[key]+=counts[key];
}
assert.equal(reports.length,9);assert.deepEqual(total,{pass:247,fail:0,skip:0});
assert.deepEqual(JSON.parse(text('pinned-original-summary.json')),{reports:9,counts:total,corpus_revisions:[manifest.corpus]});
// Early failures are separate records, never relabelled as successful checks.
assert.ok(text('isolation-tests-initial-sigsegv.log').includes('5 passed; 1 failed;'));
assert.ok(text('native-initial-jit-policy.log').includes('jit_is_optional_and_default_build_has_no_jit_dependency ... FAILED'));
assert.ok(text('native-offline-metadata-failure.log').includes('attempting to make an HTTP request, but --offline was specified'));
console.log(JSON.stringify({integrity:'pass',files:manifest.files.length,source_inputs:Object.keys(manifest.source_inputs).length,native:705,msrv:705,original_variants:total,qualification:manifest.qualification}));
