import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..');
const dir=root+'/full-current-integrated-v1',read=p=>JSON.parse(fs.readFileSync(p));
const sha=b=>createHash('sha256').update(b).digest('hex'),e=read(dir+'/evidence.json');
assert.equal(e.source,'010ac60ddeb68ce702e1f3b2a4cfdd87345f2568');assert.equal(e.run,41207723463);
assert.equal(e.parent.source,'91fb9f4e516e12b738e729f3f6441920ca642c49');assert.equal(e.parent.run,41187181537);
assert.equal(e.comparison_basis,'previous fully audited array implementation, not immediate parent');
assert.equal(e.immediate_parent_source,'6c65967973ad4de72986db751d7c7e30547251c2');
assert.equal(e.immediate_parent.source,e.immediate_parent_source);assert.equal(e.immediate_parent.run,41207659226);
for(const [file,hash]of Object.entries(e.files)){
 assert(!path.isAbsolute(file)&&!file.split('/').includes('..'));
 assert.equal(sha(fs.readFileSync(dir+'/'+file)),hash,file);
}
const temp=fs.mkdtempSync(path.join(os.tmpdir(),'temiya-object-full-'));
try {
 const reference=temp+'/reference';
 execFileSync(process.execPath,[repo+'/scripts/verify-test262-archive.mjs',repo+'/benchmarks/test262-feature-baseline',reference],{cwd:repo});
 for(const [name,meta,policy]of [['candidate',e,'policy.json'],['parent',e.parent,'parent-policy.json'],['immediate-parent',e.immediate_parent,'immediate-parent-policy.json']]){
  assert(!path.isAbsolute(meta.archive.file)&&!meta.archive.file.includes('/'));
  const archive=dir+'/'+meta.archive.file,bytes=fs.readFileSync(archive);
  assert.equal(bytes.length,meta.archive.bytes);assert.equal(sha(bytes),meta.archive.sha256);
  const names=execFileSync('tar',['-tzf',archive],{encoding:'utf8',maxBuffer:16*1024*1024}).trim().split('\n');
  assert.equal(new Set(names).size,names.length);assert(names.every(n=>!path.isAbsolute(n)&&!n.split('/').some(p=>p==='..'||p==='.')));
  assert(execFileSync('tar',['-tvzf',archive],{encoding:'utf8',maxBuffer:16*1024*1024}).trim().split('\n').every(n=>n[0]==='d'||n[0]==='-'));
  const out=temp+'/'+name;fs.mkdirSync(out);
  execFileSync('tar',['-xzf',archive,'--no-same-owner','--no-same-permissions','-C',out]);
  const audit=JSON.parse(execFileSync(process.execPath,[repo+'/scripts/audit-full-candidate.mjs',out,reference,dir+'/'+policy,meta.source,String(meta.run)],{cwd:repo,encoding:'utf8',maxBuffer:1024*1024}));
  assert.equal(audit.files,52133);assert.equal(audit.variants,100211);assert.deepEqual(audit.failures,[]);assert.deepEqual(audit.totals,meta.totals);
  if(name==='candidate')assert.deepEqual(read(out+'/independent-candidate-audit.json'),read(dir+'/independent-audit.json'));
 }
 const outcomes=name=>{
  const rows=[];for(let i=0;i<8;i++)rows.push(...read(`${temp}/${name}/test262-full-${i}-of-8/test262-regression/capture/outcomes.json`));
  const map=new Map(rows.map(r=>[JSON.stringify([r.path,r.variant]),r]));assert.equal(map.size,100211);return map;
 };
 const after=outcomes('candidate');
 for(const previous of [{name:'parent',source:e.parent.source,file:'previous-qualified-array-comparison.json'},{name:'immediate-parent',source:e.immediate_parent.source,file:'immediate-parent-comparison.json'}]){
 const before=outcomes(previous.name),regressions=[],newpasses=[];let preserved=0;
 for(const [key,b]of before){
  const a=after.get(key);assert(a);
  if(b.outcome==='pass'){if(a.outcome==='pass')preserved++;else regressions.push({path:b.path,variant:b.variant,before:b.outcome,after:a.outcome});}
  else if(a.outcome==='pass')newpasses.push({path:b.path,variant:b.variant,before:b.outcome,after:a.outcome});
 }
 assert.equal(preserved,33565);assert.deepEqual(regressions,[]);assert.equal(newpasses.length,4354);
 assert.deepEqual({previous_source:previous.source,source:e.source,variants:after.size,preserved_passes:preserved,regressions,newpasses},read(dir+'/'+previous.file));
 console.log(JSON.stringify({integrity:'pass',comparison:previous.name,variants:100211,preserved_passes:preserved,newpasses:newpasses.length,regressions:0,scope:e.scope}));
 }
} finally {fs.rmSync(temp,{recursive:true,force:true});}
