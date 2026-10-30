// Both machines and all unfavorable decisions remain part of this history.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..');
const e=JSON.parse(fs.readFileSync(path.join(root,'hosted-f1c/evidence.json')));
const archive=path.join(root,'hosted-f1c',e.archive.file),bytes=fs.readFileSync(archive);
assert.equal(bytes.length,e.archive.bytes);assert.equal(createHash('sha256').update(bytes).digest('hex'),e.archive.sha256);
const temp=fs.mkdtempSync(path.join(os.tmpdir(),'temiya-hosted-class-f1c-'));
try {
  const names=execFileSync('tar',['-tzf',archive],{encoding:'utf8'}).trim().split('\n');
  assert(names.every(n=>!path.isAbsolute(n)&&!n.split('/').some(p=>p==='..'||p==='.')));
  assert(execFileSync('tar',['-tvzf',archive],{encoding:'utf8'}).trim().split('\n').every(n=>n[0]==='d'||n[0]==='-'));
  execFileSync('tar',['-xzf',archive,'-C',temp]);
  const results=[];
  for(const copy of [1,2]) {
    const dir=path.join(temp,'copy-'+copy),audit=(script,files)=>JSON.parse(execFileSync(process.execPath,[path.join(repo,'scripts',script),...files.map(n=>path.join(dir,n))],{encoding:'utf8'}));
    const originals=audit('audit-original-workload-records.mjs',['beforeafter.json','boa.json','baseline-boa.json','same-binary-control.json']);
    assert.deepEqual(originals.reports.map(r=>r.outcomes),[
      {'within noise threshold':32,'after slower beyond threshold':1},
      {'Temiya faster beyond threshold':33},{'Temiya faster beyond threshold':33},{'within noise threshold':33},
    ]);
    const classes=audit('audit-class-workload-records.mjs',['class-beforeafter.json','class-boa.json','class-baseline-boa.json','class-same-binary.json']);
    assert.equal(classes.reports[1].outcomes['Temiya slower beyond threshold'],3);
    assert.deepEqual(classes.reports[3].outcomes,{'within noise threshold':12});
    const report=JSON.parse(fs.readFileSync(path.join(dir,'class-boa.json')));
    assert.equal(report.workers[0].revision,e.source_revision);
    for(const row of report.comparisons.filter(r=>r.workload==='class-method'))assert.equal(row.outcome,'Temiya slower beyond threshold');
    results.push({copy,originals:originals.reports.map(r=>r.outcomes),classes:classes.reports.map(r=>r.outcomes)});
  }
  console.log(JSON.stringify({integrity:'pass',source:e.source_revision,results,scope:e.scope}));
} finally {fs.rmSync(temp,{recursive:true,force:true});}
