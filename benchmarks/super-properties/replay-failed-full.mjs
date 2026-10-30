// Replay the exact historical namespace regression; never turn its red gate green.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
assert.equal(process.argv.length,7,'usage: audit-full-candidate.mjs CAPTURE_ROOT REFERENCE_DIR POLICY SOURCE_SHA RUN_ID');
const policyFile=process.argv[4],source=process.argv[5],run=Number(process.argv[6]);
assert.match(source,/^[a-f0-9]{40}$/);assert(Number.isSafeInteger(run)&&run>0);
const root=path.resolve(process.argv[2]),referenceRoot=path.resolve(process.argv[3]),files=new Set(),variants=new Set(),results=[],totals={};
for(let i=0;i<8;i++){
 const artifact=path.join(root,`test262-full-${i}-of-8`),capture=path.join(artifact,'test262-regression/capture');
 const verification=JSON.parse(execFileSync(process.execPath,['scripts/verify-test262-baseline.mjs',capture],{encoding:'utf8'}));
 const baseline=JSON.parse(fs.readFileSync(capture+'/baseline.json')),info=JSON.parse(fs.readFileSync(artifact+'/test262-build-info.json'));
 assert.equal(info.source_revision,source);assert.equal(info.source_clean,true);
 assert.equal(info.executable_sha256,baseline.conditions.engine_sha256);
 const gate=JSON.parse(fs.readFileSync(artifact+'/test262-regression/check-test262-regressions.mjs.stdout'));
 assert.equal(gate.mode,'full');assert.equal(gate.shard,i);assert.equal(gate.shards,8);
 const checked=await import('node:child_process').then(({spawnSync})=>spawnSync(process.execPath,['scripts/check-test262-regressions.mjs','--full','--shard',i+'/8',referenceRoot,capture,policyFile],{encoding:'utf8',maxBuffer:16*1024*1024}));
 assert.equal(checked.status,gate.passed?0:1,checked.stderr);
 assert.deepEqual(JSON.parse(checked.stdout),gate);
 for(const p of JSON.parse(fs.readFileSync(capture+'/selection.json'))){assert(!files.has(p));files.add(p);}
 for(const row of JSON.parse(fs.readFileSync(capture+'/outcomes.json'))){const key=JSON.stringify([row.path,row.variant]);assert(!variants.has(key));variants.add(key);}
 for(const[k,v]of Object.entries(baseline.totals))if(Number.isInteger(v)&&!k.includes('rate'))totals[k]=(totals[k]??0)+v;
 results.push({shard:i,verification,engine:info,gate_passed:gate.passed,failures:gate.failures,transitions:gate.transitions.length});
}
const reference=JSON.parse(fs.readFileSync(path.join(referenceRoot,'universe.json')));assert.equal(reference.length,52133);assert.deepEqual([...files].sort(),reference);
assert.equal(variants.size,100211);
const output={source:source,run,files:files.size,variants:variants.size,totals,results};fs.writeFileSync(root+'/independent-candidate-audit.json',JSON.stringify(output,null,2)+'\n');
assert.deepEqual(results.flatMap(r=>r.failures),[{path:'test/language/module-code/namespace/internals/has-property-str-found-uninit.js',variant:'Module',reason:'known pass regressed to fail'}]);assert.equal(results.filter(r=>r.gate_passed).length,7);
console.log(JSON.stringify({files:files.size,variants:variants.size,totals,failures:results.flatMap(r=>r.failures),capture_integrity:'all8pass',gate:'historical failure retained:7 gates passed, namespace membership regression in shard3'},null,2));
