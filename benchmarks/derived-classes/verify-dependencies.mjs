// Check that dependency explanations account for every recorded failure,
// without changing any outcome or claiming later assertions were executed.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),dir=root+'/dependencies-v1';
const read=p=>JSON.parse(fs.readFileSync(p)),sha=b=>createHash('sha256').update(b).digest('hex');
const audit=read(dir+'/audit.json'),provenance=read(dir+'/provenance.json');
assert.equal(audit.source_revision,'06ceeee645204673aa06f6c407fa2cfa9e536c17');
assert.equal(provenance.source_revision,audit.source_revision);
for(const row of provenance.files)assert.equal(sha(fs.readFileSync(dir+'/'+row.file)),row.sha256);
const recorded=fs.readFileSync(dir+'/probes.log','utf8').trim().split('\n');
assert.equal(recorded.length,44);assert.equal(audit.probes.length,44);
for(let i=0;i<recorded.length;i++){
 const [backend,name,observation,...source]=recorded[i].split('\t');
 assert.deepEqual(audit.probes[i],{backend,name,observation,source:source.join('\t')});assert.equal(observation,'Boolean(true)');
}
const temp=fs.mkdtempSync(path.join(os.tmpdir(),'temiya-derived-dependencies-'));
function unpack(name){
 const e=read(root+'/'+name+'/evidence.json'),archive=root+'/'+name+'/capture.tar.gz',out=temp+'/'+name;
 assert.equal(e.archive.file,'capture.tar.gz');const bytes=fs.readFileSync(archive);assert.equal(bytes.length,e.archive.bytes);assert.equal(sha(bytes),e.archive.sha256);
 const names=execFileSync('tar',['-tzf',archive],{encoding:'utf8'}).trim().split('\n');
 assert.equal(new Set(names).size,names.length);assert(names.every(n=>!path.isAbsolute(n)&&!n.split('/').some(p=>p==='..'||p==='.')));
 assert(execFileSync('tar',['-tvzf',archive],{encoding:'utf8'}).trim().split('\n').every(n=>n[0]==='d'||n[0]==='-'));
 fs.mkdirSync(out);execFileSync('tar',['-xzf',archive,'--no-same-owner','--no-same-permissions','-C',out]);return out;
}
const key=r=>JSON.stringify([r.path,r.variant]);
try {
 const native=unpack('native-prototypes-v1'),initial=unpack('initial-v1');
 const sources=new Map(read(initial+'/original-inputs.json').files.map(r=>[r.file,r]));
 const entries=new Map(audit.entries.map(r=>[key(r),r]));assert.equal(entries.size,187);assert.equal(audit.entries.length,187);
 for(const mode of ['ast','auto']){
  const capture=native+'/derived-'+mode,conditions=read(capture+'/conditions.json'),outcomes=read(capture+'/outcomes.json');
  assert.equal(conditions.engine_sha256,read(native+'/engine-info.json').executable_sha256);
  assert.equal(conditions.corpus_revision,audit.corpus_revision);assert.equal(conditions.selection_sha256,audit.selection_sha256);
  assert.equal(outcomes.length,401);assert.equal(outcomes.filter(r=>r.outcome==='pass').length,214);
  const failed=outcomes.filter(r=>r.outcome==='fail');assert.equal(failed.length,187);
  for(const row of failed){
   const e=entries.get(key(row));assert(e);assert.equal(e.later_behavior_unproven,true);
   assert(e.classification.issues.length>0&&e.classification.issues.every(n=>Number.isInteger(n)&&n!==155));assert(e.classification.reason.length>30);
   const source=sources.get(e.path);assert(source);assert.equal(e.source_sha256,source.sha256);assert.equal(sha(fs.readFileSync(initial+'/original-inputs/'+e.path)),e.source_sha256);
   if(e.classification.probe)assert(audit.probes.some(p=>p.name===e.classification.probe&&p.backend===(mode==='ast'?'Ast':'Auto')));
   const [file,pointer]=row.raw_record.split('#'),bytes=fs.readFileSync(capture+'/'+file),raw=JSON.parse(bytes).results[+pointer.split('/')[2]];
   assert.deepEqual(e.records[mode],{raw_record:row.raw_record,raw_file_sha256:sha(bytes),diagnostic:raw.diagnostic,observed_failure:raw.observed_failure,detail:raw.detail});
  }
 }
 console.log(JSON.stringify({integrity:'pass',files:new Set(audit.entries.map(r=>r.path)).size,failures_per_backend:entries.size,dependency_probes:audit.probes.length,waived_failures:0,scope:audit.scope}));
} finally {fs.rmSync(temp,{recursive:true,force:true});}
