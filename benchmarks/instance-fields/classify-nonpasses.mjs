// Inventory evidence, never convert dependency candidates into passing tests.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
const [capture,corpus]=process.argv.slice(2);
assert(capture&&corpus,'usage: classify-nonpasses.mjs CAPTURE PINNED_TEST262');
const read=p=>JSON.parse(fs.readFileSync(p));
const catalog=new Map(read(path.join(capture,'catalog.json')).map(x=>[x.path,x]));
const cache=new Map(),groups=new Map(),rows=[];
for(const outcome of read(path.join(capture,'outcomes.json'))) {
 if(outcome.outcome==='pass')continue;
 const [file,pointer]=outcome.raw_record.split('#');
 if(!cache.has(file))cache.set(file,read(path.join(capture,file)));
 const record=cache.get(file).results[Number(pointer.split('/').at(-1))];
 assert.equal(record.path,outcome.path);assert.equal(record.variant,outcome.variant);
 const source=fs.readFileSync(path.join(corpus,outcome.path),'utf8');
 const diagnostic=record.diagnostic??'',issues=[];
 let interpretation='unresolved; no attribution established';
 if(diagnostic.includes('static class fields, static blocks and private elements')) {
  const features=catalog.get(outcome.path).features;
  if(features.includes('class-static-fields-public')||features.includes('class-static-block'))issues.push(158);
  if(features.includes('class-fields-private'))issues.push(159);
  if(features.includes('class-methods-private'))issues.push(160);
  interpretation='explicit unsupported-element diagnostic; metadata narrows dependency candidates, not proof of remaining assertions';
 } else if(diagnostic.includes('async generators are not yet implemented')) {
  issues.push(190);interpretation='explicit unsupported async-generator diagnostic';
 } else if(diagnostic.includes('eval is not defined')&&/\beval\s*\(/.test(source)) {
  issues.push(169);interpretation='missing eval diagnostic and source call';
 } else if(diagnostic.includes('different error constructor')&&/\beval\s*\(/.test(source)) {
  issues.push(169);interpretation='eval appears in assertion test; missing-eval probe supports dependency candidate, exact assertion remains unproven';
 } else if(diagnostic.includes('function target must be an object')&&/\bSymbol\s*\(/.test(source)) {
  issues.push(204);interpretation='Symbol construction occurs in failing source; distinct-Symbol probe supports dependency candidate, subsequent assertions remain unproven';
 } else if(diagnostic.includes('value is not callable')&&/Promise\.resolve\s*\(/.test(source)) {
  issues.push(179);interpretation='Promise.resolve occurs in failing source; standalone async probe supports dependency candidate, subsequent assertions remain unproven';
 } else if(diagnostic.includes('accessor cannot be a generator')) {
  interpretation='confirmed parser ASI defect fixed by 38983db; final whole-file result must still be checked';
 }
 const key=JSON.stringify([outcome.outcome,record.detail,diagnostic]);groups.set(key,(groups.get(key)??0)+1);
 rows.push({...outcome,detail:record.detail,diagnostic,source_sha256:createHash('sha256').update(source).digest('hex'),features:catalog.get(outcome.path).features,issue_candidates:issues,interpretation,source_evidence:source.split('\n').flatMap((line,i)=>/\beval\s*\(|\bSymbol\s*\(|Promise\.resolve\s*\(/.test(line)?[{line:i+1,text:line}]:[])});
}
console.log(JSON.stringify({scope:'Inventory of nonpassing official variants. Dependency candidates do not establish conformance or waive any failure/timeout.',conditions:read(path.join(capture,'conditions.json')),nonpasses:rows.length,groups:[...groups].map(([key,count])=>({diagnostic:JSON.parse(key),count})),rows},null,2));
