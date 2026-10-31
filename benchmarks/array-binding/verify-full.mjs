import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..');
const read=p=>JSON.parse(fs.readFileSync(p));
const captures=[
 {name:'full-cache32-v1',source:'9f84bc5c6416911df51d76cf10b936ad3c3e003f',run:41234441785,parent:'1090d929ab73326097af7c5e0b8a9a685d761cd3',parentRun:41223794490,preserved:33565,added:0},
 {name:'full-array-data-properties-v1',source:'1090d929ab73326097af7c5e0b8a9a685d761cd3',run:41223794490,parent:'6c65967973ad4de72986db751d7c7e30547251c2',parentRun:41207659226,preserved:33565,added:0},
 {name:'full-initial-integrated-v1',source:'91fb9f4e516e12b738e729f3f6441920ca642c49',run:41187181537,parent:'4a4c103ade6b0acae5f5a16cfa68220f24c2300e',parentRun:41186658237,preserved:29701,added:3864},
 {name:'full-current-integrated-v1',source:'6c65967973ad4de72986db751d7c7e30547251c2',run:41207659226,parent:'91fb9f4e516e12b738e729f3f6441920ca642c49',parentRun:41187181537,preserved:33565,added:0},
];
for(const expected of captures){
const dir=root+'/'+expected.name;
const sha=b=>createHash('sha256').update(b).digest('hex'),e=read(dir+'/evidence.json');
assert.equal(e.source,expected.source);assert.equal(e.run,expected.run);
assert.equal(e.parent.source,expected.parent);assert.equal(e.parent.run,expected.parentRun);
if(expected.name==='full-current-integrated-v1'){
 assert.equal(e.parent.commit,'e53046edb33b5f7adb65db21607a79a48945e49a');
 assert.equal(execFileSync('git',['-c','safe.directory='+repo,'rev-parse',e.source+'^1'],{cwd:repo,encoding:'utf8'}).trim(),e.parent.commit);
 execFileSync('git',['-c','safe.directory='+repo,'diff','--exit-code',e.parent.source,e.parent.commit,'--','Cargo.toml','Cargo.lock','build.rs','src','examples/measure_workload.rs','test262/manifest.json'],{cwd:repo});
}
for(const [file,hash]of Object.entries(e.files)){
 assert(!path.isAbsolute(file)&&!file.split('/').includes('..'));
 assert.equal(sha(fs.readFileSync(dir+'/'+file)),hash,file);
}
const temp=fs.mkdtempSync(path.join(os.tmpdir(),'temiya-array-full-'));
try {
 const reference=temp+'/reference';
 execFileSync(process.execPath,[repo+'/scripts/verify-test262-archive.mjs',repo+'/benchmarks/test262-feature-baseline',reference],{cwd:repo});
 for(const [name,meta,policy]of [['candidate',e,'policy.json'],['parent',e.parent,'parent-policy.json']]){
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
 const before=outcomes('parent'),after=outcomes('candidate'),regressions=[],newpasses=[];let preserved=0;
 for(const [key,b]of before){
  const a=after.get(key);assert(a);
  if(b.outcome==='pass'){if(a.outcome==='pass')preserved++;else regressions.push({path:b.path,variant:b.variant,before:b.outcome,after:a.outcome});}
  else if(a.outcome==='pass')newpasses.push({path:b.path,variant:b.variant,before:b.outcome,after:a.outcome});
 }
 assert.equal(preserved,expected.preserved);assert.deepEqual(regressions,[]);assert.equal(newpasses.length,expected.added);
 assert.deepEqual({previous_source:e.parent.source,source:e.source,variants:after.size,preserved_passes:preserved,regressions,newpasses},read(dir+'/previous-parent-comparison.json'));
 console.log(JSON.stringify({integrity:'pass',variants:100211,preserved_passes:preserved,newpasses:newpasses.length,regressions:0,scope:e.scope}));
} finally {fs.rmSync(temp,{recursive:true,force:true});}

}
