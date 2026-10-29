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
for(const raw of baseline.raw_reports){
 assert.equal(hash(bytes(raw.stdout)),raw.stdout_sha256);assert.equal(hash(bytes(raw.stderr)),raw.stderr_sha256);
 if(raw.error||raw.signal||![0,1,3].includes(raw.exit_code))continue;
 let report;try{report=read(raw.stdout);}catch{continue;}
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
   const pointer=raw.stdout+'#/results/'+i;
   if(r.variant===null&&r.stage==='setup'&&r.status==='fail'){
    assert(!rawSetups.has(r.path));rawSetups.set(r.path,{path:r.path,detail:r.detail,raw_record:pointer});
   }else{const id=key(r.path,r.variant);assert(!rawRows.has(id));rawRows.set(id,{outcome:r.status==='fail'&&r.timed_out===true?'timeout':r.status,raw_record:pointer});}
  }
  assert.deepEqual(count,report.counts);assert.equal(raw.exit_code,count.fail?1:count.pass?0:3);executionBatches.push(...raw.paths);
 }
}
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
console.log(JSON.stringify({integrity:'pass',files:universe.length,variants:totals.total,features:features.size,totals:summary(totals),corpus_revision:conditions.corpus_revision,qualification:'Recomputed report integrity and recorded outcomes; not full language feature conformance.'}));
