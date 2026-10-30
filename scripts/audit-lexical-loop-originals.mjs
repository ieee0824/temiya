import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {fileURLToPath} from 'node:url';
assert.equal(process.argv.length, 3, 'usage: audit-lexical-loop-originals.mjs CAPTURE_DIR');
const root=path.resolve(process.argv[2]),read=p=>JSON.parse(fs.readFileSync(p));
const descriptor=read(root+'/selection-info.json');assert.equal(descriptor.files,385);
assert.equal(createHash('sha256').update(fs.readFileSync(root+'/selection.json')).digest('hex'),descriptor.selection_sha256);
const results=[];
function signature(dir,row) {
 const [file,pointer]=row.raw_record.split('#'); const match=/^\/results\/(\d+)$/.exec(pointer);assert(match);
 const actual=read(path.join(dir,file)).results[Number(match[1])];assert.equal(actual.path,row.path);assert.equal(actual.variant,row.variant);
 const fields=['status','observed_failure','diagnostic','detail','reason','async_outcome'];return Object.fromEntries(fields.map(f=>[f,actual[f]??null]));
}
for(const mode of ['ast','auto']){
 const dirs=['before','after'].map(v=>root+'/'+v+'-'+mode);
 for(const dir of dirs)execFileSync(process.execPath,[fileURLToPath(new URL('./verify-test262-baseline.mjs',import.meta.url)),dir]);
 const [oldBaseline,newBaseline]=dirs.map(d=>read(d+'/baseline.json'));const [oldRows,newRows]=dirs.map(d=>read(d+'/outcomes.json'));
 assert.deepEqual(read(dirs[0]+'/selection.json'),read(dirs[1]+'/selection.json'));assert.equal(oldRows.length,758);assert.equal(newRows.length,758);
 for(const key of ['timeout_ms','memory_mb','stack_kb','batch_size','corpus_revision','selection_sha256','universe_sha256'])assert.deepEqual(oldBaseline.conditions[key],newBaseline.conditions[key]);
 assert.equal(oldBaseline.conditions.engine_sha256,'062b7a9bbbde7f75615e81657d78c2457c3076422422da3b0bb797a0e1219342');
 assert.equal(newBaseline.conditions.engine_sha256,'26616f9e4b9b252b7adf8608fc69a4e2daf631451d865d82cca97c2100ab4f8d');
 const key=r=>JSON.stringify([r.path,r.variant]),next=new Map(newRows.map(r=>[key(r),r]));const changes=[],regressions=[],changedFailures=[];
 for(const old of oldRows){const after=next.get(key(old));assert(after);if(old.outcome!==after.outcome)changes.push({path:old.path,variant:old.variant,before:old.outcome,after:after.outcome});if(old.outcome==='pass'&&after.outcome!=='pass')regressions.push(changes.at(-1));
  if(old.outcome!=='pass'&&after.outcome!=='pass'&&JSON.stringify(signature(dirs[0],old))!==JSON.stringify(signature(dirs[1],after)))changedFailures.push({path:old.path,variant:old.variant,before:signature(dirs[0],old),after:signature(dirs[1],after)});
 }
 results.push({mode,before:oldBaseline.totals,after:newBaseline.totals,changes,regressions,changedFailures});
}
const result={schema:1,scope:descriptor.scope,selection_sha256:descriptor.selection_sha256,results};assert.deepEqual(read(root+'/comparison.json'),result);
console.log(JSON.stringify(results.map(({mode,before,after,changes,regressions,changedFailures})=>({mode,before,after,changes:changes.length,regressions:regressions.length,changed_failures:changedFailures.length})),null,2));
assert(results.every(r=>r.regressions.length===0&&r.changedFailures.length===0),'for-loop correctness regression or changed failure signature');
