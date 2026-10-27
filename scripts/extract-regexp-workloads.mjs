import fs from 'node:fs';
import path from 'node:path';
import {createHash} from 'node:crypto';
import {regexpFixtureVersion,regexpWorkloads} from './regexp-benchmark-reference.mjs';
import {revision} from './omoikane-benchmark-reference.mjs';
const [directory]=process.argv.slice(2);
if(!directory)throw Error('usage: extract-regexp-workloads.mjs NEW_DIRECTORY');
fs.mkdirSync(directory);
const hash=s=>createHash('sha256').update(s).digest('hex');
const workloads=regexpWorkloads.map(workload=>{
 fs.writeFileSync(path.join(directory,workload.name+'.json'),JSON.stringify(workload,null,2)+'\n');
 const {setup,invocation,...entry}=workload;
 return {...entry,setup_sha256:hash(setup),invocation_sha256:hash(invocation)};
});
fs.writeFileSync(path.join(directory,'manifest.json'),JSON.stringify({
 fixture_kind:'authored-regexp',fixture_version:regexpFixtureVersion,
 reference_engine_revision:revision,
 source:'Authored RegExp suite; original omoikane eleven unchanged',
 workloads,
},null,2)+'\n');
