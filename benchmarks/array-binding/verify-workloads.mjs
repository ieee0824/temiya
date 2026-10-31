import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {fileURLToPath} from 'node:url';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {classFixtureVersion, classManifestSha256, classReferenceRevision, classWorkloads} from '../../scripts/array-binding-benchmark-reference.mjs';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..');
const sha=b=>createHash('sha256').update(b).digest('hex'),read=p=>JSON.parse(fs.readFileSync(p));
const frozen=JSON.parse(execFileSync('git',['-c','safe.directory='+repo,'show','eb0adf89b872896a028007ad2564a98940ab5932:benchmarks/array-binding/workload-freeze.json'],{cwd:repo,encoding:'utf8'}));
assert.deepEqual(read(root+'/workload-freeze.json'),frozen);
const bytes=fs.readFileSync(root+'/workloads-v1/manifest.json'),manifest=JSON.parse(bytes);
assert.equal(sha(bytes),classManifestSha256);assert.equal(manifest.fixture_version,classFixtureVersion);
assert.equal(manifest.fixture_kind,'authored-array-binding');assert.equal(manifest.reference_engine_revision,classReferenceRevision);
assert.equal(manifest.frozen_at,frozen.time);assert.equal(manifest.workloads.length,6);assert.equal(classWorkloads.length,6);
for(let i=0;i<6;i++){
 const freeze=frozen.inputs[i],entry=manifest.workloads[i],reference=classWorkloads[i];
 assert.equal(entry.name,freeze.name);assert.equal(entry.file,freeze.file);assert.equal(entry.sha256,freeze.sha256);
 const raw=fs.readFileSync(root+'/workloads-v1/'+entry.file),w=JSON.parse(raw);assert.equal(sha(raw),freeze.sha256);
 assert.equal(w.name,entry.name);assert.equal(w.iterations,10000);assert.equal(w.expected,50005000);
 assert.equal(entry.setup_sha256,sha(w.setup));assert.equal(entry.invocation_sha256,sha(w.invocation));
 assert.deepEqual(reference,{...w,file_sha256:freeze.sha256,oracle:entry.oracle});
 const output=read(root+'/boa-oracles-v1/'+entry.name+'.json');assert.equal(output.expected,w.expected);assert.equal(output.validated_passes,2);
}
const boa=read(root+'/boa-oracles-v1/build-info.json');assert.equal(boa.omoikane_revision,classReferenceRevision);assert.equal(boa.checkout_clean,true);assert.equal(boa.profile,'release');
console.log(JSON.stringify({integrity:'pass',frozen_source:'eb0adf89b872896a028007ad2564a98940ab5932',workloads:6,boa_validated_outputs:12,scope:'Frozen input/manifest/reference/oracle identity only; no speed qualification.'}));
