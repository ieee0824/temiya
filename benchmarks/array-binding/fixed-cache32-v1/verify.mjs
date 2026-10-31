import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../../..');
const read=p=>JSON.parse(fs.readFileSync(p)),hash=b=>createHash('sha256').update(b).digest('hex');
const evidence=read(root+'/evidence.json'),archive=root+'/capture.tar.gz',data=fs.readFileSync(archive);
assert.equal(evidence.archive.file,'capture.tar.gz');assert.equal(data.length,evidence.archive.bytes);assert.equal(hash(data),evidence.archive.sha256);
assert.equal(hash(fs.readFileSync(root+'/comparison.json')),evidence.comparison_sha256);
assert.equal(hash(fs.readFileSync(root+'/source-inputs.json')),evidence.source_inputs_sha256);
const sourceInputs=read(root+'/source-inputs.json');
assert.equal(sourceInputs.frozen_source,'e9854f8fd8d5c9aca3683403a6dc4280b3774854');
assert.equal(sourceInputs.qualification_source,'9f84bc5c6416911df51d76cf10b936ad3c3e003f');
assert.equal(sourceInputs.inputs.length,83);
for(const revision of [sourceInputs.frozen_source,sourceInputs.qualification_source])for(const input of sourceInputs.inputs)assert.equal(hash(execFileSync('git',['-C',repo,'show',revision+':'+input.file])),input.sha256,input.file);
const saved=read(root+'/comparison.json');
assert.equal(saved.source,'e9854f8fd8d5c9aca3683403a6dc4280b3774854');
assert.equal(saved.baseline_source,'c2bc478647815fdc013f254f5e1e72cfd804945b');
assert.equal(saved.initial_source,'208ecd71d2287269405b2b20630b73908650e8d2');
assert.equal(saved.files,8806);assert.equal(saved.variants,17414);
const temp=fs.mkdtempSync(path.join(os.tmpdir(),'temiya-array-fixed-pair-'));
const key=r=>JSON.stringify([r.path,r.variant]);
function compare(before,after){
 const map=new Map(after.map(r=>[key(r),r]));assert.equal(map.size,17414);assert.equal(before.length,17414);
 const changes={},regressions=[],added=[];let preserved=0;
 for(const a of before){const b=map.get(key(a));assert(b);const change=a.outcome+' -> '+b.outcome;changes[change]=(changes[change]??0)+1;
  if(a.outcome==='pass'){if(b.outcome==='pass')preserved++;else regressions.push({path:a.path,variant:a.variant,before:a.outcome,after:b.outcome});}
  else if(b.outcome==='pass')added.push({path:a.path,variant:a.variant,before:a.outcome});
 }
 return{changes,preserved,regressions,added};
}
try{
 const names=execFileSync('tar',['-tzf',archive],{encoding:'utf8',maxBuffer:32*1024*1024}).trim().split('\n');
 assert.equal(new Set(names).size,names.length);assert(names.every(n=>!path.isAbsolute(n)&&!n.split('/').some(p=>p==='.'||p==='..')));
 assert(execFileSync('tar',['-tvzf',archive],{encoding:'utf8',maxBuffer:32*1024*1024}).trim().split('\n').every(n=>n[0]==='d'||n[0]==='-'));
 execFileSync('tar',['-xzf',archive,'--no-same-owner','--no-same-permissions','-C',temp]);
 const captures={};
 for(const [name,source,backend,files,variants]of [
  ['before-ast',saved.baseline_source,'ast',8806,17414],['before-auto',saved.baseline_source,'auto',8806,17414],
  ['after-ast',saved.source,'ast',8806,17414],['after-auto',saved.source,'auto',8806,17414],
  ['initial/ast',saved.initial_source,'ast',8806,17414],['initial/auto-missing-recovery',saved.initial_source,'auto',4,8],
 ]){
  const dir=temp+'/'+name,result=JSON.parse(execFileSync(process.execPath,[repo+'/scripts/verify-test262-baseline.mjs',dir],{encoding:'utf8',maxBuffer:1024*1024}));
  assert.equal(result.integrity,'pass');assert.equal(result.files,files);assert.equal(result.variants,variants);
  const c=read(dir+'/conditions.json');assert.equal(c.backend_requested,backend);assert.equal(c.provided_engine_build_info.source_revision,source);
  assert.equal(c.engine_sha256,c.provided_engine_build_info.executable_sha256);assert.equal(c.timeout_ms,1000);assert.equal(c.memory_mb,512);assert.equal(c.stack_kb,8192);
  if(files===8806)assert.deepEqual(read(dir+'/selection.json'),read(root+'/../selection.json'));
  captures[name]={rows:read(dir+'/outcomes.json'),conditions:c,totals:result.totals};
 }
 // Reconstruct the original FAILED Auto collection without rewriting it into
 // a successful capture. Only its eight genuinely unreported variants may be
 // joined with the separately verified missing-only recovery above.
 const original=temp+'/initial/auto',b=read(original+'/baseline.json'),conditions=read(original+'/conditions.json');
 assert.deepEqual(b.conditions,conditions);assert.equal(conditions.provided_engine_build_info.source_revision,saved.initial_source);
 assert.equal(hash(fs.readFileSync(original+'/'+b.catalog.file)),b.catalog.sha256);assert.equal(hash(fs.readFileSync(original+'/'+b.outcomes.file)),b.outcomes.sha256);
 assert.equal(b.capture_errors.length,1);assert.equal(b.capture_errors[0].stage,'execution');assert.equal(b.file_setup_failures.length,0);
 const observed=new Map(),metadata=new Map(),executionPaths=[],bad=[];
 for(const raw of b.raw_reports){
  for(const [file,sha]of[[raw.stdout,raw.stdout_sha256],[raw.stderr,raw.stderr_sha256]]){
   assert(!path.isAbsolute(file)&&!file.split('/').some(p=>p==='.'||p==='..'));assert.equal(hash(fs.readFileSync(original+'/'+file)),sha);
  }
  if(raw.error||raw.signal||![0,1,3].includes(raw.exit_code)){bad.push(raw);executionPaths.push(...raw.paths);continue;}
  const report=read(original+'/'+raw.stdout);assert.equal(report.schema,1);assert.equal(report.corpus_revision,conditions.corpus_revision);assert.equal(report.manifest_revision,conditions.manifest_revision);
  if(raw.stage==='metadata'){
   assert.equal(report.engine_executed,false);assert.equal(report.kind,'test262-metadata');assert.deepEqual(report.cases.map(c=>c.path).sort(),[...raw.paths].sort());
   for(const c of report.cases){assert.equal(c.parsed,true);assert(!metadata.has(c.path));metadata.set(c.path,c);}
  }else{
   assert.equal(raw.stage,'execution');assert.equal(report.backend,'Auto');const counts={pass:0,fail:0,skip:0};
   for(const [i,r]of report.results.entries()){
    assert(raw.paths.includes(r.path));assert(Object.hasOwn(counts,r.status));counts[r.status]++;assert(!observed.has(key(r)));
    observed.set(key(r),{outcome:r.status==='fail'&&r.timed_out===true?'timeout':r.status,raw_record:raw.stdout+'#/results/'+i});
   }
   assert.deepEqual(counts,report.counts);assert.equal(raw.exit_code,counts.fail?1:counts.pass?0:3);executionPaths.push(...raw.paths);
  }
 }
 assert.equal(bad.length,1);assert.equal(bad[0].stdout,b.capture_errors[0].raw_report+'.json');assert.deepEqual(bad[0].paths,b.capture_errors[0].paths);
 const catalog=read(original+'/catalog.json');assert.deepEqual([...metadata.values()],catalog);assert.deepEqual([...executionPaths].sort(),read(original+'/selection.json'));
 const rows=catalog.flatMap(test=>test.variants.map(variant=>{const r=observed.get(key({path:test.path,variant}));return{path:test.path,variant,outcome:r?.outcome??'unreported',...(r?{raw_record:r.raw_record}:{})};}));
 assert.deepEqual(rows,read(original+'/outcomes.json'));assert.equal(rows.length,17414);
 const missing=rows.filter(r=>r.outcome==='unreported');assert.equal(missing.length,8);assert.equal(b.totals.unreported,8);
 const recovery=captures['initial/auto-missing-recovery'];
 for(const name of ['backend_requested','timeout_ms','memory_mb','stack_kb','engine_sha256','corpus_revision','manifest_revision','manifest_sha256'])assert.deepEqual(recovery.conditions[name],conditions[name]);
 assert.deepEqual(recovery.conditions.provided_engine_build_info,conditions.provided_engine_build_info);
 const recoveryMap=new Map(recovery.rows.map(r=>[key(r),r]));assert.deepEqual([...recoveryMap.keys()].sort(),missing.map(key).sort());
 const joined=rows.map(r=>{const capture=r.outcome==='unreported'?'auto-missing-recovery':'auto';const actual=capture==='auto'?r:recoveryMap.get(key(r));return{path:r.path,variant:r.variant,outcome:actual.outcome,raw_record:capture+'/'+actual.raw_record,capture};});
 const joinedRecord=read(temp+'/initial/auto-completed-observations.json');assert.deepEqual(joined,joinedRecord.rows);assert.deepEqual(joinedRecord.original_collection_errors,b.capture_errors);assert.equal(joinedRecord.original_unreported,8);assert.equal(joinedRecord.recovered_variants,8);
 const totals={};for(const r of joined)totals[r.outcome]=(totals[r.outcome]??0)+1;assert.deepEqual(totals,joinedRecord.totals);
 for(const backend of ['ast','auto']){
  const after=captures['after-'+backend],initial=backend==='ast'?captures['initial/ast'].rows:joined;
  assert.deepEqual(saved.totals[backend],after.totals);
  assert.deepEqual(saved.comparisons[backend].baseline,compare(captures['before-'+backend].rows,after.rows));
  assert.deepEqual(saved.comparisons[backend].initial,compare(initial,after.rows));
  assert.equal(saved.comparisons[backend].baseline.regressions.length,0);assert.equal(saved.comparisons[backend].initial.regressions.length,0);
 }
 const auto=captures['after-auto'].rows,ast=captures['after-ast'].rows;
 assert.equal(saved.ast_auto_different_outcomes,ast.filter((r,i)=>key(r)!==key(auto[i])||r.outcome!==auto[i].outcome).length);assert.equal(saved.ast_auto_different_outcomes,0);
 console.log(JSON.stringify({integrity:'pass',source:saved.source,variantsPerBackend:17414,totals:saved.totals,preservedInitial:{ast:saved.comparisons.ast.initial.preserved,auto:saved.comparisons.auto.initial.preserved},regressions:0,originalAutoCollectionErrors:1,originalUnreported:8,missingOnlyRecovered:8,nativeQualification:false}));
}finally{fs.rmSync(temp,{recursive:true,force:true});}
