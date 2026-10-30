// The historical full gate rejected one real regression; never relabel it pass.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..');
const e=JSON.parse(fs.readFileSync(path.join(root,'full-f1c/evidence.json')));
const archive=path.join(root,'full-f1c',e.archive.file),bytes=fs.readFileSync(archive);
assert.equal(bytes.length,e.archive.bytes);assert.equal(createHash('sha256').update(bytes).digest('hex'),e.archive.sha256);
const temp=fs.mkdtempSync(path.join(os.tmpdir(),'temiya-full-class-f1c-'));
try {
  const names=execFileSync('tar',['-tzf',archive],{encoding:'utf8',maxBuffer:16*1024*1024}).trim().split('\n');
  assert(names.every(n=>!path.isAbsolute(n)&&!n.split('/').some(p=>p==='..'||p==='.')));
  assert(execFileSync('tar',['-tvzf',archive],{encoding:'utf8',maxBuffer:16*1024*1024}).trim().split('\n').every(n=>n[0]==='d'||n[0]==='-'));
  execFileSync('tar',['-xzf',archive,'-C',temp]);
  for(const file of ['independent-audit.json','policy.json'])fs.copyFileSync(path.join(root,'full-f1c',file),path.join(temp,file));
  const reference=path.join(temp,'reference');
  execFileSync(process.execPath,[path.join(repo,'scripts/verify-test262-archive.mjs'),path.join(repo,'benchmarks/test262-feature-baseline'),reference],{stdio:'pipe'});
  const result=JSON.parse(execFileSync(process.execPath,[path.join(repo,'scripts/audit-full-f1c.mjs'),temp,reference],{encoding:'utf8',cwd:repo,maxBuffer:1024*1024}));
  assert.equal(result.files,52133);assert.equal(result.variants,100211);
  assert.equal(result.failures.length,1);assert.equal(result.gate,'one known-pass regression; not accepted');
  console.log(JSON.stringify({integrity:'pass',source:e.source_revision,files:result.files,variants:result.variants,totals:result.totals,knownpass_regressions:result.failures,scope:e.scope}));
} finally {fs.rmSync(temp,{recursive:true,force:true});}
