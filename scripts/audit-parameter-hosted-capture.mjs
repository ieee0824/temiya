// Replay all suites, including adverse speed observations, before qualification.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {verifyParameterCorrectionReceipt} from './omoikane-parameter-correction.mjs';

const [directory,source,baselineRevision] = process.argv.slice(2);
assert(directory && /^[a-f0-9]{40}$/.test(source ?? '') && /^[a-f0-9]{40}$/.test(baselineRevision ?? ''));
const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const read = name => JSON.parse(fs.readFileSync(path.join(directory, name + '.json')));
const run = (script,args) => JSON.parse(execFileSync(process.execPath,
  [path.join(repo,'scripts',script),...args], {cwd:repo,encoding:'utf8',maxBuffer:16*1024*1024}));
const identity = run('audit-omoikane-hosted-identities.mjs',[directory,source,baselineRevision]);
const baseline = read('baseline-build-info'), original = read('boa-build-info');
const corrected = read('parameter-boa-build-info');
verifyParameterCorrectionReceipt(corrected);
assert.equal(corrected.rustc,identity.compiler);
assert.equal(corrected.worker_source_sha256,original.worker_source_sha256);
assert.equal(corrected.original_lock_sha256,original.original_lock_sha256);
assert.equal(corrected.adapter_lock_sha256,original.adapter_lock_sha256);
assert.deepEqual(corrected.features,original.features);
const suites = [
  ['original',null,11],['class',null,4],['derived','--derived',5],['super','--super',5],
  ['fields','--fields',5],['static','--static',5],['private','--private',5],
  ['elements','--elements',6],['object-bindings','--object-bindings',6],
  ['bindings','--bindings',6],['iterator','--iterators',6],
  ['assignments','--assignments',6],['parameters','--parameters',8],
];
const direct = new Set(['elements','object-bindings','bindings','assignments','parameters']);
const groups = [];
for (const [name,flag,count] of suites) {
  const reports = direct.has(name) ? [name+'-boa',name+'-same-binary'] : name==='original'
    ? ['beforeafter','boa','baseline-boa','same-binary-control']
    : ['beforeafter','boa','baseline-boa','same-binary'].map(s=>name+'-'+s);
  const audit = run(name==='original'?'audit-original-workload-records.mjs':'audit-class-workload-records.mjs',
    [...(flag?[flag]:[]),...reports.map(n=>path.join(directory,n+'.json'))]);
  for (const [i,reportName] of reports.entries()) {
    const r = read(reportName), isBaseline = !direct.has(name) && i===2;
    assert.equal(r.rustc,identity.compiler);
    for (const key of ['platform','release','architecture','cpu_models','logical_cpus','cpu_affinity']) {
      assert.deepEqual(r.machine[key],identity.machine[key],key);
    }
    assert.equal(r.all_workloads_valid,true);
    assert.equal(r.workers[0].revision,isBaseline?baselineRevision:source);
    assert.equal(r.workers[0].executable_sha256,isBaseline?baseline.executable_sha256:identity.same_binary_hash);
    assert.equal(r.workers[0].profile,'release');
    assert.equal(r.workers[0].mode,'Auto Script');
    assert.deepEqual(r.workers[0].features,[]);
    const other = r.workers[1];
    if (reportName.endsWith('-boa') || reportName==='boa') {
      assert.deepEqual(other.build,name==='parameters'?corrected:original);
    } else {
      const before = !direct.has(name) && i===0;
      assert.equal(other.revision,before?baselineRevision:source);
      assert.equal(other.executable_sha256,before?baseline.executable_sha256:identity.same_binary_hash);
      assert.equal(other.profile,'release');assert.equal(other.mode,'Auto Script');
      assert.deepEqual(other.features,[]);
    }
  }
  const report = read(name==='original'?'boa':name+'-boa'), wins = {};
  for (const c of report.comparisons) wins[c.workload]=(wins[c.workload]??0)+(c.outcome==='Temiya faster beyond threshold'?1:0);
  assert.equal(Object.keys(wins).length,count);
  groups.push({name,reports:audit.reports,boa_wins_by_workload:wins,
    minimum_two_of_three:Object.values(wins).every(n=>n>=2)});
}
assert.equal(groups.reduce((n,g)=>n+Object.keys(g.boa_wins_by_workload).length,0),78);
console.log(JSON.stringify({integrity:'pass',source,baselineRevision,machine:identity.machine,
  reports:groups.reduce((n,g)=>n+g.reports.length,0),groups,
  all78MinimumTwoOfThree:groups.every(g=>g.minimum_two_of_three),
  referenceScope:'70 original pinned Boa; 8 explicitly corrected pinned Boa; no stock Boa claim'},null,2));
