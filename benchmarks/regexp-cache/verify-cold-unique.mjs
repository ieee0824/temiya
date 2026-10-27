// Offline verification; executes the authored oracle in Node, never times an engine.
import fs from 'node:fs';
import {createHash} from 'node:crypto';
import vm from 'node:vm';
const root=new URL('./cold-unique/',import.meta.url);
const bytes=fs.readFileSync(new URL('input.json',root));
const input=JSON.parse(bytes),r=JSON.parse(fs.readFileSync(new URL('report.json',root)));
const need=(b,s)=>{if(!b)throw Error(s);};
const hash=b=>createHash('sha256').update(b).digest('hex');
const median=a=>[...a].sort((a,b)=>a-b)[Math.floor(a.length/2)];
need(r.input_sha256===hash(bytes)&&input.iterations===2048&&input.expected===2048*(8+5+2),'input hash and independent oracle');
need(r.runner_source_sha256===hash(fs.readFileSync(new URL('../../scripts/compare-omoikane-workloads.mjs',import.meta.url))),'recorded watchdog source');
const context=vm.createContext({});vm.runInContext(input.setup,context);
for(let pass=0;pass<16;pass++)need(vm.runInContext(input.invocation,context)===30720,'Node independent result');
need(vm.runInContext('uniqueSerial;',context)===16*2048,'fresh serial for every construction');
need(r.samples===15&&r.pairs===3&&r.workers.length===3&&r.runs.length===9&&r.comparisons.length===6,'complete diagnostic');
const seen=new Set();
for(const row of r.runs){const index=r.workers.findIndex(w=>w.name===row.worker),m=row.measurement,key=row.worker+'/'+row.pair;
 need(index>=0&&[0,1,2].includes(row.pair)&&!seen.has(key)&&index===(row.pair+row.order)%3,'rotating unique coverage');seen.add(key);
 need(row.status==='pass'&&row.exit.code===0&&row.exit.signal===null&&m.expected===30720&&m.validated_passes===16&&m.samples_ns.length===15&&m.samples_ns.every(n=>Number.isSafeInteger(n)&&n>0)&&m.warmup_ns>0,'checked fresh-pattern passes');
 const med=median(m.samples_ns),mad=median(m.samples_ns.map(n=>Math.abs(n-med)));need(m.median_ns===med&&m.mad_ns===mad,'median/MAD');
 if(index<2)need(m.setup_backend==='Bytecode'&&m.invocation_backend==='Bytecode','actual default backend');
 need(row.sampled_peak_hwm_kib>0&&row.last_observed_rss_kib>0,'memory observed, never null-as-zero');
}
for(const c of r.comparisons){const a=r.runs.find(x=>x.worker===r.workers[0].name&&x.pair===c.pair).measurement,b=r.runs.find(x=>x.worker===c.reference&&x.pair===c.pair).measurement;
 const ratio=a.median_ns/b.median_ns,t=Math.max(.1,3*(a.mad_ns/a.median_ns+b.mad_ns/b.median_ns)),outcome=1-ratio>t?'candidate faster':ratio-1>t?'candidate slower':'within noise';need(c.ratio===ratio&&c.threshold===t&&c.outcome===outcome,'independent comparison');}
console.log(JSON.stringify({workers:9,checked_results:144,fresh_patterns_per_worker:32768,decisions:6,outcomes:r.comparisons.map(c=>({reference:c.reference,pair:c.pair,outcome:c.outcome,ratio:c.ratio}))},null,2));
