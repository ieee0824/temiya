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
    assert.equal(hash(fs.readFileSync(path.join(repo,'benchmarks/array-binding/workloads-v1',row.name+'.json'))),row.inputSha256);
    cases++;
  }
}
const layouts = ['before','after'].map(label => gunzipSync(fs.readFileSync(path.join(root,'layout-'+label+'.log.gz'))).toString());
for (const [type, before, after] of [['runtime::ObjectData',456,400],['runtime::object::ObjectState',368,312]]) {
  layouts.forEach((raw,index) => assert(raw.includes('type: '+String.fromCharCode(96)+type+String.fromCharCode(96)+': '+[before,after][index]+' bytes')));
}
const miri = gunzipSync(fs.readFileSync(path.join(root,'miri.log.gz'))).toString();
assert(miri.includes('test result: ok. 1 passed; 0 failed;'));
assert(miri.includes('test result: ok. 6 passed; 0 failed;'));
console.log(JSON.stringify({integrity:'pass',diagnostics:cases,nativeQualification:false}));
