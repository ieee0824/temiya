// Retain every identical-worker control outcome, including apparent slowdowns.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..');
const e=JSON.parse(fs.readFileSync(path.join(root,'native-loop-v1/evidence.json'))),a=e.originals;
assert.equal(a.file,'originals.tar.gz');
const archive=path.join(root,'native-loop-v1',a.file),bytes=fs.readFileSync(archive);
assert.equal(bytes.length,a.bytes);assert.equal(createHash('sha256').update(bytes).digest('hex'),a.sha256);
const temp=fs.mkdtempSync(path.join(os.tmpdir(),'temiya-native-loop-originals-'));
try {
 const names=execFileSync('tar',['-tzf',archive],{encoding:'utf8'}).trim().split('\n');
 assert.equal(new Set(names).size,names.length);assert(names.every(n=>/^[a-zA-Z0-9_.-]+$/.test(n)&&n!=='.'&&n!=='..'));
 assert(execFileSync('tar',['-tvzf',archive],{encoding:'utf8'}).trim().split('\n').every(n=>n[0]==='-'));
 execFileSync('tar',['-xzf',archive,'--no-same-owner','--no-same-permissions','-C',temp]);
 const reports=['beforeafter.json','boa.json','baseline-boa.json','same-binary.json'];
 const audit=JSON.parse(execFileSync(process.execPath,[path.join(repo,'scripts/audit-original-workload-records.mjs'),...reports.map(n=>path.join(temp,n))],{encoding:'utf8',maxBuffer:16*1024*1024}));
 assert.deepEqual(audit.reports.map(r=>r.outcomes),[
  {'within noise threshold':32,'after faster beyond threshold':1},
  {'Temiya faster beyond threshold':27,'within noise threshold':6},
  {'Temiya faster beyond threshold':30,'within noise threshold':3},
  {'within noise threshold':31,'after slower beyond threshold':2},
 ]);
 const read=n=>JSON.parse(fs.readFileSync(path.join(temp,n)));
 for(const name of ['beforeafter.json','boa.json','same-binary.json'])assert.equal(read(name).workers[0].revision,e.source_revision);
 const self=read('same-binary.json');assert.equal(self.workers[0].executable_sha256,self.workers[1].executable_sha256);
 assert.equal(self.workers[0].executable_sha256,'5bbae46cfe51aa142030c5ecc163f39cc6c680f957e742281832cb01d4bbbc9e');
 console.log(JSON.stringify({integrity:'pass',source:e.source_revision,outcomes:audit.reports.map(r=>r.outcomes),scope:a.scope}));
} finally {fs.rmSync(temp,{recursive:true,force:true});}
