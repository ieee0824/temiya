import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {gunzipSync} from 'node:zlib';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..');
const dir=root+'/initial-profiles-v1',read=p=>JSON.parse(fs.readFileSync(p));
const sha=b=>createHash('sha256').update(b).digest('hex'),e=read(dir+'/evidence.json');
assert.equal(e.source,'1076d17e57f8367c47c1017424fc885f9f14cfe6');
for(const [file,hash]of Object.entries(e.files)){
 assert.equal(path.basename(file),file);assert.equal(sha(fs.readFileSync(dir+'/'+file)),hash);
}
const info=read(dir+'/worker-info.json');assert.equal(info.revision,e.source);
assert.equal(info.profile,'release');assert.deepEqual(info.features,[]);assert.equal(info.provenance.source_clean,true);
const inputs=execFileSync('git',['-c','safe.directory='+repo,'ls-tree','-r','--name-only',e.source,'--','Cargo.lock','Cargo.toml','build.rs','examples/measure_workload.rs','src'],{cwd:repo,encoding:'utf8'}).trim().split('\n').filter(p=>p.endsWith('.rs')||p==='Cargo.lock'||p==='Cargo.toml');
assert.equal(inputs.length,82);assert.deepEqual(info.provenance.source_inputs.map(x=>x.file),inputs);
for(const input of info.provenance.source_inputs){
 assert.equal(sha(execFileSync('git',['-c','safe.directory='+repo,'show',e.source+':'+input.file],{cwd:repo})),input.sha256);
}
const freeze=read(root+'/workload-freeze.json');
assert.deepEqual(e.profiles.map(x=>'object-binding-'+x.name),freeze.inputs.map(x=>x.name));
for(const row of e.profiles){
 const raw=gunzipSync(fs.readFileSync(`${dir}/${row.name}.callgrind.gz`)).toString();
 assert.equal(Number(raw.match(/^summary: (\d+)$/m)?.[1]),row.instructions);
 assert.equal(Number(raw.match(/^totals: (\d+)$/m)?.[1]),row.instructions);
 const output=read(`${dir}/${row.name}-worker.json`);assert.equal(output.expected,50005000);assert.equal(output.validated_passes,2);
 assert.equal(row.expected,output.expected);assert.equal(row.validated_passes,output.validated_passes);
}
console.log(JSON.stringify({integrity:'pass',source:e.source,profiles:e.profiles,scope:e.scope}));
