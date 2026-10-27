import fs from 'node:fs';
import {createHash} from 'node:crypto';
const [file]=process.argv.slice(2),r=JSON.parse(fs.readFileSync(file));
const need=(b,s)=>{if(!b)throw Error(s);},hash=b=>createHash('sha256').update(b).digest('hex'),median=a=>[...a].sort((a,b)=>a-b)[Math.floor(a.length/2)];
need(r.input_sha256==='66911cc92b9cf12141fa42a66f21176457ec148cd3a09aa829eba22f4b228ce9'&&r.samples===15&&r.pairs===3&&r.workers.length===3&&r.runs.length===9&&r.comparisons.length===6,'complete unchanged protocol');
if(r.limits)need(r.limits.timeout_ms===180000&&r.limits.rss_limit_kib===524288&&r.limits.proc_poll_ms===25,'same guards');
const seen=new Set();for(const row of r.runs){const i=r.workers.findIndex(w=>w.name===row.worker),m=row.measurement,key=i+'/'+row.pair;
 need(i>=0&&[0,1,2].includes(row.pair)&&!seen.has(key)&&i===(row.pair+row.order)%3,'unique rotations');seen.add(key);
 need(row.status==='pass'&&row.exit.code===0&&row.exit.signal===null&&m.expected===2048*(8+5+2)&&m.validated_passes===16&&m.samples_ns.length===15&&m.samples_ns.every(n=>Number.isSafeInteger(n)&&n>0)&&m.warmup_ns>0,'144 checked results');
 const med=median(m.samples_ns),mad=median(m.samples_ns.map(n=>Math.abs(n-med)));need(m.median_ns===med&&m.mad_ns===mad,'independent median/MAD');
 if(i<2)need(m.setup_backend==='Bytecode'&&m.invocation_backend==='Bytecode'&&m.mode==='Auto Script'&&m.profile==='release','observed public default Engine');
}
for(const c of r.comparisons){const a=r.runs.find(x=>x.worker===r.workers[0].name&&x.pair===c.pair)?.measurement,b=r.runs.find(x=>x.worker===c.reference&&x.pair===c.pair)?.measurement;need(a&&b,'complete pair');const ratio=a.median_ns/b.median_ns,t=Math.max(.1,3*(a.mad_ns/a.median_ns+b.mad_ns/b.median_ns)),out=1-ratio>t?'candidate faster':ratio-1>t?'candidate slower':'within noise';need(c.ratio===ratio&&c.threshold===t&&c.outcome===out,'independent ratio/threshold/outcome');}
const gate=r.comparisons.filter(c=>c.reference===r.workers[2].name).every(c=>c.outcome==='candidate faster');if('all_cold_comparisons_faster_than_actual_boa'in r)need(r.all_cold_comparisons_faster_than_actual_boa===gate&&r.all_workloads_valid===true,'honest speed gate');
const boa=r.workers[2].build??r.workers[2];need(boa.omoikane_revision==='007556adccb8fb4329236932ca8175f8e8d119d2'&&boa.checkout_clean&&boa.original_lock_sha256==='f3988c4d9b0ecc28560df18df1cae7760c7c55a016318ea3e31f3ebbfcaea211'&&boa.execution_mode==='interpreter; baseline-jit feature absent'&&!boa.features.boa_engine.includes('baseline-jit')&&boa.features.regress.includes('utf16'),'actual pinned fork/mode');
console.log(JSON.stringify({path:file,sha256:hash(fs.readFileSync(file)),workers:9,checked_results:144,decisions:6,actual_boa_gate:gate,cpu:r.machine?.cpu_models??null}));
