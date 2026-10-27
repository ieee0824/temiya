// Recalculate recorded timing evidence without executing an engine or benchmark.
import fs from 'node:fs';
import path from 'node:path';
import {createHash} from 'node:crypto';
import {regexpWorkloads} from '../../scripts/regexp-benchmark-reference.mjs';
const hash=b=>createHash('sha256').update(b).digest('hex');
const directory=process.argv[2];
if(!directory)throw Error('Usage: node verify-recorded-timing.mjs <evidence directory>');
const require=(condition,label)=>{if(!condition)throw Error(label);};
const median=values=>[...values].sort((a,b)=>a-b)[Math.floor(values.length/2)];
const expected=new Map([
 ['regexp-construction',128*(8+5+2)],['regexp-global-exec',2048*(3+4+2)],['regexp-unicode-captures',1024*6],
 ['arith',64],['prop-mono',500003500000],['prop-mega',1750000],['call',1000000],
 ['closure-alloc',715003],['object-alloc',730003],['string-concat',2494],['array',124999750000],
 ['primitive-string-property',499997],['primitive-string-method',999856],['proto-method',750000],
 ['promise-chain',2048],['promise-fanout',2096128],['promise-rejection-chain',2048],
]);
const iterations=new Map([['regexp-construction',128],['regexp-global-exec',2048],['regexp-unicode-captures',2048],['arith',2000000],['prop-mono',1000000],['prop-mega',500000],['call',1000000],['closure-alloc',300000],['object-alloc',300000],['string-concat',200000],['array',500000],['primitive-string-property',500000],['primitive-string-method',500000],['proto-method',500000],['promise-chain',2048],['promise-fanout',2048],['promise-rejection-chain',2048]]);
function* files(dir){for(const entry of fs.readdirSync(dir,{withFileTypes:true})){const file=path.join(dir,entry.name);if(entry.isDirectory())yield* files(file);else if(entry.name.endsWith('.json'))yield file;}}
let reports=0,workers=0,values=0,decisions=0;
for(const file of files(directory)){
 const r=JSON.parse(fs.readFileSync(file));
 if(!Array.isArray(r.runs)||!Array.isArray(r.comparisons)||!r.manifest?.workloads)continue;
 const names=r.manifest.workloads.map(w=>w.name),async=names.every(n=>n.startsWith('promise-')),regexp=names.every(n=>n.startsWith('regexp-'));
 require(names.length===(async||regexp?3:11)&&new Set(names).size===names.length,'full distinct workload set');
 require(r.samples===15&&r.pairs===3&&r.workers.length===2&&r.runs.length===names.length*6&&r.comparisons.length===names.length*3&&r.all_workloads_valid,'complete checked protocol');
 require(r.limits.timeout_ms===180000&&r.limits.rss_limit_kib===524288,'watchdogs');
 for(const w of r.manifest.workloads)require(expected.has(w.name)&&w.expected===expected.get(w.name)&&w.iterations===iterations.get(w.name),'independent oracle and original iterations');
 require(r.manifest.fixture_version===(async||regexp?1:2),'fixture version');
 if(regexp)require(r.manifest.fixture_kind==='authored-regexp'&&r.manifest.reference_engine_revision==='007556adccb8fb4329236932ca8175f8e8d119d2','authored reference pin');
 if(!async&&!regexp)require(r.manifest.revision==='007556adccb8fb4329236932ca8175f8e8d119d2'&&r.manifest.fixture_sha256==='177eb80ae73d1081eb620f747b4703e2a9f2731e0255bb60131b73b7dcac6464','pinned original fixture');
 if(regexp){
  require(r.authored_reference_sha256===hash(fs.readFileSync(new URL('../../scripts/regexp-benchmark-reference.mjs',import.meta.url))),'exact authored source hash');
  for(const [i,w]of r.manifest.workloads.entries()){const source=regexpWorkloads[i];require(w.name===source.name&&w.setup_sha256===hash(source.setup)&&w.invocation_sha256===hash(source.invocation),'exact authored setup and invocation');}
 }
 const group=[...expected.keys()].slice(regexp?0:async?14:3,regexp?3:async?17:14);
 require(names.every(n=>group.includes(n)),'exact suite workload set');
 const seen=new Set();
 for(const row of r.runs){const m=row.measurement,index=r.workers.findIndex(w=>w.name===row.worker),key=`${row.workload}/${index}/${row.pair}`;
  require(names.includes(row.workload)&&index>=0&&[0,1,2].includes(row.pair)&&!seen.has(key)&&row.order===(row.pair%2?1-index:index),'unique alternating coverage');seen.add(key);
  require(row.status==='pass'&&row.exit.code===0&&row.exit.signal===null&&m.expected===expected.get(row.workload)&&m.samples_ns.length===15&&m.validated_passes===16&&m.warmup_ns>0&&m.samples_ns.every(n=>Number.isSafeInteger(n)&&n>0),'checked results');
  const v=median(m.samples_ns),mad=median(m.samples_ns.map(n=>Math.abs(n-v)));
  require(m.median_ns===v&&m.mad_ns===mad,'independent median/MAD');
 }
 const pairs=new Set();
 for(const c of r.comparisons){const key=`${c.workload}/${c.pair}`;require(!pairs.has(key),'unique pair decision');pairs.add(key);
  const [a,b]=r.workers.map(w=>r.runs.find(row=>row.workload===c.workload&&row.pair===c.pair&&row.worker===w.name)?.measurement);require(a&&b,'complete pair');
  const boa='temiya_to_boa_ratio'in c,ratio=a.median_ns/b.median_ns,threshold=Math.max(.1,3*(a.mad_ns/a.median_ns+b.mad_ns/b.median_ns)),prefix=boa?'Temiya':'after';
  const outcome=1-ratio>threshold?`${prefix} faster beyond threshold`:ratio-1>threshold?`${prefix} slower beyond threshold`:'within noise threshold';
  require(c[boa?'temiya_to_boa_ratio':'after_to_before_ratio']===ratio&&c.noise_threshold===threshold&&c.outcome===outcome,'independent ratio/threshold/decision');
 }
 for(const [key,gate]of Object.entries(r))if(key.startsWith('all_')&&key.endsWith('_faster_beyond_threshold'))require(gate===r.comparisons.every(c=>c.outcome.endsWith('faster beyond threshold')),'complete speed gate');
 reports++;workers+=r.runs.length;values+=r.runs.length*16;decisions+=r.comparisons.length;
 console.log(path.relative(directory,file)+': '+JSON.stringify(r.comparisons.reduce((a,c)=>(a[c.outcome]=(a[c.outcome]??0)+1,a),{})));
}
require(reports>0,'no recorded reports found');
console.log(JSON.stringify({reports,workers,checked_results:values,pair_decisions:decisions}));
