import fs from 'node:fs';
import {createHash} from 'node:crypto';
import vm from 'node:vm';
import {regexpFixtureVersion,regexpWorkloads} from './regexp-benchmark-reference.mjs';
import {revision} from './omoikane-benchmark-reference.mjs';
const base=new URL('../benchmarks/regexp-inputs/',import.meta.url);
const manifest=JSON.parse(fs.readFileSync(new URL('manifest.json',base)));
const hash=s=>createHash('sha256').update(s).digest('hex');
const oracles=[
 ['regexp-construction',128,128*(8+5+2)],
 ['regexp-global-exec',2048,2048*(3+4+2)],
 ['regexp-unicode-captures',2048,1024*(4+2)],
];
if(manifest.fixture_kind!=='authored-regexp'||manifest.fixture_version!==regexpFixtureVersion||manifest.reference_engine_revision!==revision||manifest.workloads.length!==3||regexpWorkloads.length!==3)throw Error('authored manifest contract');
for(const [i,source]of regexpWorkloads.entries()){
 const fixture=JSON.parse(fs.readFileSync(new URL(source.name+'.json',base))),entry=manifest.workloads[i];
 if(JSON.stringify(fixture)!==JSON.stringify(source)||JSON.stringify([source.name,source.iterations,source.expected])!==JSON.stringify(oracles[i])||entry.name!==source.name||entry.iterations!==source.iterations||entry.expected!==source.expected||entry.setup_sha256!==hash(source.setup)||entry.invocation_sha256!==hash(source.invocation))throw Error('reference, fixture or independent oracle changed');
 const context=vm.createContext({});
 new vm.Script(source.setup).runInContext(context,{timeout:1000});
 for(let pass=0;pass<2;pass++)if(new vm.Script(source.invocation).runInContext(context,{timeout:1000})!==oracles[i][2])throw Error('Node reference disagrees with independent length/count oracle');
}
console.log('Three exact authored fixtures and six Node reference passes agree with independent oracles; this is not a benchmark or Temiya conformance claim.');
