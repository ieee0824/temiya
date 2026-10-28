import fs from 'node:fs';import{createHash}from'node:crypto';
const dir='/tmp/temiya-gc-async-verification',root='/tmp/temiya-traced-runtime-graph';
const r=JSON.parse(fs.readFileSync(dir+'/async.json'));
const t=JSON.parse(fs.readFileSync(dir+'/temiya-build-info.json')),b=JSON.parse(fs.readFileSync(dir+'/boa-build-info.json'));
const checks=JSON.parse(fs.readFileSync(dir+'/worker-checks.json')).checks;
const hash=b=>createHash('sha256').update(b).digest('hex');
const median=a=>[...a].sort((x,y)=>x-y)[Math.floor(a.length/2)];
const require=(ok,label)=>{if(!ok)throw Error(label);};
require(r.runs.length===18&&r.comparisons.length===9&&r.samples===15&&r.pairs===3&&r.all_workloads_valid,'full protocol');
require(r.limits.rss_limit_kib===524288&&r.limits.timeout_ms===180000&&r.qualification==='repeated comparison','guards');
require(t.revision===fs.readFileSync(dir+'/measured-revision.txt','utf8').trim()&&r.workers[0].revision===t.revision&&r.workers[0].executable_sha256===t.executable_sha256,'Temiya identity');
require(t.source_sha256===hash(fs.readFileSync(root+'/examples/measure_async_workload.rs'))&&r.workers[0].source_sha256===t.source_sha256&&t.cargo_lock_sha256===hash(fs.readFileSync(root+'/Cargo.lock')),'Temiya source/dependencies');
require(t.rustc===r.rustc&&r.rustc===fs.readFileSync(dir+'/compiler.txt','utf8').trim()&&b.rustc===r.rustc,'compiler identity');
require(JSON.stringify(r.workers[1].build)===JSON.stringify(b)&&b.workload_kind==='async'&&b.checkout_clean&&b.omoikane_revision==='007556adccb8fb4329236932ca8175f8e8d119d2'&&b.worker_source_sha256===hash(fs.readFileSync(root+'/benchmarks/omoikane-async-worker/main.rs')),'actual pinned app source');
require(JSON.stringify(r.manifest)===JSON.stringify(JSON.parse(fs.readFileSync(dir+'/manifest.json'))),'manifest');
const definitions=(await import(root+'/scripts/async-benchmark-reference.mjs')).asyncWorkloads;
const oracles={'promise-chain':2048,'promise-fanout':2096128,'promise-rejection-chain':2048};
const seen=new Set();
for(const w of r.manifest.workloads){const d=definitions.find(d=>d.name===w.name);require(d&&w.iterations===2048&&w.expected===oracles[w.name],'independent oracle');for(const key of ['setup','invocation','completion'])require(hash(d[key])===w[key+'_sha256'],'exact input source');}
for(const row of r.runs){
 const m=row.measurement,index=r.workers.findIndex(w=>w.name===row.worker),w=r.manifest.workloads.find(w=>w.name===row.workload),key=row.workload+'/'+row.worker+'/'+row.pair;
 require(w&&index>=0&&[0,1,2].includes(row.pair)&&!seen.has(key)&&row.order===(row.pair%2?1-index:index),'unique alternating coverage');seen.add(key);
 require(row.status==='pass'&&row.exit.code===0&&row.exit.signal===null&&m.validated_passes===16&&m.samples_ns.length===15&&m.samples_ns.every(n=>Number.isSafeInteger(n)&&n>0)&&m.warmup_ns>0,'validated samples');
 require(m.engine===row.worker&&m.mode===r.workers[index].mode&&m.profile==='release'&&m.expected===oracles[row.workload]&&m.before_drain_expected===w.before_drain_expected&&m.scope==='parse+evaluate scheduling; drain jobs; parse+evaluate completion; setup excluded','real scheduling/checkpoint scope and oracle');
 if(index===0)require(m.setup_backend==='Ast'&&m.invocation_backends.length===15&&m.completion_backends.length===15&&m.drained_jobs.length===15&&m.drained_jobs.every(n=>n>0)&&m.warmup_drained_jobs>0,'actual backends/jobs');
 else require(m.drained_jobs===null&&m.sandbox.timeout_seconds===60&&m.sandbox.max_loop_iterations===100000000,'application sandbox/no fake counts');
 const med=median(m.samples_ns),mad=median(m.samples_ns.map(n=>Math.abs(n-med)));require(med===m.median_ns&&mad===m.mad_ns,'independent median/MAD');
}
const decisions=new Set();
for(const c of r.comparisons){const key=c.workload+'/'+c.pair;require(!decisions.has(key),'unique decision');decisions.add(key);const [a,b]=r.workers.map(w=>r.runs.find(x=>x.workload===c.workload&&x.pair===c.pair&&x.worker===w.name)?.measurement);require(a&&b,'pair complete');const ratio=a.median_ns/b.median_ns,threshold=Math.max(.1,3*(a.mad_ns/a.median_ns+b.mad_ns/b.median_ns)),outcome=1-ratio>threshold?'Temiya faster beyond threshold':ratio-1>threshold?'Temiya slower beyond threshold':'within noise threshold';require(c.status==='comparable'&&c.temiya_to_boa_ratio===ratio&&c.noise_threshold===threshold&&c.outcome===outcome,'independent decision');}
require(r.all_authored_async_workloads_faster_beyond_threshold===r.comparisons.every(c=>c.outcome==='Temiya faster beyond threshold'),'speed gate');
require(checks.length===14&&new Set(checks.map(c=>c.worker+'/'+c.name)).size===14,'adapter coverage');for(const w of r.workers)for(const name of ['queued-result','negative-zero','wrong-completion','wrong-before-checkpoint','wrong-after-warmup','callback-throw','completion-throw']){const c=checks.find(c=>c.worker===w.executable&&c.name===name);const success=['queued-result','negative-zero'].includes(name);require(c&&c.expected_success===success&&Number.isInteger(c.exit_status)&&(c.exit_status===0)===success,'actual native adapter checks');}
console.log(JSON.stringify({workers:18,checked_results:288,adapter_checks:14,comparisons:9,outcomes:r.comparisons.reduce((a,c)=>(a[c.outcome]=(a[c.outcome]??0)+1,a),{}),report_sha256:hash(fs.readFileSync(dir+'/async.json')),source:t.revision},null,2));
