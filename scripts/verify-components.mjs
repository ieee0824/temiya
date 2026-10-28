// Recompute stored component evidence; no execution or cross-engine speed claim.
import fs from 'node:fs';
import {componentVersion,componentCases,validateComponent,statistics} from './component-benchmark-reference.mjs';
const file=process.argv[2];if(!file)throw Error('usage: verify-components.mjs REPORT');
const report=JSON.parse(fs.readFileSync(file));
const require=(ok,label)=>{if(!ok)throw Error(label);};
require(report.fixture_version===componentVersion&&report.samples===15&&report.rounds===3&&report.runs.length===18,'complete repeated protocol');
require(report.limits.timeout_ms===180000&&report.limits.rss_limit_kib===524288,'fixed watchdogs');
require(report.build.source_clean===true&&/^[a-f0-9]{40}$/.test(report.build.revision)&&report.build.profile==='release'&&report.build.features.length===0,'release source provenance');
const seen=new Set();let checked=0;
for(const row of report.runs){
 const contract=componentCases.find(c=>c.name===row.name),key=row.round+'/'+row.name;
 require(contract&&!seen.has(key)&&[0,1,2].includes(row.round),'distinct coverage');seen.add(key);
 const index=componentCases.indexOf(contract),order=(index-row.round+componentCases.length)%componentCases.length;
 require(row.order===order,'declared rotation');
 if(row.status==='pass'){
  require(row.exit.code===0&&row.exit.signal===null,'actual successful child');
  validateComponent(row.measurement,contract,15);checked+=row.measurement.validated_passes;
  require(JSON.stringify(row.statistics)===JSON.stringify(statistics(row.measurement.samples_ns)),'independent median/MAD');
  require(JSON.stringify(JSON.parse(row.stdout))===JSON.stringify(row.measurement),'raw worker output');
 }
}
require(report.all_component_cases_valid===report.runs.every(r=>r.status==='pass'),'correctness gate, including failures');
const ownership=report.ownership;
require([0,2].includes(ownership.exit_code)&&ownership.result.reclamation_ready===(ownership.exit_code===0)&&JSON.stringify(JSON.parse(ownership.stdout))===JSON.stringify(ownership.result),'actual ownership observation');
require(ownership.result.cases.length>0&&ownership.result.cases.every(c=>c.alive_with_handle&&c.alive_with_clone),'ownership handle roots observed');
require(ownership.result.reclamation_ready===ownership.result.cases.every(c=>!c.alive_after_drop)&&report.engine_gc_reclamation_ready===ownership.result.reclamation_ready,'reclamation readiness, never overridden by standalone GC');
console.log(JSON.stringify({workers:report.runs.length,checked_results:checked,all_component_cases_valid:report.all_component_cases_valid,engine_gc_reclamation_ready:report.engine_gc_reclamation_ready,qualification:report.qualification}));
