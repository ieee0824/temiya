// Recompute measurements; successful auditing does not imply a speed pass.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {gunzipSync} from 'node:zlib';
import {fileURLToPath} from 'node:url';
const directory=path.dirname(fileURLToPath(import.meta.url));
const repository=path.resolve(directory,'../..');
const hash=b=>createHash('sha256').update(b).digest('hex');
const bytes=f=>fs.existsSync(path.join(directory,f))?fs.readFileSync(path.join(directory,f)):gunzipSync(fs.readFileSync(path.join(directory,f+'.gz')));
const read=f=>JSON.parse(bytes(f));
const evidence=read('evidence.json');
assert.equal(evidence.schema,1);
for(const entry of evidence.files){const raw=fs.readFileSync(path.join(directory,entry.path));assert.equal(hash(raw),entry.sha256,entry.path);assert.equal(hash(gunzipSync(raw)),entry.uncompressed_sha256,entry.path);}
const log=name=>gunzipSync(fs.readFileSync(path.join(directory,'evidence',name+'.gz'))).toString();
const native=[...log('native-final.log').matchAll(/test result: ok\. (\d+) passed; 0 failed; (\d+) ignored/g)];
assert.equal(native.length,91);assert.equal(native.reduce((n,r)=>n+Number(r[1]),0),695);assert.equal(native.reduce((n,r)=>n+Number(r[2]),0),25);
for(const [name,count]of Object.entries({'promises-miri.log':6,'shared-miri.log':3,'queue-lib-miri.log':5,'promise-lib-miri.log':7}))assert(log(name).includes('test result: ok. '+count+' passed; 0 failed; 0 ignored;'),name);
for(const backend of ['ast','auto']){const r=JSON.parse(log('original-initial-'+backend+'.json'));assert.equal(r.corpus_revision,evidence.corpus);assert.deepEqual(r.counts,{fail:4,pass:4,skip:0});assert.equal(r.results.filter(x=>x.status==='pass').length,4);assert.equal(r.results.filter(x=>x.status==='fail').length,4);}
const selected=log('selected-original-tests.log'),original=[];
for(let start=0;(start=selected.indexOf('{\n  "backend":',start))>=0;){let depth=0,quoted=false,escaped=false,end=start;for(;end<selected.length;end++){const c=selected[end];if(quoted){if(escaped)escaped=false;else if(c==='\\')escaped=true;else if(c==='"')quoted=false;}else if(c==='"')quoted=true;else if(c==='{')depth++;else if(c==='}'&&--depth===0){end++;break;}}original.push(JSON.parse(selected.slice(start,end)));start=end;}
assert.deepEqual(original.map(r=>[r.backend,r.counts.pass]),[['Ast',4],['Auto',4],['Auto',13],['Ast',34],['Auto',34],['Ast',60],['Auto',60]]);
for(const r of original){assert.equal(r.corpus_revision,evidence.corpus);assert.equal(r.counts.fail,0);assert.equal(r.counts.skip,0);assert.equal(r.results.length,r.counts.pass);assert(r.results.every(x=>x.status==='pass'));}
assert.equal(original.reduce((n,r)=>n+r.counts.pass,0),209);
const p=read('provenance.json');
assert.equal(p.schema,1);
for(const e of p.compressed_files){const packed=fs.readFileSync(path.join(directory,e.path));assert.equal(hash(packed),e.compressed_sha256);const raw=gunzipSync(packed);assert.equal(hash(raw),e.uncompressed_sha256);assert.equal(raw.length,e.uncompressed_bytes);}
assert.equal(p.measured_revision,'abbeac6b36767c0df6565bcce7e8b066bdaa0c06');
assert.equal(p.baseline_revision,'858afcce22c9ec50bb94f7630fad0f6f298ce821');
for(const [file,expected]of Object.entries(p.files))assert.equal(hash(fs.readFileSync(path.join(directory,file))),expected,file);
for(const [revision,inputs]of Object.entries(p.source_inputs))for(const [file,expected]of Object.entries(inputs))assert.equal(hash(execFileSync('git',['show',`${revision}:${file}`],{cwd:repository})),expected,`${revision}:${file}`);
const temporary=fs.mkdtempSync(path.join(os.tmpdir(),'temiya-async-evidence-'));
process.on('exit',()=>fs.rmSync(temporary,{recursive:true,force:true}));
const unpack=(source,target)=>{fs.mkdirSync(target,{recursive:true});for(const e of fs.readdirSync(source,{withFileTypes:true})){const src=path.join(source,e.name);if(e.isDirectory())unpack(src,path.join(target,e.name));else{const compressed=e.name.endsWith('.gz');fs.writeFileSync(path.join(target,compressed?e.name.slice(0,-3):e.name),compressed?gunzipSync(fs.readFileSync(src)):fs.readFileSync(src));}}};
for(const folder of ['arm64','hosted'])unpack(path.join(directory,folder),path.join(temporary,folder));
const expected=[];
for(const folder of ['arm64','hosted/copy1','hosted/copy2'])for(const name of ['beforeafter','boa','baseline-boa','same-binary-control'])expected.push(`${folder}/${name}.json`);
expected.push('arm64/beforeafter-recheck.json');
assert.deepEqual([...p.native_reports].sort(),expected.sort());
const audit=JSON.parse(execFileSync('node',[path.join(repository,'scripts/audit-original-workload-records.mjs'),...p.native_reports.map(f=>path.join(temporary,f))],{cwd:repository,encoding:'utf8',maxBuffer:8*1024*1024}));
assert.deepEqual(Object.fromEntries(audit.reports.map(r=>[path.relative(temporary,r.file),r.outcomes])),p.native_outcomes);
for(const folder of ['hosted/copy1','hosted/copy2'])execFileSync('node',[path.join(repository,'scripts/audit-omoikane-hosted-identities.mjs'),path.join(temporary,folder),p.measured_revision,p.baseline_revision],{cwd:repository,stdio:'pipe'});
const armRead=f=>JSON.parse(fs.readFileSync(path.join(temporary,'arm64',f+'.json')));
const candidate=armRead('current-info'),baseline=armRead('baseline-info'),boa=armRead('boa-build-info');
assert.equal(candidate.revision,p.measured_revision);assert.equal(baseline.revision,p.baseline_revision);
assert.equal(candidate.executable_sha256,'3d2ed08ec74ca46540b554ada4764f65f06ba99f26c64f8f05cf0a1c05fef2bb');
assert.equal(baseline.executable_sha256,'5b4b309e60652e2338b6611c6a0f78f407a38718839649a832f1908758ca5ca9');
assert.equal(boa.omoikane_revision,'007556adccb8fb4329236932ca8175f8e8d119d2');
assert.equal(boa.checkout_clean,true);assert.equal(boa.profile,'release');assert.equal(boa.execution_mode,'interpreter; baseline-jit feature absent');
// This older ARM build-info schema predates workload_kind; the pinned sync
// adapter source hash establishes the worker identity directly.
assert.equal(boa.worker_source_sha256,hash(execFileSync('git',['show',p.measured_revision+':benchmarks/omoikane-worker/main.rs'],{cwd:repository})));
assert.equal(boa.rustflags,'');
assert.deepEqual(boa.cargo_args,['build','--locked','--release']);
assert.deepEqual(boa.features,{omoikane:[],boa_engine:['annex-b','default','experimental','float16','intl','intl_bundled','temporal','xsum'],regress:['backend-pikevm','default','std','utf16']});
assert.equal(boa.original_lock_sha256,'f3988c4d9b0ecc28560df18df1cae7760c7c55a016318ea3e31f3ebbfcaea211');assert.equal(boa.adapter_lock_sha256,'6b4debb1e4368b52de4586b7b30c3d4b6b8882a1bd4ee87d78aa488a252517e7');
assert.equal(boa.rustc,candidate.rustc);assert.equal(baseline.rustc,candidate.rustc);
for(const [name,workers]of Object.entries({beforeafter:[candidate,baseline],boa:[candidate,boa],'baseline-boa':[baseline,boa],'same-binary-control':[candidate,candidate]})){
 const r=armRead(name);assert.equal(r.rustc,candidate.rustc);assert.deepEqual(r.machine,armRead('boa').machine);
 for(let i=0;i<2;i++){assert.equal(r.workers[i].executable_sha256??r.workers[i].build?.executable_sha256,workers[i].executable_sha256);assert.equal(r.workers[i].revision,workers[i].revision??workers[i].omoikane_revision);}
}
const profile=read('profile/metadata.json');
assert.deepEqual(profile.diagnostic_workloads,['proto-method','call']);assert.equal(profile.runs.length,4);
assert.equal(profile.rustc,read('hosted/copy1/boa.json').rustc);assert.deepEqual(profile.original_manifest,read('hosted/copy1/manifest.json'));
for(const build of profile.builds){assert.equal(build.revision,build.label==='measured'?p.measured_revision:p.baseline_revision);assert.deepEqual(build.features,[]);assert.equal(build.profile,'release');for(const folder of ['hosted/copy1','hosted/copy2'])assert.equal(build.executable_sha256,read(folder+'/beforeafter.json').workers[build.label==='measured'?0:1].executable_sha256);}
for(const name of profile.diagnostic_workloads){const input=read('profile/inputs/'+name+'.json'),manifestEntry=profile.original_manifest.workloads.find(w=>w.name===name);assert.equal(input.iterations,manifestEntry.iterations);assert.equal(input.expected,manifestEntry.expected);assert.equal(hash(input.setup),manifestEntry.setup_sha256);assert.equal(hash(input.invocation),manifestEntry.invocation_sha256);const runs=profile.runs.filter(r=>r.name===name);assert.equal(runs.length,2);for(const run of runs){assert.equal(run.exit_status,0);assert.equal(run.signal,null);assert.equal(run.error,null);assert.equal(run.annotation_exit,0);assert.equal(run.validated_passes,2);assert.equal(run.input_sha256,hash(bytes('profile/inputs/'+name+'.json')));const raw=bytes('profile/'+run.label+'/'+name+'.callgrind').toString(),events=raw.match(/^events: (.+)$/m)[1].trim().split(/\s+/),values=raw.match(/^summary: (.+)$/m)[1].trim().split(/\s+/).map(Number);assert.deepEqual(Object.fromEntries(events.map((k,i)=>[k,values[i]])),run.counters);const out=read('profile/'+run.label+'/'+name+'.stdout');assert.equal(out.expected,input.expected);assert.equal(out.mode,'Auto Script');assert.equal(out.validated_passes,2);assert.equal(out.samples_ns.length,1);}}
const ci=read('validation/ci-status.json');assert.equal(ci.conclusion,'success');assert.equal(ci.headSha,p.measured_revision);assert.equal(ci.jobs.length,4);assert(ci.jobs.every(j=>j.conclusion==='success'));
const stable=bytes('validation/ci.log').toString().split('\n').filter(l=>l.includes('Rust stable')).map(l=>l.match(/([a-f0-9]{64})\s+target\/release\/examples\/measure_workload/)).filter(Boolean);assert.equal(stable.length,1);for(const folder of ['hosted/copy1','hosted/copy2'])assert.equal(read(folder+'/boa.json').workers[0].executable_sha256,stable[0][1]);
for(const file of p.qualified_boa_reports){assert(p.native_reports.includes(file));const r=JSON.parse(fs.readFileSync(path.join(temporary,file)));assert.equal(r.comparison_kind,'Temiya versus omoikane Boa');assert.equal(r.all_eleven_workloads_faster_beyond_threshold,true);assert(r.comparisons.every(c=>c.outcome==='Temiya faster beyond threshold'));}
console.log(JSON.stringify({verified_files:Object.keys(p.files).length,native_reports:audit.reports.length,native_outcomes:p.native_outcomes,qualified_boa_reports:p.qualified_boa_reports,scope:p.qualification},null,2));
