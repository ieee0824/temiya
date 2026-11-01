import fs from 'node:fs';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {gunzipSync} from 'node:zlib';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
const dir=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(dir,'../../..');
const read=p=>JSON.parse(fs.readFileSync(p)),sha=b=>createHash('sha256').update(b).digest('hex');
const evidence=read(dir+'/evidence.json'),freeze=read(dir+'/../workload-freeze.json');
for(const [file,hash] of Object.entries(evidence.files)){
 assert(!path.isAbsolute(file)&&!file.split('/').includes('..'));
 assert.equal(sha(fs.readFileSync(dir+'/'+file)),hash);
}
const stages=['inline-parent','direct-get','extra-state','name-visitor','fresh-array'],summaries=[];
for(const stage of stages){
 const base=dir+'/'+stage,summary=read(base+'/summary.json'),info=read(base+'/worker-info.json');
 assert.equal(info.source_revision,summary.source);assert.equal(info.source_clean,true);assert.equal(info.profile,'release');
 const inputs=execFileSync('git',['ls-tree','-r','--name-only',summary.source,'--','Cargo.lock','Cargo.toml','build.rs','examples/measure_workload.rs','src'],{cwd:repo,encoding:'utf8'}).trim().split('\n').filter(p=>p.endsWith('.rs')||p==='Cargo.lock'||p==='Cargo.toml');
 assert.equal(inputs.length,83);assert.deepEqual(info.source_inputs.map(x=>x.path),inputs);
 for(const input of info.source_inputs)assert.equal(sha(execFileSync('git',['show',summary.source+':'+input.path],{cwd:repo})),input.sha256);
 assert.deepEqual(summary.rows.map(x=>x.name),freeze.inputs.map(x=>x.name));
 for(const [index,row] of summary.rows.entries()){
  assert.equal(row.inputSha256,freeze.inputs[index].sha256);
  const raw=gunzipSync(fs.readFileSync(`${base}/${row.name}.callgrind.gz`)).toString();
  for(const label of ['summary','totals'])assert.equal(Number(raw.match(new RegExp('^'+label+': (\\d+)$','m'))?.[1]),row.instructions);
  const result=read(`${base}/${row.name}.stdout`);assert.equal(result.expected,50005000);assert.equal(result.validated_passes,2);
 }
 summaries.push(summary);
}
console.log(JSON.stringify({integrity:'pass',scope:evidence.scope,comparisons:summaries[0].rows.map((r,i)=>({name:r.name,instructions:summaries.map(s=>s.rows[i].instructions)}))}));
