import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
assert.equal(process.argv.length,4,'usage: audit-full-f1c.mjs CAPTURE_ROOT REFERENCE_DIR');
const root=path.resolve(process.argv[2]),referenceRoot=path.resolve(process.argv[3]),files=new Set(),variants=new Set(),results=[],totals={};
for(let i=0;i<8;i++){
 const artifact=path.join(root,`test262-full-${i}-of-8`),capture=path.join(artifact,'test262-regression/capture');
 const verification=JSON.parse(execFileSync(process.execPath,['scripts/verify-test262-baseline.mjs',capture],{encoding:'utf8'}));
 const baseline=JSON.parse(fs.readFileSync(capture+'/baseline.json')),info=JSON.parse(fs.readFileSync(artifact+'/test262-build-info.json'));
 assert.equal(info.source_revision,'f1c2734883a62ceda9784cc0aab8f31f21836b91');assert.equal(info.source_clean,true);
 assert.equal(info.executable_sha256,baseline.conditions.engine_sha256);
 const gate=JSON.parse(fs.readFileSync(artifact+'/test262-regression/check-test262-regressions.mjs.stdout'));
 assert.equal(gate.mode,'full');assert.equal(gate.shard,i);assert.equal(gate.shards,8);
 const checked=await import('node:child_process').then(({spawnSync})=>spawnSync(process.execPath,['scripts/check-test262-regressions.mjs','--full','--shard',i+'/8',referenceRoot,capture,path.join(root,'policy.json')],{encoding:'utf8',maxBuffer:16*1024*1024}));
 assert.equal(checked.status,i===0?1:0,checked.stderr);
 assert.deepEqual(JSON.parse(checked.stdout),gate);
 for(const p of JSON.parse(fs.readFileSync(capture+'/selection.json'))){assert(!files.has(p));files.add(p);}
 for(const row of JSON.parse(fs.readFileSync(capture+'/outcomes.json'))){const key=JSON.stringify([row.path,row.variant]);assert(!variants.has(key));variants.add(key);}
 for(const[k,v]of Object.entries(baseline.totals))if(Number.isInteger(v)&&!k.includes('rate'))totals[k]=(totals[k]??0)+v;
 results.push({shard:i,verification,engine:info,gate_passed:gate.passed,failures:gate.failures,transitions:gate.transitions.length});
}
const reference=JSON.parse(fs.readFileSync(path.join(referenceRoot,'universe.json')));assert.equal(reference.length,52133);assert.deepEqual([...files].sort(),reference);
assert.equal(variants.size,100211);
const output={source:'f1c2734883a62ceda9784cc0aab8f31f21836b91',run:40960620020,files:files.size,variants:variants.size,totals,results};assert.deepEqual(JSON.parse(fs.readFileSync(root+'/independent-audit.json')),output);
assert.deepEqual(results.flatMap(r=>r.failures),[{path:'test/language/statements/function/S13.2.2_A12.js',variant:'Sloppy',reason:'known pass regressed to fail'}]);
console.log(JSON.stringify({files:files.size,variants:variants.size,totals,failures:results.flatMap(r=>r.failures),capture_integrity:'all8pass',gate:'one known-pass regression; not accepted'},null,2));
