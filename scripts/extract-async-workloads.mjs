import fs from 'node:fs';
import path from 'node:path';
import {createHash} from 'node:crypto';
import {asyncFixtureVersion,asyncWorkloads} from './async-benchmark-reference.mjs';
const [directory]=process.argv.slice(2);
if(!directory)throw Error('usage: extract-async-workloads.mjs NEW_DIRECTORY');
fs.mkdirSync(directory); // Preserve any existing inputs and measurements.
const hash=s=>createHash('sha256').update(s).digest('hex');
const workloads=asyncWorkloads.map(w=>{
  fs.writeFileSync(path.join(directory,w.name+'.json'),JSON.stringify(w,null,2)+'\n');
  const {setup,invocation,completion,...entry}=w;
  return {...entry,setup_sha256:hash(setup),invocation_sha256:hash(invocation),completion_sha256:hash(completion)};
});
fs.writeFileSync(path.join(directory,'manifest.json'),JSON.stringify({fixture_version:asyncFixtureVersion,source:'Authored async checkpoint suite; original omoikane eleven unchanged',workloads},null,2)+'\n');
