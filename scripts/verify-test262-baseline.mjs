// Independent reconstruction from preserved CLI reports, not declared summaries.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
const root=path.resolve(process.argv[2]??'');
if(process.argv.length!==3)throw Error('usage: node scripts/verify-test262-baseline.mjs BASELINE_DIR');
const file=name=>{assert.equal(typeof name,'string');assert(!path.isAbsolute(name)&&!name.split(/[\\/]/).some(p=>p==='..'||p==='.'||!p),'noncanonical artifact path');return path.join(root,name);};
const bytes=name=>fs.readFileSync(file(name));
const read=name=>JSON.parse(bytes(name));
const hash=b=>createHash('sha256').update(b).digest('hex');
const baseline=read('baseline.json'),conditions=read('conditions.json'),universe=read('universe.json'),selection=read('selection.json');
assert.equal(baseline.schema,1);assert.equal(baseline.kind,'test262-feature-baseline');assert.deepEqual(baseline.conditions,conditions);
assert.equal(hash(bytes('manifest.json')),conditions.manifest_sha256);
assert.equal(read('manifest.json').revision,conditions.manifest_revision);
for(const [list,digest]of [[universe,conditions.universe_sha256],[selection,conditions.selection_sha256]]){
 assert(Array.isArray(list));assert.deepEqual(list,[...new Set(list)].sort());assert.equal(hash(JSON.stringify(list)),digest);
 for(const p of list)assert(typeof p==='string'&&!path.posix.isAbsolute(p)&&!p.includes('\\')&&!p.split('/').some(s=>!s||s==='.'||s==='..'));
}
const selected=new Set(selection),universeSet=new Set(universe);assert(selection.every(p=>universeSet.has(p)));
assert.equal(conditions.engine_frozen_during_run,true);assert.match(conditions.engine_sha256,/^[a-f0-9]{64}$/);
if(conditions.provided_engine_build_info)assert.equal(conditions.provided_engine_build_info.executable_sha256,conditions.engine_sha256);
const catalog=read(baseline.catalog.file),outcomes=read(baseline.outcomes.file);
assert.equal(hash(bytes(baseline.catalog.file)),baseline.catalog.sha256);
assert.equal(hash(bytes(baseline.outcomes.file)),baseline.outcomes.sha256);
const indexed=new Map(),rawRows=new Map(),rawSetups=new Map(),metadataBatches=[],executionBatches=[];
const key=(p,v)=>JSON.stringify([p,v]);
const badRaw=new Set();
let recovery=null,recoverySource=null,recoveryUnknown=new Set(),recoveryChecked=false;
if(baseline.recovery){
 const descriptor=baseline.recovery;
 for(const row of [descriptor.source_baseline,descriptor.source_outcomes,descriptor.attempt])assert.equal(hash(bytes(row.file)),row.sha256);
 recoverySource=read(descriptor.source_baseline.file);recovery=read(descriptor.attempt.file);
 assert.equal(recovery.schema,1);assert.equal(recovery.kind,'test262-execution-recovery');
 assert(!recoverySource.recovery,'nested recovery requires separate review');
 assert.deepEqual(recoverySource.conditions,conditions);assert.deepEqual(recoverySource.universe,baseline.universe);
 assert.deepEqual(recoverySource.catalog,baseline.catalog);assert.deepEqual(recoverySource.file_setup_failures,[]);
 assert.equal(recoverySource.totals.not_run,0);assert(recoverySource.totals.unreported>0);
 assert.equal(hash(bytes(descriptor.source_outcomes.file)),recoverySource.outcomes.sha256);
 assert(recoverySource.capture_errors.length>0&&recoverySource.capture_errors.every(error=>error.stage==='execution'));
 assert.deepEqual(baseline.recovered_capture_errors,recoverySource.capture_errors);
 assert.deepEqual(recovery.capture_errors,[]);
 assert.deepEqual(read('recovery/conditions.json'),recovery.conditions);
 assert.equal(hash(bytes('recovery/collector.mjs')),recovery.conditions.collector_sha256);
 for(const name of ['backend_requested','module_execution_path','timeout_ms','memory_mb','stack_kb','engine_sha256',
  'provided_engine_build_info','engine_frozen_during_run','corpus_revision','manifest_revision','manifest_sha256','host'])
  assert.deepEqual(recovery.conditions[name],conditions[name],'changed recovery condition: '+name);
 assert.equal(recovery.conditions.batch_size,1);
 assert.deepEqual(baseline.raw_reports.slice(0,recoverySource.raw_reports.length),recoverySource.raw_reports);
 assert.deepEqual(baseline.raw_reports.slice(recoverySource.raw_reports.length),recovery.raw_reports);
}
function checkRecoveryBoundary(){
 assert(recovery&&catalog.every(test=>test.parsed===true));
 assert.deepEqual([...selected].sort(),universe);
 const originalRows=[];
 for(const test of catalog)for(const variant of test.variants){
  const observed=rawRows.get(key(test.path,variant));
  originalRows.push({path:test.path,variant,outcome:observed?.outcome??'unreported',...(observed?{raw_record:observed.raw_record}:{})});
 }
 assert.deepEqual(read(baseline.recovery.source_outcomes.file),originalRows,'original known results changed');
 const missing=originalRows.filter(row=>row.outcome==='unreported');
 assert.equal(missing.length,recoverySource.totals.unreported);
 assert.deepEqual(recovery.expected_missing,missing.map(({path,variant})=>({path,variant})));
 recoveryUnknown=new Set(missing.map(row=>key(row.path,row.variant)));
 const paths=[...new Set(missing.map(row=>row.path))].sort();
 assert.deepEqual(recovery.selected_paths,paths);
 assert.deepEqual([...new Set(recoverySource.capture_errors.flatMap(error=>error.paths))].sort(),paths);
 assert.deepEqual([...badRaw].sort(),recoverySource.capture_errors.map(error=>error.raw_report+'.json').sort());
 assert.deepEqual(recovery.raw_reports.map(raw=>raw.paths[0]),paths);
 const oldCount={total:originalRows.length,pass:0,fail:0,skip:0,timeout:0,not_run:0,unreported:0};
 for(const row of originalRows)oldCount[row.outcome]++;
 const attempts=oldCount.pass+oldCount.fail+oldCount.timeout;
 assert.deepEqual(recoverySource.totals,{...oldCount,reported_attempts:attempts,pass_rate_reported_attempts:attempts?oldCount.pass/attempts:null});
 for(const raw of recovery.raw_reports){
  assert.equal(raw.recovery,true);assert.equal(raw.stage,'execution');assert.equal(raw.paths.length,1);
  const test=indexed.get(raw.paths[0]);assert(test);
  for(const variant of test.variants)assert(recoveryUnknown.has(key(test.path,variant)),'cannot retry an observed variant');
  assert.equal(raw.collection_timeout_ms,Math.max(60000,test.variants.length*conditions.timeout_ms+30000));
 }
 recoveryChecked=true;
}
for(const [rawIndex,raw]of baseline.raw_reports.entries()){
 if(recovery&&rawIndex===recoverySource.raw_reports.length)checkRecoveryBoundary();
 assert.equal(hash(bytes(raw.stdout)),raw.stdout_sha256);assert.equal(hash(bytes(raw.stderr)),raw.stderr_sha256);
 if(raw.error||raw.signal||![0,1,3].includes(raw.exit_code)){assert(!raw.recovery,'failed recovery report');badRaw.add(raw.stdout);continue;}
 let report;try{report=read(raw.stdout);}catch{assert(!raw.recovery,'malformed recovery report');badRaw.add(raw.stdout);continue;}
 assert.equal(report.schema,1);assert.equal(report.corpus_revision,conditions.corpus_revision);assert.equal(report.manifest_revision,conditions.manifest_revision);
 if(raw.stage==='metadata'){
  assert.equal(report.kind,'test262-metadata');assert.equal(report.engine_executed,false);
  assert.equal(report.cases.length,raw.paths.length);assert.deepEqual(report.cases.map(c=>c.path).sort(),[...raw.paths].sort());
  for(const c of report.cases){assert.equal(c.engine_executed,false);assert(!indexed.has(c.path));indexed.set(c.path,c);}
  assert.equal(raw.exit_code,report.cases.some(c=>c.parsed!==true)?1:0);metadataBatches.push(...raw.paths);
 }else{
  assert.equal(raw.stage,'execution');assert.equal(report.backend,({auto:'Auto',ast:'Ast',bytecode:'Bytecode'})[conditions.backend_requested]);
  const count={pass:0,fail:0,skip:0};
  for(const [i,r]of report.results.entries()){
   assert(raw.paths.includes(r.path));assert(Object.hasOwn(count,r.status));count[r.status]++;
   assert(r.timed_out!==true||r.status==='fail');
   if(raw.recovery){
    assert(recoveryChecked&&recoveryUnknown.has(key(r.path,r.variant)),'recovery replaced a reported outcome');
    if(r.status!=='skip'){
     assert.equal(r.resources.timeout_ms,conditions.timeout_ms);
     assert.equal(r.resources.memory_limit_bytes,conditions.memory_mb*1024*1024);
     assert.equal(r.resources.stack_limit_bytes,conditions.stack_kb*1024);
     if(conditions.host.platform==='linux')assert.equal(r.resources.applied,true);
    }
   }
   const pointer=raw.stdout+'#/results/'+i;
   if(r.variant===null&&r.stage==='setup'&&r.status==='fail'){
    assert(!rawSetups.has(r.path));rawSetups.set(r.path,{path:r.path,detail:r.detail,raw_record:pointer});
   }else{const id=key(r.path,r.variant);assert(!rawRows.has(id));rawRows.set(id,{outcome:r.status==='fail'&&r.timed_out===true?'timeout':r.status,raw_record:pointer});}
  }
  assert.deepEqual(count,report.counts);assert.equal(raw.exit_code,count.fail?1:count.pass?0:3);executionBatches.push(...raw.paths);
 }
}
if(recovery){assert(recoveryChecked);assert.equal(badRaw.size,recoverySource.capture_errors.length);}
else assert.equal(badRaw.size,0,'untracked failed reports cannot qualify a complete capture');
// A complete validated capture cannot rely on failed report collection.
assert.deepEqual(baseline.capture_errors,[]);assert.deepEqual(metadataBatches.sort(),universe);assert.deepEqual(executionBatches.sort(),selection);
assert.deepEqual(catalog,universe.map(p=>indexed.get(p)));assert.equal(indexed.size,universe.length);
const counts=()=>({total:0,pass:0,fail:0,skip:0,timeout:0,not_run:0,unreported:0});
const totals=counts(),untagged={feature:null,files:0,selected_files:0,setup_failure_files:0,...counts()},features=new Map(),expectedRows=[];
const groups=c=>c.features.length?[...new Set(c.features)].map(f=>{if(!features.has(f))features.set(f,{feature:f,files:0,selected_files:0,setup_failure_files:0,...counts()});return features.get(f);}):[untagged];
for(const c of catalog){
 if(c.parsed!==true){assert(c.variants===undefined);continue;}
 assert(Array.isArray(c.features)&&c.features.every(f=>typeof f==='string'));assert(c.variants.length>=1&&c.variants.length<=2);assert.equal(new Set(c.variants).size,c.variants.length);
 const associated=groups(c);
 for(const g of associated){g.files++;g.selected_files+=Number(selected.has(c.path));g.setup_failure_files+=Number(rawSetups.has(c.path));}
 for(const variant of c.variants){
  assert(['Sloppy','Strict','Module'].includes(variant));const observed=rawRows.get(key(c.path,variant));
  if(!selected.has(c.path))assert(!observed);
  const row={path:c.path,variant,outcome:observed?.outcome??(selected.has(c.path)?'unreported':'not_run')};
  if(observed)row.raw_record=observed.raw_record;expectedRows.push(row);
  for(const g of [totals,...associated]){g.total++;g[row.outcome]++;}
 }
}
assert.equal(rawRows.size,expectedRows.filter(r=>r.raw_record).length);assert.deepEqual(outcomes,expectedRows);
const summary=g=>({...g,reported_attempts:g.pass+g.fail+g.timeout,pass_rate_reported_attempts:g.pass+g.fail+g.timeout?g.pass/(g.pass+g.fail+g.timeout):null});
assert.deepEqual(baseline.totals,summary(totals));assert.deepEqual(baseline.untagged,summary(untagged));
assert.deepEqual(baseline.features,[...features.values()].sort((a,b)=>a.feature<b.feature?-1:a.feature>b.feature?1:0).map(summary));
assert.deepEqual(baseline.file_setup_failures,[...rawSetups.values()]);
assert.equal(baseline.universe.files,universe.length);assert.equal(baseline.universe.selected_files,selection.length);
assert.equal(baseline.universe.metadata_error_files,catalog.filter(c=>c.parsed!==true).length);assert.equal(baseline.universe.variant_count_exact,catalog.every(c=>c.parsed===true));
assert.equal(totals.unreported,0,'incomplete variant results');
console.log(JSON.stringify({integrity:'pass',files:universe.length,variants:totals.total,features:features.size,totals:summary(totals),corpus_revision:conditions.corpus_revision,...(recovery?{recovered_collection_errors:recoverySource.capture_errors.length}:{}),qualification:'Recomputed report integrity and recorded outcomes; not full language feature conformance.'}));
