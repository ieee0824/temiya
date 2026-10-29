// Instrumented original workloads: diagnostic counters, never native speed gates.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {execFileSync, spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {oracle, revision, fixtureSha256} from './omoikane-benchmark-reference.mjs';

const [baseline, measured, fixtures, output, baselineTarget, measuredTarget, namesText] = process.argv.slice(2);
assert([baseline, measured, fixtures, output, baselineTarget, measuredTarget].every(Boolean));
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const read = p => JSON.parse(fs.readFileSync(p));
const run = (command, args, options = {}) => execFileSync(command, args, {encoding:'utf8', maxBuffer:64*1024*1024, ...options});
const git = (cwd, ...args) => run('git', ['-c', `safe.directory=${cwd}`, '-C', cwd, ...args]).trim();
const manifest = read(path.join(fixtures, 'manifest.json'));
assert.equal(manifest.revision, revision);
assert.equal(manifest.fixture_sha256, fixtureSha256);
assert.deepEqual(manifest.workloads.map(w => [w.name,w.iterations,w.expected]), oracle);
for (const item of manifest.workloads) {
  const input = read(path.join(fixtures, `${item.name}.json`));
  assert.equal(input.name, item.name); assert.equal(input.iterations, item.iterations); assert.equal(input.expected, item.expected);
  assert.equal(hash(input.setup), item.setup_sha256); assert.equal(hash(input.invocation), item.invocation_sha256);
}
const names = namesText ? namesText.split(',') : ['primitive-string-property','string-concat','prop-mega'];
assert(names.length > 0 && new Set(names).size === names.length && names.every(name => oracle.some(([known]) => known === name)), 'select unique original workload names');
const metadata = {
  scope:'Diagnostic whole-process instrumented counters, including startup/setup, one checked warmup and one checked pass. Original workload bodies/counts/oracles unchanged. Cache/branch simulation is a model, not native hardware measurements; instrumented times never qualify speed.',
  recorded_at:new Date().toISOString(), rustc:run('rustc',['-Vv']).trim(),
  valgrind:run('valgrind',['--version']).trim(), machine:run('lscpu',[]),
  original_manifest:manifest, diagnostic_workloads:names, runs:[], builds:[],
};
fs.writeFileSync(path.join(output,'metadata.json'),JSON.stringify(metadata,null,2)+'\n');
for (const [label,checkout,target,expected] of [
  ['baseline',baseline,baselineTarget,process.env.EXPECTED_BASELINE],
  ['measured',measured,measuredTarget,process.env.EXPECTED_MEASURED],
]) {
  assert(/^[a-f0-9]{40}$/.test(expected));
  assert.equal(git(checkout,'rev-parse','HEAD'),expected);
  assert.equal(git(checkout,'status','--porcelain'),'');
  const dir=path.join(output,label); fs.mkdirSync(dir);
  const executable=path.join(target,'release/examples/measure_workload');
  const build={label,revision:expected,executable_sha256:hash(fs.readFileSync(executable)),profile:'release',features:[],
    source_objects:Object.fromEntries(['src','examples','Cargo.toml','Cargo.lock','scripts/omoikane-benchmark-reference.mjs'].map(p=>[p,git(checkout,'rev-parse',`HEAD:${p}`)]))};
  metadata.builds.push(build); fs.copyFileSync(executable,path.join(dir,'worker.bin'));
  const symbols=run('nm',['-S','-C',executable]); fs.writeFileSync(path.join(dir,'symbols.txt'),symbols);
  const selected=symbols.split('\n').filter(l=>/VirtualMachine>::run::<(?:false|true)>$|Object>::(?:own_element_value|try_get_own_property)$|elements::ElementRef.*::value$|elements::Elements.*::get_value$/.test(l));
  const functions=[];
  for (const [index,line] of selected.entries()) {
    const m=line.match(/^([a-f0-9]+) ([a-f0-9]+) \w (.+)$/); assert(m);
    const start=Number.parseInt(m[1],16),bytes=Number.parseInt(m[2],16);
    const assembly=run('objdump',['-Cd',`--start-address=${start}`,`--stop-address=${start+bytes}`,executable]);
    const file=`function-${index}.asm.txt`; fs.writeFileSync(path.join(dir,file),assembly); functions.push({symbol:m[3],address:m[1],bytes,file});
  }
  fs.writeFileSync(path.join(dir,'functions.json'),JSON.stringify(functions,null,2)+'\n');
  for (const name of names) {
    const inputPath=path.join(fixtures,`${name}.json`),input=read(inputPath),prefix=path.join(dir,name);
    const args=['--tool=callgrind','--dump-instr=yes','--collect-jumps=yes','--cache-sim=yes','--branch-sim=yes',
      '--I1=32768,8,64','--D1=32768,8,64','--LL=8388608,16,64',`--callgrind-out-file=${prefix}.callgrind`,
      `--log-file=${prefix}.valgrind.log`,executable,inputPath,'1'];
    const child=spawnSync('valgrind',args,{encoding:'utf8',timeout:180000,maxBuffer:8*1024*1024});
    fs.writeFileSync(`${prefix}.stdout`,child.stdout??''); fs.writeFileSync(`${prefix}.stderr`,child.stderr??'');
    const record={label,name,args,input_sha256:hash(fs.readFileSync(inputPath)),exit_status:child.status,signal:child.signal,error:child.error?.message??null};
    try {
      assert.equal(child.status,0); assert.equal(child.signal,null);
      const result=JSON.parse(child.stdout); assert.equal(result.mode,'Auto Script'); assert.equal(result.profile,'release');
      assert.equal(result.expected,input.expected); assert.equal(result.validated_passes,2); assert.equal(result.samples_ns.length,1);
      const raw=fs.readFileSync(`${prefix}.callgrind`,'utf8');
      const events=raw.match(/^events: (.+)$/m)[1].trim().split(/\s+/),values=raw.match(/^summary: (.+)$/m)[1].trim().split(/\s+/).map(Number);
      assert.equal(events.length,values.length); assert(values.every(n=>Number.isSafeInteger(n)&&n>=0));
      record.counters=Object.fromEntries(events.map((n,i)=>[n,values[i]])); assert(record.counters.Ir>0); record.validated_passes=2;
      const annotation=spawnSync('callgrind_annotate',['--inclusive=no','--auto=no','--threshold=100',`${prefix}.callgrind`],{encoding:'utf8',maxBuffer:32*1024*1024});
      fs.writeFileSync(`${prefix}.exclusive.txt`,annotation.stdout??''); fs.writeFileSync(`${prefix}.annotation.stderr`,annotation.stderr??'');
      record.annotation_exit=annotation.status; assert.equal(annotation.status,0);
    } catch (e) { record.error??=e.message; }
    metadata.runs.push(record); fs.writeFileSync(path.join(output,'metadata.json'),JSON.stringify(metadata,null,2)+'\n'); console.log(JSON.stringify(record));
  }
}
assert.equal(metadata.runs.length,names.length*2); assert(metadata.runs.every(r=>!r.error));
