import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import z from 'node:zlib';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../../..');
const read=f=>JSON.parse(fs.readFileSync(root+'/'+f)),text=f=>z.gunzipSync(fs.readFileSync(root+'/'+f)).toString();
const hash=b=>createHash('sha256').update(b).digest('hex'),git=args=>execFileSync('git',args,{cwd:repo});
const e=read('evidence.json');
assert.equal(e.source,'13524c4654826fbd6a4ae62dfa55f78101b963fb');
assert.equal(e.predecessor,'a5d583eec34d3054554fb6812b5fe9d9dd57e0cb');
assert.equal(e.qualified,false);
for(const [file,sha]of Object.entries(e.files)){assert.equal(path.basename(file),file);assert.equal(hash(fs.readFileSync(root+'/'+file)),sha);}
const outcomes=r=>r.comparisons.reduce((a,c)=>(a[c.outcome]=(a[c.outcome]??0)+1,a),{});
for(const [name,key]of [['boa','boaOutcomes'],['beforeafter','beforeAfterOutcomes'],['same-binary','sameBinaryOutcomes']]){
 const audit=JSON.parse(execFileSync(process.execPath,[repo+'/scripts/audit-class-workload-records.mjs','--parameters',root+'/'+name+'.json'],{cwd:repo,encoding:'utf8'}));
 const omit=a=>a.reports.map(({file,...r})=>r);
 assert.equal(audit.integrity,'pass');assert.deepEqual(omit(audit),omit(read(name+'-audit.json')));
 const r=read(name+'.json');assert.equal(r.machine.architecture,'arm64');assert.equal(r.samples,15);assert.equal(r.pairs,3);assert.equal(r.workers[0].revision,e.source);assert.deepEqual(outcomes(r),e[key]);
}
assert.equal(read('boa.json').all_class_workloads_faster_beyond_threshold,false);
assert.equal(read('beforeafter.json').workers[1].revision,e.predecessor);
const same=read('same-binary.json');assert(same.workers.every(w=>w.revision===e.source));assert.equal(same.workers[0].executable_sha256,same.workers[1].executable_sha256);
const info=JSON.parse(text('worker-info.json.gz'));
assert.equal(info.source_revision,e.source);assert.equal(info.source_clean,true);assert.equal(info.source_inputs.length,86);assert.equal(info.executable_sha256,read('boa.json').workers[0].executable_sha256);
for(const [entries,paths]of [[info.source_inputs,['src','examples/measure_workload.rs','Cargo.toml','Cargo.lock','build.rs']],[e.testInputs,['tests']]]){
 assert.deepEqual(entries.map(f=>f.path),git(['ls-tree','-r','--name-only',e.source,'--',...paths]).toString().trim().split('\n'));
 for(const f of entries)assert.equal(hash(git(['show',e.source+':'+f.path])),f.sha256);
}
const native=text('native.log.gz'),counts=[...native.matchAll(/test result: ok\. (\d+) passed; 0 failed; (\d+) ignored;/g)];
assert(!native.includes('test result: FAILED'));assert.equal(counts.reduce((n,m)=>n+Number(m[1]),0),1059);assert.equal(counts.reduce((n,m)=>n+Number(m[2]),0),26);
assert(native.includes('eligibility_checks_all_fragments_before_any_execution ... ok'));
assert(text('clippy.log.gz').includes('Finished `dev` profile'));assert(text('release.log.gz').includes('Finished `release` profile'));
const miri=text('miri.log.gz');for(const name of ['outer_default_arrows_keep_fresh_names_live_bindings_and_dynamic_lookup','parameter_and_lexical_context_arrows_keep_the_full_header_path','prepared_named_arrows_preserve_function_descriptors_and_nonconstructability'])assert(miri.includes('test '+name+' ... ok'));
assert(miri.includes('test result: ok. 3 passed; 0 failed; 0 ignored;'));
for(const [file,name]of [['gc-miri.log.gz','outer_default_arrow_traces_enclosing_cells_through_allocation_and_realm_release'],['rest-gc-miri.log.gz','real_rest_array_traces_arguments_during_allocation_and_after_return'],['cache-miri.log.gz','child_code_cache_survives_script_release_without_owning_its_parent'],['suspended-miri.log.gz','escaping_defaults_and_suspended_generator_keep_cells_across_gc'],['async-miri.log.gz','generator_and_async_bodies_keep_defaults_outside_body_vars']]){const log=text(file);assert(log.includes(name+' ... ok'));assert(log.includes('test result: ok. 1 passed; 0 failed; 0 ignored;'));}
assert(text('cache-native.log.gz').includes('child_code_cache_survives_script_release_without_owning_its_parent ... ok'));
assert(text('nested-before.callgrind.gz').includes('summary: 591799727'));assert(text('nested-after.callgrind.gz').includes('summary: 488304350'));for(const file of ['nested-before.stdout.gz','nested-after.stdout.gz']){const value=JSON.parse(text(file));assert.equal(value.expected,50005000);assert.equal(value.validated_passes,2);}
assert(text('before.callgrind.gz').includes('summary: 457084275'));assert(text('after.callgrind.gz').includes('summary: 340644997'));
const output=JSON.parse(text('after.stdout.gz'));assert.equal(output.expected,50005000);assert.equal(output.validated_passes,2);
console.log(JSON.stringify({integrity:'pass',source:e.source,nativeTests:1059,ignored:26,targetedPublicMiri:3,compilerInputs:86,boaOutcomes:e.boaOutcomes,beforeAfterOutcomes:e.beforeAfterOutcomes,sameBinaryOutcomes:e.sameBinaryOutcomes,qualified:false,scope:e.scope}));
