import fs from 'node:fs';
import crypto from 'node:crypto';
import cp from 'node:child_process';
import zlib from 'node:zlib';
import assert from 'node:assert/strict';
const dir=new URL('./',import.meta.url),hash=b=>crypto.createHash('sha256').update(b).digest('hex');
const read=f=>JSON.parse(fs.readFileSync(new URL(f,dir)));
for(const [file,sha]of Object.entries(read('evidence.json').files)){assert.ok(!file.includes('/'));assert.equal(hash(fs.readFileSync(new URL(file,dir))),sha);}
const freeze=read('freeze.json'),summary=read('summary.json');assert.equal(summary.nativeQualification,false);
assert.equal(hash(fs.readFileSync(new URL('workload.json',dir))),freeze.sha256);assert.equal(summary.inputSha256,freeze.sha256);
const texts={};for(const name of ['before','after']){
 const unzip=suffix=>zlib.gunzipSync(fs.readFileSync(new URL(name+(suffix==='worker-info.json'?'-':'.')+suffix+'.gz',dir)));
 const info=JSON.parse(unzip('worker-info.json')),result=JSON.parse(unzip('stdout')),profile=unzip('callgrind').toString();
 assert.equal(result.expected,193);assert.equal(result.validated_passes,2);assert.equal(info.profile,'release');assert.equal(info.source_clean,true);assert.equal(info.source_inputs.length,84);
 for(const input of info.source_inputs)assert.equal(hash(cp.execFileSync('git',['show',info.source_revision+':'+input.path])),input.sha256);
 const row=summary.rows.find(r=>r.name===name);assert.equal(row.source,info.source_revision);assert.equal(row.instructions,Number(profile.match(/^summary: (\d+)/m)[1]));assert.equal(row.instructions,Number(profile.match(/^totals: (\d+)/m)[1]));texts[name]=new Map(info.source_inputs.map(i=>[i.path,i.sha256]));
}
assert.deepEqual([...texts.after].filter(([path,sha])=>texts.before.get(path)!==sha).map(([path])=>path),['src/runtime/function/parameter_environment.rs']);
assert.equal(summary.changePercent,100*(summary.rows[1].instructions/summary.rows[0].instructions-1));
console.log(JSON.stringify({integrity:'pass',changePercent:summary.changePercent,nativeQualification:false,scope:summary.scope}));
