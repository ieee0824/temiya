import fs from 'node:fs';import assert from 'node:assert/strict';import path from 'node:path';import z from 'node:zlib';import {createHash} from 'node:crypto';import {execFileSync} from 'node:child_process';import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../../..'),read=f=>JSON.parse(fs.readFileSync(root+'/'+f)),hash=b=>createHash('sha256').update(b).digest('hex'),e=read('evidence.json');
assert.equal(e.source,'88dd92e64fb24adaded1aee52ced4b4f8bbfa88a');
for(const [f,sha]of Object.entries(e.files)){assert.equal(path.basename(f),f);assert.equal(hash(fs.readFileSync(root+'/'+f)),sha);}
for(const [file,saved]of [['comparison.json','audit.json'],['same-binary.json','same-binary-audit.json']]){
 const result=JSON.parse(execFileSync(process.execPath,[repo+'/scripts/audit-class-workload-records.mjs','--parameters',root+'/'+file],{cwd:repo,encoding:'utf8'}));
 const previous=read(saved);assert.equal(result.integrity,'pass');assert.equal(previous.integrity,'pass');
 const omitFile=r=>r.reports.map(({file,...rest})=>rest);assert.deepEqual(omitFile(result),omitFile(previous));
 const r=read(file);assert.equal(r.machine.architecture,'arm64');assert.equal(r.runs.length,48);assert.equal(r.comparisons.length,24);assert(r.workers.every(w=>w.revision===e.source||w.build));
}
const report=read('comparison.json');assert.equal(report.all_class_workloads_faster_beyond_threshold,false);assert.equal(report.comparisons.filter(c=>c.outcome==='Temiya slower beyond threshold').length,20);assert.equal(report.comparisons.filter(c=>c.outcome==='within noise threshold').length,4);
const info=JSON.parse(z.gunzipSync(fs.readFileSync(root+'/worker-info.json.gz')));assert.equal(info.source_revision,e.source);assert.equal(info.source_inputs.length,84);for(const f of info.source_inputs)assert.equal(hash(execFileSync('git',['show',e.source+':'+f.path],{cwd:repo})),f.sha256);
const profile=JSON.parse(z.gunzipSync(fs.readFileSync(root+'/var-copy.stdout.gz')));assert.equal(profile.expected,50005000);assert.equal(profile.validated_passes,2);assert.equal(profile.profile,'release');assert.equal(profile.mode,'Auto Script');assert(z.gunzipSync(fs.readFileSync(root+'/var-copy.callgrind.gz')).toString().includes('summary: 246701156'));
assert.equal(e.nayuta.release,'v0.3.3');assert(fs.readFileSync(root+'/nayuta.stderr','utf8').includes('scanned 3 Rust file(s), found 41 issue(s)'));
console.log(JSON.stringify({integrity:'pass',source:e.source,slowerPairs:20,noisePairs:4,nativeQualification:false,scope:e.scope}));
