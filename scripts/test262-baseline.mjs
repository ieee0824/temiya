// Capture real isolated CLI outcomes; metadata/unexecuted cases never imply pass.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import {spawnSync,execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {fileURLToPath} from 'node:url';
const hash=b=>createHash('sha256').update(b).digest('hex');
const read=f=>JSON.parse(fs.readFileSync(f));
const optionalText=f=>{try{return fs.readFileSync(f,'utf8').trim();}catch{return null;}};
const options={backend:'auto','timeout-ms':'1000','memory-mb':'512','stack-kb':'8192','batch-size':'16'};
const valued=new Set(['engine','engine-info','corpus','output','manifest','universe','selection','backend','timeout-ms','memory-mb','stack-kb','batch-size']);
let pinned=false;
for(let i=2;i<process.argv.length;i++){
 const arg=process.argv[i];
 if(arg==='--pinned'){pinned=true;continue;}
 if(arg==='--help'){console.log('node scripts/test262-baseline.mjs --engine EXE --corpus DIR --output NEW_DIR [--pinned] [--engine-info BUILD_JSON] [--manifest JSON] [--universe PATHS_JSON] [--selection PATHS_JSON] [--backend auto|ast|bytecode] [--timeout-ms 1000] [--memory-mb 512] [--stack-kb 8192] [--batch-size 16]');process.exit(0);}
 if(!arg.startsWith('--')||!valued.has(arg.slice(2))||i+1===process.argv.length)throw Error('invalid option: '+arg);
 options[arg.slice(2)]=process.argv[++i];
}
for(const key of ['engine','corpus','output'])if(!options[key])throw Error('--'+key+' is required');
if(!['auto','ast','bytecode'].includes(options.backend))throw Error('invalid backend');
for(const [name,min,max]of [['timeout-ms',1,120000],['memory-mb',16,65536],['stack-kb',128,65536],['batch-size',1,64]]){
 const n=Number(options[name]);if(!Number.isInteger(n)||n<min||n>max)throw Error('invalid --'+name);options[name]=n;
}
const output=path.resolve(options.output),corpus=path.resolve(options.corpus),engine=path.resolve(options.engine);
if(fs.existsSync(output)&&fs.readdirSync(output).length)throw Error('output directory must be empty');
const manifestBytes=fs.readFileSync(options.manifest??new URL('../test262/manifest.json',import.meta.url));
const manifest=JSON.parse(manifestBytes),revision=pinned?manifest.revision:null;
if(!/^[a-f0-9]{40}$/.test(manifest.revision))throw Error('invalid manifest revision');
function paths(value){
 if(!Array.isArray(value)||value.some(p=>typeof p!=='string'||!p||p.includes('\\')||path.posix.isAbsolute(p)||p.split('/').some(s=>!s||s==='.'||s==='..')))throw Error('expected canonical corpus-relative paths');
 if(new Set(value).size!==value.length)throw Error('duplicate test path');return [...value].sort();
}
function walk(directory,prefix='test'){
 return fs.readdirSync(directory,{withFileTypes:true}).flatMap(e=>e.isDirectory()?walk(path.join(directory,e.name),prefix+'/'+e.name):e.isFile()?[prefix+'/'+e.name]:[]);
}
let excluded=[];
let universe;
if(options.universe)universe=paths(read(options.universe));
else{
 const files=pinned?execFileSync('git',['-c','safe.directory='+corpus,'-C',corpus,'ls-tree','-r','-z','--name-only',revision,'--','test/'],{encoding:'utf8',maxBuffer:32*1024*1024}).split('\0').filter(Boolean):walk(path.join(corpus,'test'));
 const js=files.filter(p=>p.endsWith('.js'));
 excluded=js.filter(p=>p.endsWith('_FIXTURE.js')||p.startsWith('test/staging/'));
 universe=paths(js.filter(p=>!p.endsWith('_FIXTURE.js')&&!p.startsWith('test/staging/')));
}
const selected=options.selection?paths(read(options.selection)):universe;
const universeSet=new Set(universe);if(selected.some(p=>!universeSet.has(p)))throw Error('selection is outside the indexed universe');
const selectedSet=new Set(selected);
fs.mkdirSync(path.join(output,'raw'),{recursive:true});
fs.writeFileSync(path.join(output,'manifest.json'),manifestBytes);
fs.writeFileSync(path.join(output,'universe.json'),JSON.stringify(universe,null,2)+'\n');
fs.writeFileSync(path.join(output,'selection.json'),JSON.stringify(selected,null,2)+'\n');
const temporary=fs.mkdtempSync(path.join(os.tmpdir(),'temiya-baseline-engine-'));
const frozen=path.join(temporary,'temiya-test262');fs.copyFileSync(engine,frozen);fs.chmodSync(frozen,0o700);
try {
const engineHash=hash(fs.readFileSync(frozen));
const engineInfo=options['engine-info']?read(options['engine-info']):null;
if(engineInfo&&engineInfo.executable_sha256!==engineHash)throw Error('engine build-info hash does not match frozen executable');
const conditions={backend_requested:options.backend,module_execution_path:'AST ModuleGraph',timeout_ms:options['timeout-ms'],memory_mb:options['memory-mb'],stack_kb:options['stack-kb'],batch_size:options['batch-size'],engine_sha256:engineHash,provided_engine_build_info:engineInfo,engine_frozen_during_run:true,corpus_revision:revision,manifest_revision:manifest.revision,manifest_sha256:hash(manifestBytes),universe_sha256:hash(JSON.stringify(universe)),selection_sha256:hash(JSON.stringify(selected)),controller_sha256:hash(fs.readFileSync(fileURLToPath(import.meta.url))),host:{platform:os.platform(),architecture:os.arch(),kernel:os.release(),cpu_models:[...new Set(os.cpus().map(c=>c.model))].sort(),node:process.version,cpu_allowed_list:optionalText('/proc/self/status')?.match(/^Cpus_allowed_list:\s*(.+)$/m)?.[1]??null,cgroup_cpu_max:optionalText('/sys/fs/cgroup/cpu.max'),cgroup_memory_max:optionalText('/sys/fs/cgroup/memory.max'),locale_environment:{TZ:process.env.TZ??null,LANG:process.env.LANG??null,LC_ALL:process.env.LC_ALL??null}} ,qualification:pinned?'Fixed Git corpus and frozen executable; selected results only; language conformance is not inferred from feature counts.':'Unpinned authored/local inputs; not a fixed-revision conformance qualification.'};
fs.writeFileSync(path.join(output,'conditions.json'),JSON.stringify(conditions,null,2)+'\n');
const cases=new Map(),rows=new Map(),rawReports=[],errors=[],setupFailures=new Map(),groups=new Map();
const counters=()=>({total:0,pass:0,fail:0,skip:0,timeout:0,not_run:0,unreported:0});
const totals=counters(),untagged={feature:null,files:0,selected_files:0,setup_failure_files:0,...counters()};
const started=new Date().toISOString();let indexed=0,completedFiles=0;
const key=(p,v)=>JSON.stringify([p,v]);
const featureGroups=c=>c.features.length?[...new Set(c.features)].map(name=>{
 if(!groups.has(name))groups.set(name,{feature:name,files:0,selected_files:0,setup_failure_files:0,...counters()});return groups.get(name);
}):[untagged];
const summarize=c=>({...c,reported_attempts:c.pass+c.fail+c.timeout,pass_rate_reported_attempts:c.pass+c.fail+c.timeout?c.pass/(c.pass+c.fail+c.timeout):null});
function progress(stage){fs.writeFileSync(path.join(output,'progress.json'),JSON.stringify({stage,started_at:started,updated_at:new Date().toISOString(),indexed_files:indexed,universe_files:universe.length,completed_selected_files:completedFiles,selected_files:selected.length,totals:summarize(totals),capture_errors:errors.length},null,2)+'\n');}
function transition(row,outcome,record){
 if(row.outcome!=='not_run')throw Error('duplicate variant outcome');
 for(const c of [totals,...featureGroups(cases.get(row.path))]){c.not_run--;c[outcome]++;}
 row.outcome=outcome;if(record)row.raw_record=record;
}
function capture(stage,batch,number){
 const stem='raw/'+stage+'-'+String(number).padStart(5,'0');
 const stdout=fs.openSync(path.join(output,stem+'.json'),'wx'),stderr=fs.openSync(path.join(output,stem+'.stderr'),'wx');
 const args=['--corpus',corpus,'--manifest',path.join(output,'manifest.json'),'--backend',options.backend,'--timeout-ms',String(options['timeout-ms']),'--memory-mb',String(options['memory-mb']),'--stack-kb',String(options['stack-kb'])];
 if(pinned)args.push('--pinned');if(stage==='metadata')args.push('--metadata-only');args.push('--',...batch);
 const variants=stage==='metadata'?0:batch.reduce((s,p)=>s+(cases.get(p)?.variants?.length??1),0);
 const result=spawnSync(frozen,args,{stdio:['ignore',stdout,stderr],detached:process.platform!=='win32',timeout:Math.max(60000,variants*options['timeout-ms']+30000)});
 fs.closeSync(stdout);fs.closeSync(stderr);
 if(result.pid&&process.platform!=='win32'){try{process.kill(-result.pid,'SIGKILL');}catch(e){if(e.code!=='ESRCH')throw e;}}
 const bytes=fs.readFileSync(path.join(output,stem+'.json'));
 const record={stage,paths:batch,stdout:stem+'.json',stderr:stem+'.stderr',stdout_sha256:hash(bytes),stderr_sha256:hash(fs.readFileSync(path.join(output,stem+'.stderr'))),exit_code:result.status,signal:result.signal,error:result.error?.message??null};rawReports.push(record);
 try{
  if(result.error||result.signal||![0,1,3].includes(result.status))throw Error('CLI terminated without a normal report');
  const r=JSON.parse(bytes);if(r.schema!==1||r.corpus_revision!==revision||r.manifest_revision!==manifest.revision)throw Error('report provenance mismatch');
  if(stage==='metadata'){
   if(r.kind!=='test262-metadata'||r.engine_executed!==false||!Array.isArray(r.cases)||r.cases.length!==batch.length||new Set(r.cases.map(c=>c.path)).size!==batch.length||r.cases.some(c=>!batch.includes(c.path)||c.engine_executed!==false))throw Error('invalid metadata report');
   if(result.status!==(r.cases.some(c=>c.parsed!==true)?1:0))throw Error('metadata exit mismatch');
  }else{
   if(r.backend!==({ast:'Ast',auto:'Auto',bytecode:'Bytecode'})[options.backend]||!Array.isArray(r.results))throw Error('invalid execution report');
   const count={pass:0,fail:0,skip:0},seen=new Set();
   for(const row of r.results){if(!batch.includes(row.path)||!Object.hasOwn(count,row.status))throw Error('invalid result identity/status');const id=key(row.path,row.variant);if(seen.has(id))throw Error('duplicate result');seen.add(id);count[row.status]++;}
   if(JSON.stringify(count)!==JSON.stringify({pass:r.counts.pass,fail:r.counts.fail,skip:r.counts.skip}))throw Error('result totals mismatch');
   if(result.status!==(count.fail?1:count.pass?0:3))throw Error('execution exit mismatch');
  }
  return {report:r,stem};
 }catch(e){errors.push({stage,paths:batch,raw_report:stem,detail:e.message});return null;}
}
 for(let offset=0;offset<universe.length;offset+=options['batch-size']){
  const batch=universe.slice(offset,offset+options['batch-size']),captured=capture('metadata',batch,offset/options['batch-size']);
  for(const p of batch){
   const c=captured?.report.cases.find(c=>c.path===p)??{path:p,parsed:false,engine_executed:false,detail:'metadata report unavailable'};
   if(c.parsed===true&&(!Array.isArray(c.features)||c.features.some(f=>typeof f!=='string')||!Array.isArray(c.variants)||c.variants.length<1||c.variants.length>2||new Set(c.variants).size!==c.variants.length||c.variants.some(v=>!['Sloppy','Strict','Module'].includes(v))))throw Error('invalid metadata fields');
   cases.set(p,c);indexed++;
   if(c.parsed!==true)continue;
   for(const g of featureGroups(c)){g.files++;if(selectedSet.has(p))g.selected_files++;}
   for(const variant of c.variants){const row={path:p,variant,outcome:'not_run'};rows.set(key(p,variant),row);for(const g of [totals,...featureGroups(c)]){g.total++;g.not_run++;}}
  }
  progress('indexing');
 }
 for(let offset=0;offset<selected.length;offset+=options['batch-size']){
  const batch=selected.slice(offset,offset+options['batch-size']),captured=capture('execution',batch,offset/options['batch-size']);
  if(captured){
   for(const [index,r]of captured.report.results.entries()){
    const pointer=captured.stem+'.json#/results/'+index,c=cases.get(r.path);
    if(r.variant===null&&r.stage==='setup'&&r.status==='fail'){
     setupFailures.set(r.path,{path:r.path,detail:r.detail,raw_record:pointer});
     if(c.parsed===true)for(const g of featureGroups(c))g.setup_failure_files++;
     continue;
    }
    const row=rows.get(key(r.path,r.variant));if(!row)throw Error('execution variant absent from metadata index');
    transition(row,r.status==='fail'&&r.timed_out===true?'timeout':r.status,pointer);
   }
  }
  for(const p of batch)for(const variant of cases.get(p)?.variants??[]){const row=rows.get(key(p,variant));if(row.outcome==='not_run')transition(row,'unreported');}
  completedFiles+=batch.length;progress('executing');
 }
 const catalog=[...cases.values()],outcomes=[...rows.values()];
 fs.writeFileSync(path.join(output,'catalog.json'),JSON.stringify(catalog,null,2)+'\n');
 fs.writeFileSync(path.join(output,'outcomes.json'),JSON.stringify(outcomes,null,2)+'\n');
 const baseline={schema:1,kind:'test262-feature-baseline',started_at:started,completed_at:new Date().toISOString(),conditions,universe:{enumeration:options.universe?'explicit path list':'test/**/*.js excluding _FIXTURE.js and test/staging',files:universe.length,excluded_paths:excluded,selected_files:selected.length,metadata_error_files:catalog.filter(c=>c.parsed!==true).length,variant_count_exact:catalog.every(c=>c.parsed===true)},totals:summarize(totals),features:[...groups.values()].sort((a,b)=>a.feature<b.feature?-1:a.feature>b.feature?1:0).map(summarize),untagged:summarize(untagged),file_setup_failures:[...setupFailures.values()],capture_errors:errors,raw_reports:rawReports,catalog:{file:'catalog.json',sha256:hash(fs.readFileSync(path.join(output,'catalog.json')))},outcomes:{file:'outcomes.json',sha256:hash(fs.readFileSync(path.join(output,'outcomes.json')))},scope:'Feature rows overlap for multi-feature tests. Timeout is exclusive here and a subset of raw CLI fail. Not-run/unreported/skip are never passes. Metadata errors have unknown variant/feature counts and separate file failures.'};
 fs.writeFileSync(path.join(output,'baseline.json'),JSON.stringify(baseline,null,2)+'\n');progress('completed');
 console.log(JSON.stringify({baseline:path.join(output,'baseline.json'),totals:baseline.totals,metadata_error_files:baseline.universe.metadata_error_files,capture_errors:errors.length}));
 if(errors.length||totals.unreported)process.exitCode=1;
}finally{fs.rmSync(temporary,{recursive:true,force:true});}
