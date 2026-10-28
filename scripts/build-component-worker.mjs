// Build a frozen, clean revision before any timing begins.
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const directory=process.argv[2];if(!directory)throw Error('usage: build-component-worker.mjs OUTPUT_DIRECTORY');
const out=path.resolve(directory),git=args=>execFileSync('git',['-c','safe.directory='+root,'-C',root,...args],{encoding:'utf8'}).trim();
if(git(['status','--porcelain'])!=='')throw Error('build requires a clean source checkout');
if(fs.existsSync(out))throw Error('output directory must be new to preserve frozen evidence');
fs.mkdirSync(out,{recursive:true});
const env={...process.env,CARGO_TARGET_DIR:path.join(out,'target'),CARGO_PROFILE_RELEASE_DEBUG:'false',RUSTFLAGS:'',CARGO_ENCODED_RUSTFLAGS:''};
execFileSync('cargo',['build','--locked','--release','--no-default-features','--example','measure_components','--example','ownership_probe'],{cwd:root,env,stdio:'inherit'});
const hash=b=>createHash('sha256').update(b).digest('hex'),suffix=process.platform==='win32'?'.exe':'';
const workers={};for(const name of ['measure_components','ownership_probe']){const executable=path.join(out,'target/release/examples',name+suffix);workers[name]={executable,sha256:hash(fs.readFileSync(executable))};}
const source_files={};for(const file of ['examples/measure_components.rs','examples/ownership_probe.rs','examples/support/ownership.rs','scripts/component-benchmark-reference.mjs','Cargo.lock'])source_files[file]=hash(fs.readFileSync(path.join(root,file)));
fs.writeFileSync(path.join(out,'build-info.json'),JSON.stringify({revision:git(['rev-parse','HEAD']),source_tree:git(['rev-parse','HEAD^{tree}']),source_clean:git(['status','--porcelain'])==='',rustc:execFileSync('rustc',['-Vv'],{encoding:'utf8'}).trim(),profile:'release',features:[],release_debug:false,rustflags:'',workers,source_files},null,2)+'\n');
