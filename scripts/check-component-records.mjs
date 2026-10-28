// Corrupt complete real observations to check that offline acceptance rejects them.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const original=JSON.parse(fs.readFileSync(path.join(root,'benchmarks/components/local-arm64/components.json')));
const dir=fs.mkdtempSync(path.join(os.tmpdir(),'temiya-component-negative-'));
let rejected=0;
try{
 for(const mutate of [
  r=>r.runs[0].statistics.median_ns++,
  r=>r.runs[0].measurement.warmup_result++,
  r=>r.runs[1].name=r.runs[0].name,
  r=>r.runs[0].order=99,
  r=>r.engine_gc_reclamation_ready=!r.engine_gc_reclamation_ready,
  r=>r.all_component_cases_valid=false,
  r=>r.build.source_clean=false,
 ]){
  const report=structuredClone(original);mutate(report);
  const file=path.join(dir,'invalid.json');fs.writeFileSync(file,JSON.stringify(report));
  const child=spawnSync(process.execPath,[path.join(root,'scripts/verify-components.mjs'),file],{encoding:'utf8',timeout:10000});
  if(child.status===0||child.signal||child.error)throw Error('invalid stored observation was accepted or checker failed to run');
  rejected++;
 }
}finally{fs.rmSync(dir,{recursive:true});}
console.log(JSON.stringify({rejected_record_mutations:rejected}));
