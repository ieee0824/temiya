import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {gunzipSync} from 'node:zlib';
import {execFileSync} from 'node:child_process';
const root=path.dirname(new URL(import.meta.url).pathname);
const repo=execFileSync('git',['rev-parse','--show-toplevel'],{encoding:'utf8'}).trim();
const hash=b=>createHash('sha256').update(b).digest('hex');
const read=f=>JSON.parse(fs.readFileSync(path.join(root,f)));
for(const [file,sha] of Object.entries(read('evidence.json').files)){
  assert(!path.isAbsolute(file)&&!file.split('/').includes('..'));
  assert.equal(hash(fs.readFileSync(path.join(root,file))),sha,file);
}
let cases=0;
for(const label of ['candidate']){
  const summary=read(label+'/diagnostic-summary.json');
  assert.equal(summary.native_qualification,false);
  const worker=read(label+'/worker-info.json');
  assert.equal(worker.revision,summary.revision);
  for(const input of worker.provenance.source_inputs){
    assert.equal(hash(execFileSync('git',['-C',repo,'show',summary.revision+':'+input.file])),input.sha256,input.file);
  }
  assert.equal(summary.results.length,6);
  for(const row of summary.results){
    const count=dir=>{
      const raw=gunzipSync(fs.readFileSync(path.join(root,dir,row.name+'.callgrind.gz'))).toString();
      const total=Number(raw.match(/^totals: (\d+)/m)[1]);
      assert.equal(total,Number(raw.match(/^summary: (\d+)/m)[1]));
      return total;
    };
    assert.equal(row.before,count('baseline'));
    assert.equal(row.after,count(label));
    assert(Math.abs(row.changePercent-100*(row.after/row.before-1))<1e-10);
    const result=read(label+'/'+row.name+'.profile.json');
    assert.equal(result.expected,50005000);
    assert.equal(result.validated_passes,2);
    assert.equal(hash(fs.readFileSync(path.join(repo,'benchmarks/object-binding/workloads-v1',row.name+'.json'))),row.inputSha256);
    cases++;
  }
}
for(const label of ['baseline','candidate']) {
 const info=read(label+'/worker-info.json');
 assert.equal(info.revision,read('candidate/diagnostic-summary.json')[label==='baseline'?'baseline':'revision']);
 const files=execFileSync('git',['-C',repo,'ls-tree','-r','--name-only',info.revision],{encoding:'utf8'}).trim().split('\n').filter(p=>['Cargo.toml','Cargo.lock','build.rs','examples/measure_workload.rs'].includes(p)||(p.startsWith('src/')&&p.endsWith('.rs')));
 assert.deepEqual(info.provenance.source_inputs.map(i=>i.file),files);
 for(const input of info.provenance.source_inputs)assert.equal(hash(execFileSync('git',['-C',repo,'show',info.revision+':'+input.file])),input.sha256,input.file);
}
console.log(JSON.stringify({integrity:'pass',diagnostics:cases,nativeQualification:false}));
