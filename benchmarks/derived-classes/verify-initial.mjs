// Verify the initial candidate without turning noisy controls into source effects.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..');
const read=p=>JSON.parse(fs.readFileSync(p)),sha=b=>createHash('sha256').update(b).digest('hex');
const e=read(path.join(root,'initial-v1/evidence.json'));assert.equal(e.source_revision,'061ffcded6b11f31fa49b967fecce1842c467984');
assert.equal(e.archive.file,'capture.tar.gz');const archive=path.join(root,'initial-v1',e.archive.file),bytes=fs.readFileSync(archive);
assert.equal(bytes.length,e.archive.bytes);assert.equal(sha(bytes),e.archive.sha256);
const temp=fs.mkdtempSync(path.join(os.tmpdir(),'temiya-derived-initial-'));
const run=(name,args)=>JSON.parse(execFileSync(process.execPath,[path.join(repo,'scripts',name),...args],{encoding:'utf8',maxBuffer:16*1024*1024,cwd:repo}));
try {
 const names=execFileSync('tar',['-tzf',archive],{encoding:'utf8'}).trim().split('\n');
 assert.equal(new Set(names).size,names.length);assert(names.every(n=>!path.isAbsolute(n)&&!n.split('/').some(p=>p==='..'||p==='.')));
 assert(execFileSync('tar',['-tvzf',archive],{encoding:'utf8'}).trim().split('\n').every(n=>n[0]==='d'||n[0]==='-'));
 execFileSync('tar',['-xzf',archive,'--no-same-owner','--no-same-permissions','-C',temp]);
 const audit=run('audit-class-workload-records.mjs',['--derived',temp+'/derived-boa.json',temp+'/derived-same-binary.json']);
 assert.deepEqual(audit.reports.map(r=>r.outcomes),[
  {'within noise threshold':3,'Temiya faster beyond threshold':12},
  {'after faster beyond threshold':1,'within noise threshold':13,'after slower beyond threshold':1},
 ]);
 const performance=read(temp+'/derived-boa.json');assert.equal(performance.workers[0].revision,e.source_revision);
 assert.equal(performance.workers[0].executable_sha256,e.worker_sha256);
 const self=read(temp+'/derived-same-binary.json');assert(self.workers.every(w=>w.executable_sha256===e.worker_sha256));
 assert(performance.comparisons.filter(r=>r.workload==='derived-explicit-construct').every(r=>r.outcome==='within noise threshold'));
 const info=read(temp+'/engine-info.json');assert.equal(info.source_revision,e.source_revision);assert.equal(info.executable_sha256,e.engine_sha256);assert.equal(info.source_clean,true);
 const inputs=read(temp+'/original-inputs.json');assert.equal(inputs.files.length,209);assert.equal(inputs.files.length,e.source_inputs);
 for(const row of inputs.files){assert(!path.isAbsolute(row.file)&&!row.file.split('/').includes('..'));const bytes=fs.readFileSync(path.join(temp,'original-inputs',row.file));assert.equal(bytes.length,row.bytes);assert.equal(sha(bytes),row.sha256);}
 const selected=fs.readFileSync(temp+'/derived-originals/selection.json');assert.equal(sha(selected),'2d779e799276ea60af131f405f55416353aff4fcefe89783b97addc81f4a152b');
 assert.equal(JSON.parse(selected).length,203);const identities=new Map(inputs.files.map(r=>[r.file,r.sha256]));
 for(const row of read(temp+'/derived-originals/selection-info.json').inputs)assert.equal(identities.get(row.file),row.sha256);
 const key=r=>JSON.stringify([r.path,r.variant]);
 for(const mode of ['ast','auto']){
  const dirs=['before','after'].map(n=>temp+'/derived-originals/'+n+'-'+mode);
  const checks=dirs.map(d=>run('verify-test262-baseline.mjs',[d]));
  assert(checks.every(v=>v.files===203&&v.variants===401));
  assert.equal(checks[0].totals.pass,4);assert.equal(checks[0].totals.fail,397);assert.equal(checks[1].totals.pass,208);assert.equal(checks[1].totals.fail,193);
  for(const v of checks)for(const f of ['skip','timeout','not_run','unreported'])assert.equal(v.totals[f],0);
  const conditions=dirs.map(d=>read(d+'/conditions.json'));
  for(const f of ['corpus_revision','timeout_ms','memory_mb','stack_kb','batch_size','selection_sha256','universe_sha256'])assert.deepEqual(conditions[0][f],conditions[1][f]);
  assert.equal(conditions[1].engine_sha256,e.engine_sha256);
  const old=read(dirs[0]+'/outcomes.json'),next=new Map(read(dirs[1]+'/outcomes.json').map(r=>[key(r),r]));assert.equal(next.size,401);
  let improved=0;for(const row of old){const after=next.get(key(row));assert(after);if(row.outcome==='pass')assert.equal(after.outcome,'pass');if(row.outcome!==after.outcome){assert.equal(row.outcome,'fail');assert.equal(after.outcome,'pass');improved++;}}assert.equal(improved,204);
  const base=temp+'/base-originals/'+mode,v=run('verify-test262-baseline.mjs',[base]);assert.equal(v.files,149);assert.equal(v.variants,294);assert.equal(v.totals.pass,276);assert.equal(v.totals.fail,18);assert.equal(read(base+'/conditions.json').engine_sha256,e.engine_sha256);
  const known=new Map(read(temp+'/base-originals/expectations.json').known_failures.map(r=>[key(r),r]));
  for(const row of read(base+'/outcomes.json')){if(row.outcome==='pass')continue;assert.equal(row.outcome,'fail');const rule=known.get(key(row));assert(rule);const [file,pointer]=row.raw_record.split('#');assert.match(pointer,/^\/results\/\d+$/);const actual=read(path.join(base,file)).results[+pointer.split('/')[2]];assert.deepEqual(actual.observed_failure,rule.observed_failure);assert.equal(actual.diagnostic,rule.diagnostic);}
 }
 const rows=[...fs.readFileSync(temp+'/native-all-tests.log','utf8').matchAll(/test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored/g)];
 assert.equal(rows.length,97);assert.equal(rows.reduce((n,r)=>n+ +r[1],0),754);assert(rows.every(r=>+r[2]===0));assert.equal(rows.reduce((n,r)=>n+ +r[3],0),26);
 assert(fs.readFileSync(temp+'/native-derived14.log','utf8').includes('14 passed; 0 failed'));
 console.log(JSON.stringify({integrity:'pass',source:e.source_revision,performance:audit.reports.map(r=>r.outcomes),original_files:203,variants_per_backend:401,improvements_per_backend:204,known_pass_regressions:0,base_originals_pass:276,base_dependency_failures:18,scope:e.scope}));
} finally {fs.rmSync(temp,{recursive:true,force:true});}
