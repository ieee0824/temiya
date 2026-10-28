// Offline evidence verification. No engine execution or elapsed-speed claim.
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {createHash} from 'node:crypto';
import {gunzipSync} from 'node:zlib';
const root=path.dirname(fileURLToPath(import.meta.url));
const hash=b=>createHash('sha256').update(b).digest('hex');
const require=(ok,message)=>{if(!ok)throw Error(message);};
const inventory=JSON.parse(fs.readFileSync(path.join(root,'inventory.json')));
const files=[];
function walk(dir){for(const e of fs.readdirSync(dir,{withFileTypes:true})){const f=path.join(dir,e.name);if(e.isDirectory())walk(f);else if(f!==path.join(root,'inventory.json'))files.push(path.relative(root,f));}}
walk(root);require(JSON.stringify(files.sort())===JSON.stringify(Object.keys(inventory.files).sort()),'inventory coverage');
for(const [file,record] of Object.entries(inventory.files)){
 const bytes=fs.readFileSync(path.join(root,file));require(bytes.length===record.bytes&&hash(bytes)===record.sha256,'stored bytes '+file);
 if(record.uncompressed){const plain=gunzipSync(bytes);require(plain.length===record.uncompressed.bytes&&hash(plain)===record.uncompressed.sha256,'decompressed bytes '+file);}
}
const dir=path.join(root,'run-40617376161/intel6973-key-eq-profile-40617376161-1-copy-4');
const read=file=>{const p=path.join(dir,file);return fs.existsSync(p)?fs.readFileSync(p):gunzipSync(fs.readFileSync(p+'.gz'));};
const json=file=>JSON.parse(read(file));
const hardware=json('hardware.json'),provenance=json('provenance.json'),input=json('arith-input.json'),manifest=json('workloads/manifest.json');
require(hardware.target_available===true&&hardware.architecture==='x64'&&hardware.observed_cpu_models.length>0&&hardware.observed_cpu_models.every(m=>m.includes('6973P-C')),'actual requested Intel CPU');
require(manifest.revision==='007556adccb8fb4329236932ca8175f8e8d119d2'&&manifest.fixture_version===2&&manifest.fixture_sha256==='177eb80ae73d1081eb620f747b4703e2a9f2731e0255bb60131b73b7dcac6464','pinned original fixture');
const entry=manifest.workloads.find(w=>w.name==='arith');
require(input.name==='arith'&&input.iterations===2000000&&input.expected===64&&entry.iterations===2000000&&entry.expected===64,'original arithmetic counts');
require(hash(read('arith-input.json'))===provenance.input_sha256&&read('arith-input.json').equals(read('workloads/arith.json')),'unchanged input bytes');
require(hash(input.setup)===entry.setup_sha256&&hash(input.setup)===provenance.setup_sha256&&hash(input.invocation)===entry.invocation_sha256&&hash(input.invocation)===provenance.invocation_sha256,'setup and invocation hashes');
require(entry.body_sha256==='faeedc595cc9e0e6e538fb38b8e37c6a398ef2839f1ec74752cb73336fd6a30f'&&input.invocation==='__temiyaBenchmarkBody(2000000);\n','exact original arithmetic');
let answer=1n;for(let i=0n;i<2000000n;i++)answer=(answer+i*3n)%1000003n;
require(answer===64n&&provenance.independent_expected===Number(answer)&&provenance.iterations===2000000,'independent BigInt oracle');
const totals={};
for(const [label,revision] of Object.entries({baseline:'56164251391bea02c75bb1d8bbf2483dc03545ee',current:'87c64d3cf73999393cddcbb114d0a1075a54db2b'})){
 const worker=json(label+'-worker-output.json'),build=provenance.workers[label],raw=read(label+'-callgrind.out').toString();
 require(build.revision===revision&&/^[a-f0-9]{64}$/.test(build.executable_sha256)&&build.profile==='release'&&build.features.length===0&&build.release_debug===1&&build.rustflags==='-C dwarf-version=4','source and diagnostic build '+label);
 require(worker.engine==='Temiya'&&worker.profile==='release'&&worker.architecture==='x86_64'&&worker.expected===64&&worker.validated_passes===2&&worker.samples_ns.length===1&&worker.samples_ns[0]>0&&worker.warmup_ns>0&&worker.setup_backend==='Bytecode'&&worker.invocation_backend==='Bytecode','two actual checked Bytecode passes '+label);
 require(/^events: Ir$/m.test(raw),'instruction event '+label);
 const summary=Number(raw.match(/^summary: (\d+)$/m)?.[1]),total=Number(raw.match(/^totals: (\d+)$/m)?.[1]);
 require(Number.isSafeInteger(total)&&total>0&&total===summary,'complete raw instruction total '+label);
 const log=Number(read(label+'-valgrind.log').toString().match(/Collected\s*:\s*(\d+)/)?.[1]);require(log===total,'Valgrind total '+label);
 for(const mode of ['exclusive','inclusive']){const annotated=Number(read(label+'-'+mode+'.txt').toString().match(/([\d,]+)\s+\(100\.0%\)\s+PROGRAM TOTALS/)?.[1].replaceAll(',',''));require(annotated===total,'annotation total '+label+' '+mode);}
 totals[label]=total;
}
let unavailable=0;
for(const run of fs.readdirSync(root).filter(n=>n.startsWith('run-'))){for(const copy of fs.readdirSync(path.join(root,run))){const h=JSON.parse(fs.readFileSync(path.join(root,run,copy,'hardware.json')));if(h.target_available===false)unavailable++;}}
console.log(JSON.stringify({files:files.length,checked_results:4,original_iterations:2000000,expected:Number(answer),instruction_references:totals,unavailable_probes:unavailable,qualification:'Actual Intel instruction evidence; not native elapsed-speed qualification'}));
