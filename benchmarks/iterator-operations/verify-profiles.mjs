import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {gunzipSync} from 'node:zlib';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..'),sha=b=>createHash('sha256').update(b).digest('hex'),read=p=>JSON.parse(fs.readFileSync(p));
const captures=[['indexed-arguments-diagnostic-v1','74ff04d7d0c26c7baf589d9854c1bf271118e04a','c2bc478647815fdc013f254f5e1e72cfd804945b',882,[['call-spread',5286694951,343139030]]],['definition-cache-diagnostic-v1','09766bba7308eaede7b76059b88b73dd2bbb4efe','74ff04d7d0c26c7baf589d9854c1bf271118e04a',884,[['done-value-order',2990744792,1272405193],['invalid-next-result',1624065902,1148010885]]]];
for(const [name,source,baseline,native,profiles]of captures){
 const dir=root+'/'+name,e=read(dir+'/evidence.json');assert.equal(e.source,source);assert.equal(e.baseline,baseline);assert.equal(e.native,native);assert.deepEqual(e.profiles,profiles);
 for(const [file,hash]of Object.entries(e.files))assert.equal(sha(fs.readFileSync(dir+'/'+file)),hash,file);
 for(const [label,revision]of [['before',baseline],['after',source]]){const info=read(dir+'/'+label+'-worker-info.json');assert.equal(info.revision,revision);assert.equal(info.provenance.source_clean,true);assert.equal(info.provenance.source_inputs.length,80);for(const input of info.provenance.source_inputs)assert.equal(sha(execFileSync('git',['show',revision+':'+input.file],{cwd:repo})),input.sha256,input.file);}
 const tests=[...fs.readFileSync(dir+'/native-tests.log','utf8').matchAll(/test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored/g)];assert.equal(tests.reduce((n,r)=>n+Number(r[1]),0),native);assert(tests.every(r=>Number(r[2])===0));assert.equal(tests.reduce((n,r)=>n+Number(r[3]),0),26);assert.match(fs.readFileSync(dir+'/clippy.log','utf8'),/Finished `dev` profile/);
 for(const [workload,before,after]of profiles)for(const [label,count]of [['before',before],['after',after]]){const raw=gunzipSync(fs.readFileSync(dir+'/'+workload+'-'+label+'.callgrind.gz')).toString();assert.equal(Number(raw.match(/^summary: (\d+)$/m)?.[1]),count);assert.equal(Number(raw.match(/^totals: (\d+)$/m)?.[1]),count);const worker=read(dir+'/'+workload+'-'+label+'-worker.json');assert.equal(worker.expected,50005000);assert.equal(worker.validated_passes,2);}
 const oracles=fs.readdirSync(dir+'/oracles');assert.equal(oracles.length,6);for(const file of oracles){const r=read(dir+'/oracles/'+file);assert.equal(r.expected,50005000);assert.equal(r.validated_passes,2);}
 console.log(JSON.stringify({integrity:'pass',source,native,profiles,scope:e.scope}));
}
