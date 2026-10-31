import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..');
const read=p=>JSON.parse(fs.readFileSync(p)),sha=b=>createHash('sha256').update(b).digest('hex');
for(const [name,source]of [['fixed-hot-sites-v1','fdecf72f1dc2c6fc5a23e47a4ce994993da82427'],['fixed-direct-region-v1','9835c41e3ba81cc8dd068f32e98318c586511fe0'],['fixed-literal-accessors-v1','0e9c642fcc48fc9e1a3f870dda7482e7a220af27'],['fixed-inline-auxiliary-v1','d9906db3c26e0b6b3663206fe0617fa5dbd7bbfc'],['fixed-empty-frame-v1','f678ea70d4a5ab38703d6956a88f34151025c766'],['fixed-initial-v1','c2bc478647815fdc013f254f5e1e72cfd804945b'],['fixed-definition-cache-v1','09766bba7308eaede7b76059b88b73dd2bbb4efe']]) {
const dir=root+'/'+name;
const e=read(dir+'/evidence.json'),comparison=read(dir+'/comparison.json'),freeze=read(root+'/selection-freeze.json');
assert.equal(e.source,source);assert.equal(comparison.source,source);assert.equal(comparison.baseline_source,e.baseline_source);
if(name==='fixed-empty-frame-v1'){assert.equal(e.runtime_source,'4a4c103ade6b0acae5f5a16cfa68220f24c2300e');execFileSync('git',['-c','safe.directory='+repo,'diff','--exit-code',e.runtime_source,e.source,'--','Cargo.toml','Cargo.lock','build.rs','src','examples/measure_workload.rs','test262/manifest.json'],{cwd:repo});}assert.equal(e.baseline_source,'622bc975af697c49298d2fb5b71b575aeed6cdb5');
assert.equal(sha(fs.readFileSync(root+'/selection.json')),freeze.selection_sha256);
const bytes=fs.readFileSync(dir+'/capture.tar.gz');assert.equal(bytes.length,e.archive.bytes);assert.equal(sha(bytes),e.archive.sha256);
const temp=fs.mkdtempSync(path.join(os.tmpdir(),'temiya-iterator-fixed-')),opts={encoding:'utf8',maxBuffer:16*1024*1024};
try {
 const names=execFileSync('tar',['-tzf',dir+'/capture.tar.gz'],opts).trim().split('\n');assert.equal(names.length,new Set(names).size);assert(names.every(n=>!path.isAbsolute(n)&&!n.split('/').some(p=>p==='.'||p==='..')));
 assert(execFileSync('tar',['-tvzf',dir+'/capture.tar.gz'],opts).trim().split('\n').every(n=>n[0]==='d'||n[0]==='-'));
 execFileSync('tar',['-xzf',dir+'/capture.tar.gz','--no-same-owner','--no-same-permissions','-C',temp]);
 const rows={},totals={};
 for(const label of ['before','after']){rows[label]={};totals[label]={};for(const mode of ['ast','auto']){
  const capture=temp+'/'+label+'-'+mode,v=JSON.parse(execFileSync(process.execPath,[repo+'/scripts/verify-test262-baseline.mjs',capture],{...opts,cwd:repo})),c=read(capture+'/conditions.json');
  assert.equal(v.files,1947);assert.equal(v.variants,3880);assert.equal(v.corpus_revision,freeze.corpus_revision);
  assert.equal(c.backend_requested,mode);assert.equal(c.timeout_ms,1000);assert.equal(c.memory_mb,512);assert.equal(c.stack_kb,8192);
  assert.equal(c.provided_engine_build_info.source_revision,label==='before'?e.baseline_source:e.source);assert.equal(c.engine_sha256,c.provided_engine_build_info.executable_sha256);assert.equal(c.engine_frozen_during_run,true);
  assert.deepEqual(read(capture+'/selection.json'),read(root+'/selection.json'));
  const expected=label==='before'&&mode==='auto'?{pass:194,fail:1894,timeout:1792}:{pass:235,fail:1983,timeout:1662};
  for(const [k,n]of Object.entries({...expected,skip:0,not_run:0,unreported:0}))assert.equal(v.totals[k],n);
  if(label==='before'&&mode==='auto')assert.equal(v.recovered_collection_errors,1);
  rows[label][mode]=read(capture+'/outcomes.json');totals[label][mode]=v.totals;
 }}
 assert.deepEqual(totals,comparison.totals);
 const key=r=>[r.path,r.variant],comparisons=[];
 for(const mode of ['ast','auto']){const a=rows.before[mode],b=rows.after[mode];assert.deepEqual(a.map(key),b.map(key));const regressions=a.filter((r,i)=>r.outcome==='pass'&&b[i].outcome!=='pass');assert.deepEqual(regressions,[]);comparisons.push({backend:mode,preserved:a.filter((r,i)=>r.outcome==='pass'&&b[i].outcome==='pass').length,added:a.filter((r,i)=>r.outcome!=='pass'&&b[i].outcome==='pass').length,regressions,different_outcomes:a.filter((r,i)=>r.outcome!==b[i].outcome).length});}
 assert.deepEqual(comparisons,comparison.comparisons);const differences={};for(const label of ['before','after']){const a=rows[label].ast,b=rows[label].auto;assert.deepEqual(a.map(key),b.map(key));differences[label]=a.filter((r,i)=>r.outcome!==b[i].outcome).length;}assert.deepEqual(differences,comparison.ast_auto_different_outcomes);assert.equal(differences.after,0);
 console.log(JSON.stringify({integrity:'pass',source:e.source,files:1947,variants:3880,comparisons,scope:e.scope}));
} finally {fs.rmSync(temp,{recursive:true,force:true});}

}
