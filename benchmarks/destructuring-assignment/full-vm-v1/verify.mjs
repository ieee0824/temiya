import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const dir=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(dir,'../../..'),read=p=>JSON.parse(fs.readFileSync(p));
const sha=b=>createHash('sha256').update(b).digest('hex'),meta=read(dir+'/evidence.json');
assert.equal(meta.source,'eb29c4169c0fdfd9246546f76b5874097a53ef62');assert.equal(meta.run,41303173669);
const temp=fs.mkdtempSync(path.join(os.tmpdir(),'temiya-assignment-full-'));
try {
 const reference=temp+'/reference';execFileSync(process.execPath,[repo+'/scripts/verify-test262-archive.mjs',repo+'/benchmarks/test262-feature-baseline',reference],{cwd:repo});
 const maps=new Map();let candidateCatalog=[];
 for(const [name,base]of [['candidate',dir],['initial',path.resolve(dir,'../full-initial-v1')],['main',repo+'/benchmarks/object-binding/full-prepared-guards-v1']]){
  const e=read(base+'/evidence.json');for(const [file,hash]of Object.entries(e.files)){assert.equal(path.basename(file),file);assert.equal(sha(fs.readFileSync(base+'/'+file)),hash);}
  const archive=base+'/capture.tar.gz';const names=execFileSync('tar',['-tzf',archive],{encoding:'utf8',maxBuffer:16*1024*1024}).trim().split('\n');assert.equal(new Set(names).size,names.length);assert(names.every(n=>!path.isAbsolute(n)&&!n.split('/').includes('..')));assert(execFileSync('tar',['-tvzf',archive],{encoding:'utf8',maxBuffer:16*1024*1024}).trim().split('\n').every(n=>['d','-'].includes(n[0])));
  const out=temp+'/'+name;fs.mkdirSync(out);execFileSync('tar',['-xzf',archive,'--no-same-owner','--no-same-permissions','-C',out]);
  const result=JSON.parse(execFileSync(process.execPath,[repo+'/scripts/audit-full-candidate.mjs',out,reference,base+'/policy.json',e.source,String(e.run)],{cwd:repo,encoding:'utf8',maxBuffer:1024*1024}));assert.equal(result.variants,100211);assert.deepEqual(result.failures,[]);assert.deepEqual(read(out+'/independent-candidate-audit.json'),read(base+'/independent-candidate-audit.json'));
  const map=new Map();for(let i=0;i<8;i++)for(const r of read(`${out}/test262-full-${i}-of-8/test262-regression/capture/outcomes.json`)){const key=JSON.stringify([r.path,r.variant]);assert(!map.has(key));map.set(key,r.outcome);}assert.equal(map.size,100211);maps.set(name,map);if(name==='candidate')for(let i=0;i<8;i++)candidateCatalog.push(...read(`${out}/test262-full-${i}-of-8/test262-regression/capture/catalog.json`));
 }
 const selection=read(repo+'/benchmarks/destructuring-assignment/selection.json'),selectionBytes=fs.readFileSync(repo+'/benchmarks/destructuring-assignment/selection.json'),summary=read(dir+'/selection-summary.json');
 assert.equal(sha(selectionBytes),summary.selectionSha256);assert.equal(summary.source,meta.source);assert.equal(summary.run,meta.run);
 const tagged=candidateCatalog.filter(c=>c.features.includes('destructuring-assignment')).map(c=>c.path).sort();
 const textOnly=['test/language/expressions/assignment/destructuring/obj-prop-__proto__dup.js'];assert.deepEqual(summary.textOnly,textOnly);
 const textPaths=[...new Set([...tagged,...textOnly])].sort();assert.deepEqual(summary.frozenTextMatches.paths,textPaths);assert.equal(textPaths.length,142);assert(textPaths.every(p=>selection.includes(p)));
 function group(paths){const set=new Set(paths),totals={},seenFiles=new Set();let variants=0;for(const [key,outcome]of maps.get('candidate')){const [p]=JSON.parse(key);if(set.has(p)){variants++;seenFiles.add(p);totals[outcome]=(totals[outcome]??0)+1;}}assert.equal(seenFiles.size,set.size);return{files:paths.length,variants,totals};}
 assert.deepEqual(summary.selection,group(selection));assert.deepEqual(summary.metadataTagged,group(tagged));assert.deepEqual(summary.frozenTextMatches,{...group(textPaths),paths:textPaths});
 const failures=[];for(const [key,outcome]of maps.get('candidate')){const [p,variant]=JSON.parse(key);if(textPaths.includes(p)&&outcome!=='pass')failures.push({path:p,variant,outcome,dependency:171});}assert.deepEqual(summary.dependencyFailures,failures);assert.equal(failures.length,2);assert(failures.every(r=>r.path==='test/language/statements/for-of/body-dstr-assign-error.js'&&r.outcome==='fail'));
 console.log(JSON.stringify({selection:summary.selection,assignmentTextMatches:{files:142,variants:219,pass:217,dependencyFailures:2},scope:summary.scope}));
 const next=maps.get('candidate'),saved=read(dir+'/initial-assignment-comparison.json');assert.equal(saved.complete,true);assert.equal(saved.coveredVariants,100211);assert.equal(saved.sources.after,meta.source);assert.equal(saved.sources.before,'d460123c29dfd7a250b390e9c9464552f7b330db');
 for(const [name,expected]of [['initial',38758],['main',37919]]){
  let preserved=0;const regressions=[],newPasses=[];for(const [key,old]of maps.get(name)){assert(next.has(key));const after=next.get(key),[p,variant]=JSON.parse(key);if(old==='pass'){if(after==='pass')preserved++;else regressions.push({path:p,variant,before:old,after});}else if(after==='pass')newPasses.push({path:p,variant,before:old,after});}
  assert.equal(preserved,expected);assert.deepEqual(regressions,[]);if(name==='initial'){assert.equal(saved.preservedPasses,preserved);assert.deepEqual(saved.regressions,regressions);assert.deepEqual(saved.newPasses,newPasses);}
  console.log(JSON.stringify({integrity:'pass',comparison:name,preserved,newPasses:newPasses.length,regressions:0,scope:meta.scope}));
 }
} finally { fs.rmSync(temp,{recursive:true,force:true}); }
