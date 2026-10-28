import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {createHash} from 'node:crypto';
import {gunzipSync} from 'node:zlib';
import {execFileSync} from 'node:child_process';
const root=path.dirname(fileURLToPath(import.meta.url));
const inventory=JSON.parse(fs.readFileSync(path.join(root,'inventory.json')));
const hash=b=>createHash('sha256').update(b).digest('hex'),files=[];
function walk(dir){for(const e of fs.readdirSync(dir,{withFileTypes:true})){const f=path.join(dir,e.name);if(e.isDirectory())walk(f);else if(f!==path.join(root,'inventory.json'))files.push(path.relative(root,f));}}
walk(root);
if(JSON.stringify(files.sort())!==JSON.stringify(Object.keys(inventory.files).sort()))throw Error('archive file coverage');
for(const [file,entry] of Object.entries(inventory.files)){
 const bytes=fs.readFileSync(path.join(root,file));if(bytes.length!==entry.bytes||hash(bytes)!==entry.sha256)throw Error('stored bytes '+file);
 if(entry.uncompressed){const plain=gunzipSync(bytes);if(plain.length!==entry.uncompressed.bytes||hash(plain)!==entry.uncompressed.sha256)throw Error('decompressed bytes '+file);}
}
const source=JSON.parse(fs.readFileSync(path.join(root,'source-verification.json')));
let workers=0,checked=0,reports=0;
for(const relative of files.filter(f=>f.endsWith('/components.json'))){
 const file=path.join(root,relative),report=JSON.parse(fs.readFileSync(file));
 const observed=JSON.parse(execFileSync(process.execPath,[path.join(root,'../../scripts/verify-components.mjs'),file],{encoding:'utf8'}));
 const adjacent=path.join(path.dirname(file),'build-info.json');
 const build=JSON.parse(fs.readFileSync(fs.existsSync(adjacent)?adjacent:path.join(path.dirname(file),'component-build/build-info.json')));
 if(JSON.stringify(build)!==JSON.stringify(report.build)||build.revision!==source.measured_source)throw Error('recorded build/source mismatch');
 const checks=JSON.parse(fs.readFileSync(path.join(path.dirname(file),'component-checks.json')));
 if(checks.cases!==6||checks.checked_results!==12||checks.rejected_measurement_mutations!==42||checks.rejected_commands!==6)throw Error('native worker check coverage');
 reports++;workers+=observed.workers;checked+=observed.checked_results;
}
console.log(JSON.stringify({files:files.length,reports,workers,checked_results:checked,qualification:'Archive and observations verified; Engine reclamation and cross-engine speed are separate gates'}));
