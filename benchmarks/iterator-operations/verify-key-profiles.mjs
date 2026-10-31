import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {fileURLToPath} from 'node:url';
import {gunzipSync} from 'node:zlib';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..');
const dir=root+'/key-and-result-diagnostic-v1',read=p=>JSON.parse(fs.readFileSync(p)),sha=b=>createHash('sha256').update(b).digest('hex');
const e=read(dir+'/evidence.json');
assert.equal(e.nodes.length,13);
for(const [file,hash]of Object.entries(e.files)){
 assert(!path.isAbsolute(file)&&!file.split('/').includes('..'));
 assert.equal(sha(fs.readFileSync(dir+'/'+file)),hash,file);
}
for(const node of e.nodes){
 const info=read(dir+'/'+node.name+'/worker-info.json');assert.equal(info.revision,node.source);assert.equal(info.profile,'release');assert.deepEqual(info.features,[]);assert.equal(info.provenance.source_clean,true);assert.equal(info.provenance.source_inputs.length,['short-functions','metadata-observers'].includes(node.name)?82:['empty-frame','error-strings','activation-fingerprints','inline-stacks','inline-auxiliary'].includes(node.name)?81:80);
 for(const input of info.provenance.source_inputs)assert.equal(sha(execFileSync('git',['-c','safe.directory='+repo,'show',node.source+':'+input.file],{cwd:repo})),input.sha256,input.file);
 assert.equal(node.profiles.length,({'definition-cache':3,'property-keys':3,'iterator-results':3,'environment-roots':3,'native-errors':1,'native-step':6,'empty-frame':6,'error-strings':3,'activation-fingerprints':6,'inline-stacks':6,'inline-auxiliary':6,'short-functions':6,'metadata-observers':6})[node.name]);
 for(const p of node.profiles){
  const base=dir+'/'+node.name+'/'+p.name,raw=gunzipSync(fs.readFileSync(base+'.callgrind.gz')).toString();
  assert.equal(Number(raw.match(/^summary: (\d+)$/m)?.[1]),p.instructions);
  assert.equal(Number(raw.match(/^totals: (\d+)$/m)?.[1]),p.instructions);
  const output=read(base+'-worker.json');assert.equal(output.expected,50005000);assert.equal(output.validated_passes,2);
 }
}
console.log(JSON.stringify({integrity:'pass',nodes:e.nodes,scope:e.scope}));
