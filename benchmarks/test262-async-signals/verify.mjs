// Independently verify preserved samples and identities. Valid data can include regressions.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {gunzipSync} from 'node:zlib';
import {fileURLToPath} from 'node:url';
const directory=path.dirname(fileURLToPath(import.meta.url)),repository=path.resolve(directory,'../..');
const hash=b=>createHash('sha256').update(b).digest('hex');
const bytes=f=>fs.existsSync(path.join(directory,f))?fs.readFileSync(path.join(directory,f)):gunzipSync(fs.readFileSync(path.join(directory,f+'.gz')));
const read=f=>JSON.parse(bytes(f)),p=read('provenance.json');
assert.equal(p.schema,1);assert.equal(p.measured_revision,'30d6c412277cb7c417277eeff693dfb17ed2c50d');assert.equal(p.baseline_revision,'858afcce22c9ec50bb94f7630fad0f6f298ce821');
for(const [file,expected]of Object.entries(p.files))assert.equal(hash(fs.readFileSync(path.join(directory,file))),expected,file);
for(const e of p.compressed_files){const packed=fs.readFileSync(path.join(directory,e.path));assert.equal(hash(packed),e.compressed_sha256);const raw=gunzipSync(packed);assert.equal(hash(raw),e.uncompressed_sha256);assert.equal(raw.length,e.uncompressed_bytes);}
for(const [revision,inputs]of Object.entries(p.source_inputs))for(const [file,expected]of Object.entries(inputs))assert.equal(hash(execFileSync('git',['show',`${revision}:${file}`],{cwd:repository})),expected,`${revision}:${file}`);
// The initial candidate remains a separate qualification with all unfavorable results.
execFileSync('node',[path.join(repository,'benchmarks/test262-async/verify.mjs')],{cwd:repository,stdio:'pipe',maxBuffer:8*1024*1024});
const temporary=fs.mkdtempSync(path.join(os.tmpdir(),'temiya-async-signals-'));
process.on('exit',()=>fs.rmSync(temporary,{recursive:true,force:true}));
const unpack=(source,target)=>{fs.mkdirSync(target,{recursive:true});for(const e of fs.readdirSync(source,{withFileTypes:true})){const src=path.join(source,e.name);if(e.isDirectory())unpack(src,path.join(target,e.name));else{const compressed=e.name.endsWith('.gz');fs.writeFileSync(path.join(target,compressed?e.name.slice(0,-3):e.name),compressed?gunzipSync(fs.readFileSync(src)):fs.readFileSync(src));}}};
for(const folder of ['arm64','hosted'])unpack(path.join(directory,folder),path.join(temporary,folder));
const expected=[];for(const folder of ['arm64','hosted/copy1','hosted/copy2'])for(const name of ['beforeafter','boa','baseline-boa','same-binary-control'])expected.push(`${folder}/${name}.json`);
assert.deepEqual([...p.native_reports].sort(),expected.sort());
const audit=JSON.parse(execFileSync('node',[path.join(repository,'scripts/audit-original-workload-records.mjs'),...p.native_reports.map(f=>path.join(temporary,f))],{cwd:repository,encoding:'utf8',maxBuffer:8*1024*1024}));
assert.deepEqual(Object.fromEntries(audit.reports.map(r=>[path.relative(temporary,r.file),r.outcomes])),p.native_outcomes);
for(const folder of ['hosted/copy1','hosted/copy2'])execFileSync('node',[path.join(repository,'scripts/audit-omoikane-hosted-identities.mjs'),path.join(temporary,folder),p.measured_revision,p.baseline_revision],{cwd:repository,stdio:'pipe'});
const arm=f=>read('arm64/'+f+'.json'),candidate=arm('current-info'),baseline=arm('baseline-info'),boa=arm('boa-build-info');
assert.equal(candidate.revision,p.measured_revision);assert.equal(baseline.revision,p.baseline_revision);
assert.equal(candidate.executable_sha256,'889f4545b3b2a67dfccf1f1d7081d57f807691b5ae55676dd5e274f26b8f7352');assert.equal(baseline.executable_sha256,'5b4b309e60652e2338b6611c6a0f78f407a38718839649a832f1908758ca5ca9');
assert.equal(boa.omoikane_revision,'007556adccb8fb4329236932ca8175f8e8d119d2');assert.equal(boa.checkout_clean,true);assert.equal(boa.profile,'release');assert.equal(boa.rustflags,'');assert.equal(boa.execution_mode,'interpreter; baseline-jit feature absent');assert.deepEqual(boa.cargo_args,['build','--locked','--release']);
assert.equal(boa.worker_source_sha256,hash(execFileSync('git',['show',p.measured_revision+':benchmarks/omoikane-worker/main.rs'],{cwd:repository})));
assert.deepEqual(boa.features,{omoikane:[],boa_engine:['annex-b','default','experimental','float16','intl','intl_bundled','temporal','xsum'],regress:['backend-pikevm','default','std','utf16']});
assert.equal(boa.original_lock_sha256,'f3988c4d9b0ecc28560df18df1cae7760c7c55a016318ea3e31f3ebbfcaea211');assert.equal(boa.adapter_lock_sha256,'6b4debb1e4368b52de4586b7b30c3d4b6b8882a1bd4ee87d78aa488a252517e7');
assert.equal(boa.rustc,candidate.rustc);assert.equal(baseline.rustc,candidate.rustc);
for(const [name,workers]of Object.entries({beforeafter:[candidate,baseline],boa:[candidate,boa],'baseline-boa':[baseline,boa],'same-binary-control':[candidate,candidate]})){const r=arm(name);assert.equal(r.rustc,candidate.rustc);assert.deepEqual(r.machine,arm('boa').machine);for(let i=0;i<2;i++){assert.equal(r.workers[i].executable_sha256??r.workers[i].build?.executable_sha256,workers[i].executable_sha256);assert.equal(r.workers[i].revision,workers[i].revision??workers[i].omoikane_revision);}}
const ci=read('validation/ci-status.json');assert.equal(ci.conclusion,'success');assert.equal(ci.headSha,p.measured_revision);assert.equal(ci.jobs.length,4);assert(ci.jobs.every(j=>j.conclusion==='success'));
const stable=bytes('validation/ci.log').toString().split('\n').filter(l=>l.includes('Rust stable')).map(l=>l.match(/([a-f0-9]{64})\s+target\/release\/examples\/measure_workload/)).filter(Boolean);assert.equal(stable.length,1);for(const folder of ['hosted/copy1','hosted/copy2'])assert.equal(read(folder+'/boa.json').workers[0].executable_sha256,stable[0][1]);
const qualified=expected.filter(f=>f.endsWith('/boa.json')&&read(f).all_eleven_workloads_faster_beyond_threshold===true);assert.deepEqual(p.qualified_boa_reports,qualified);
console.log(JSON.stringify({verified_files:Object.keys(p.files).length,native_reports:audit.reports.length,native_outcomes:p.native_outcomes,qualified_boa_reports:qualified,scope:p.qualification},null,2));
