// Serial isolated measurements. Results are component observations, not cross-engine wins.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {spawn,spawnSync,execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {componentVersion,componentCases,validateComponent,statistics} from './component-benchmark-reference.mjs';
const [infoFile]=process.argv.slice(2);if(!infoFile||process.argv.length!==3)throw Error('usage: measure-components.mjs BUILD_INFO');
if(process.platform!=='linux')throw Error('Linux /proc watchdog required');
const hash=b=>createHash('sha256').update(b).digest('hex'),build=JSON.parse(fs.readFileSync(infoFile));
if(!build.source_clean||!/^[a-f0-9]{40}$/.test(build.revision)||build.profile!=='release'||build.features.length!==0||build.release_debug!==false||build.rustflags!==''||build.rustc!==execFileSync('rustc',['-Vv'],{encoding:'utf8'}).trim())throw Error('frozen build provenance mismatch');
for(const worker of Object.values(build.workers))if(hash(fs.readFileSync(worker.executable))!==worker.sha256)throw Error('executable changed');
for(const file of ['examples/measure_components.rs','examples/ownership_probe.rs','examples/support/ownership.rs','scripts/component-benchmark-reference.mjs','Cargo.lock']){
 const current=fs.readFileSync(new URL('../'+file,import.meta.url));if(hash(current)!==build.source_files[file])throw Error('measured build and driver input differ: '+file);
}
const ownership=spawnSync(build.workers.ownership_probe.executable,['--require-reclamation'],{encoding:'utf8',timeout:180000});
let ownership_result;try{ownership_result=JSON.parse(ownership.stdout);}catch{throw Error('ownership observation missing');}
if(![0,2].includes(ownership.status)||ownership.error||ownership.signal||ownership_result.reclamation_ready!==(ownership.status===0))throw Error('ownership probe command failed');
const report={fixture_version:componentVersion,build,machine:{platform:os.platform(),release:os.release(),architecture:os.arch(),logical_cpus:os.cpus().length,cpu_models:[...new Set(os.cpus().map(c=>c.model))]},samples:15,rounds:3,limits:{timeout_ms:180000,rss_limit_kib:524288,proc_poll_ms:25},ownership:{exit_code:ownership.status,stdout:ownership.stdout,stderr:ownership.stderr,result:ownership_result},runs:[],qualification:'Single-engine component API measurements. Standalone Heap is not Engine GC; no cross-engine speed gate.'};
async function measure(contract,round,order){
 const row={name:contract.name,round,order,status:'error',sampled_peak_hwm_kib:null,last_observed_rss_kib:null};
 const child=spawn(build.workers.measure_components.executable,[contract.name,'15'],{stdio:['ignore','pipe','pipe']});
 let stdout='',stderr='',stopped;
 const stop=reason=>{stopped??=reason;child.kill('SIGKILL');};
 const interval=setInterval(()=>{try{const text=fs.readFileSync('/proc/'+child.pid+'/status','utf8');for(const [key,pattern] of [['sampled_peak_hwm_kib',/^VmHWM:\s+(\d+) kB$/m],['last_observed_rss_kib',/^VmRSS:\s+(\d+) kB$/m]]){const value=text.match(pattern);if(value)row[key]=key==='sampled_peak_hwm_kib'?Math.max(row[key]??0,Number(value[1])):Number(value[1]);}if(row.last_observed_rss_kib>report.limits.rss_limit_kib)stop('memory_limit');}catch{/* Process exited. */}},report.limits.proc_poll_ms);
 const timeout=setTimeout(()=>stop('timeout'),report.limits.timeout_ms);
 child.stdout.on('data',b=>{stdout+=b;if(stdout.length>1000000)stop('output_limit');});
 child.stderr.on('data',b=>{stderr+=b;if(stderr.length>1000000)stop('output_limit');});
 try{
  row.exit=await new Promise((resolve,reject)=>{child.once('error',reject);child.once('close',(code,signal)=>resolve({code,signal}));});
  row.stdout=stdout;row.stderr=stderr;
  if(stopped)row.status=stopped;
  else if(row.exit.code===0&&row.exit.signal===null){
   row.measurement=JSON.parse(stdout);validateComponent(row.measurement,contract,report.samples);
   row.statistics=statistics(row.measurement.samples_ns);row.status='pass';
  }
 }catch(error){row.error=String(error);row.stdout=stdout;row.stderr=stderr;}
 finally{clearInterval(interval);clearTimeout(timeout);}
 return row;
}
for(let round=0;round<report.rounds;round++){
 // Declared rotation balances serial order; it never selects favorable results.
 const ordered=[...componentCases.slice(round),...componentCases.slice(0,round)];
 for(const [order,contract] of ordered.entries()){
  const row=await measure(contract,round,order);report.runs.push(row);
  process.stderr.write(JSON.stringify({round,order,name:contract.name,status:row.status,statistics:row.statistics})+'\n');
 }
}
report.all_component_cases_valid=report.runs.every(row=>row.status==='pass');
report.engine_gc_reclamation_ready=ownership_result.reclamation_ready;
console.log(JSON.stringify(report,null,2));
if(!report.all_component_cases_valid)process.exitCode=1;
