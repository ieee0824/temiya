// Retain every failure; dependency classification never changes its outcome.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url)),repo=path.resolve(root,'../..');
const read=p=>JSON.parse(fs.readFileSync(p)),sha=b=>createHash('sha256').update(b).digest('hex');
const dir=root+'/dependencies-v1',e=read(dir+'/evidence.json'),a=read(dir+'/audit.json');
assert.equal(e.source_revision,'b1557a52fcd5b574a90dfa31ba7b65d363c31879');
assert.equal(a.source_revision,e.source_revision);assert.equal(a.engine_sha256,e.engine_sha256);
assert.equal(a.corpus_revision,'d53ebc8b1931d9f1541e43d59650776e4b766248');
assert.equal(a.entries.length,196);assert.equal(a.missing_classification,0);
const archive=dir+'/capture.tar.gz',bytes=fs.readFileSync(archive);
assert.equal(bytes.length,e.archive.bytes);assert.equal(sha(bytes),e.archive.sha256);
const temp=fs.mkdtempSync(path.join(os.tmpdir(),'temiya-super-dependencies-'));
const verify=dir=>JSON.parse(execFileSync(process.execPath,[repo+'/scripts/verify-test262-baseline.mjs',dir],{encoding:'utf8',cwd:repo,maxBuffer:4*1024*1024}));
try {
 const names=execFileSync('tar',['-tzf',archive],{encoding:'utf8'}).trim().split('\n');
 assert.equal(new Set(names).size,names.length);
 assert(names.every(n=>!path.isAbsolute(n)&&!n.split('/').some(p=>p==='..'||p==='.')));
 assert(execFileSync('tar',['-tvzf',archive],{encoding:'utf8'}).trim().split('\n').every(n=>n[0]==='d'||n[0]==='-'));
 execFileSync('tar',['-xzf',archive,'--no-same-owner','--no-same-permissions','-C',temp]);
 const originals=read(temp+'/original-inputs.json');assert.equal(originals.corpus_revision,a.corpus_revision);assert.equal(originals.files.length,311);
 const inputs=new Map();for(const row of originals.files){assert(!inputs.has(row.file));assert(!path.isAbsolute(row.file)&&!row.file.split('/').some(p=>p==='.'||p==='..'));const b=fs.readFileSync(temp+'/originals/'+row.file);assert.equal(b.length,row.bytes);assert.equal(sha(b),row.sha256);inputs.set(row.file,row.sha256);}
 const prior=read(repo+'/benchmarks/derived-classes/dependencies-v1/audit.json');
 const priorEntries=new Map(prior.entries.map(r=>[r.path+'#'+r.variant,r]));
 const catalog=new Map(read(temp+'/capture-ast/catalog.json').map(r=>[r.path,r]));
 const counted={};for(const row of a.entries)counted[row.classification.category]=(counted[row.classification.category]??0)+1;
 assert.deepEqual(counted,a.classification_counts);
 assert.deepEqual(counted,{'eval-unavailable':24,'unsupported-class-element-gate':108,'async-generator-gate':2,'retained-verified-dependency':40,'missing-native-api':10,'proposal-adoption-and-parser':12});
 const probes=read(temp+'/probes/probes-v2.json');assert.equal(probes.length,24);assert.equal(new Set(probes.map(p=>p.name)).size,24);
 for(const mode of ['ast','auto']){
  const capture=temp+'/capture-'+mode,v=verify(capture);assert.equal(v.files,310);assert.equal(v.variants,601);assert.equal(v.totals.pass,405);assert.equal(v.totals.fail,196);
  for(const key of ['skip','timeout','not_run','unreported'])assert.equal(v.totals[key],0);
  const conditions=read(capture+'/conditions.json');assert.equal(conditions.engine_sha256,e.engine_sha256);assert.equal(conditions.provided_engine_build_info.source_revision,a.source_revision);
  const failures=new Map(read(capture+'/outcomes.json').filter(r=>r.outcome==='fail').map(r=>[r.path+'#'+r.variant,r]));assert.equal(failures.size,a.entries.length);
  const rawFiles=new Map();
  for(const row of a.entries){const key=row.path+'#'+row.variant,outcome=failures.get(key);assert(outcome,key);assert.equal(row.later_behavior_unproven,true);assert.equal(row.source_sha256,inputs.get(row.path));
   const source=fs.readFileSync(temp+'/originals/'+row.path,'utf8'),record=row.records[mode];assert.equal(record.raw_record,outcome.raw_record);
   const match=/^(raw\/execution-\d+\.json)#\/results\/(\d+)$/.exec(record.raw_record);assert(match);
   const file=capture+'/'+match[1];assert.equal(sha(fs.readFileSync(file)),record.raw_file_sha256);if(!rawFiles.has(file))rawFiles.set(file,read(file));const raw=rawFiles.get(file).results[Number(match[2])];
   assert.equal(raw.path,row.path);assert.equal(raw.variant,row.variant);assert.equal(raw.status,'fail');for(const field of ['diagnostic','detail','observed_failure'])assert.deepEqual(raw[field],record[field]);
   const c=row.classification;assert(c.issues.length>0);assert(c.issues.every(Number.isInteger));assert(!c.issues.includes(156),'stale super blocker');
   if(c.probe)assert(probes.some(p=>p.name===c.probe),'missing independent probe');
   if(c.category==='retained-verified-dependency'){const old=priorEntries.get(key);assert(old);assert.equal(old.source_sha256,row.source_sha256);assert.deepEqual(c.issues,old.classification.issues);for(const field of ['diagnostic','detail','observed_failure'])assert.deepEqual(record[field],old.records[mode][field]);}
   else if(c.category==='unsupported-class-element-gate'){assert.equal(record.diagnostic,'Engine: class fields and private elements are not yet implemented');const issues=[];for(const[feature,issue]of [['class-fields-public',157],['class-static-fields-public',158],['class-static-block',158],['class-fields-private',159],['class-methods-private',160]])if(catalog.get(row.path).features.includes(feature)&&!issues.includes(issue))issues.push(issue);assert.deepEqual(c.issues,issues);}
   else if(c.category==='eval-unavailable'){assert.deepEqual(c.issues,[169]);assert(/\beval\b/.test(source));assert.equal(c.probe,'eval');}
   else if(c.category==='async-generator-gate'){assert.deepEqual(c.issues,[190]);assert.equal(record.diagnostic,'Engine: async generators are not yet implemented');}
   else if(c.category==='proposal-adoption-and-parser'){assert.deepEqual(c.issues,[541]);assert(catalog.get(row.path).features.includes('import-defer'));assert(/import\s+defer\b/.test(source));assert(/unexpected token/.test(record.diagnostic));}
   else if(c.category==='missing-native-api'){if(c.probe==='Object.freeze'){assert(source.includes('Object.freeze('));assert.deepEqual(c.issues,[208]);}else{assert.equal(c.probe,'String.toLowerCase');assert(source.includes('.toLowerCase('));assert.deepEqual(c.issues,[224]);}}
   else assert.fail('unknown dependency category');
  }
  const probeCapture=temp+'/probes/capture-v2-'+mode,pv=verify(probeCapture);assert.equal(pv.corpus_revision,null);assert.equal(pv.files,24);assert.equal(pv.variants,24);assert.equal(pv.totals.pass,24);for(const key of ['fail','skip','timeout','not_run','unreported'])assert.equal(pv.totals[key],0);assert.equal(read(probeCapture+'/conditions.json').engine_sha256,e.engine_sha256);
 }
 console.log(JSON.stringify({integrity:'pass',source:a.source_revision,variants_per_backend:601,pass:405,retained_failures:196,authored_missing_behavior_probes_per_backend:24,categories:counted,scope:a.scope}));
} finally {fs.rmSync(temp,{recursive:true,force:true});}
