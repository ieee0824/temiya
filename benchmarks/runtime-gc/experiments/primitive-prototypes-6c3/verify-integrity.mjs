// Verify retained bytes, including decompressed CI logs. This is not a speed gate.
import fs from 'node:fs';
import {fileURLToPath} from 'node:url';
import {createHash} from 'node:crypto';
import {gunzipSync} from 'node:zlib';
import assert from 'node:assert/strict';
const directory=fileURLToPath(new URL('.',import.meta.url));
const read=name=>JSON.parse(fs.readFileSync(directory+name));
const hash=bytes=>createHash('sha256').update(bytes).digest('hex');
const provenance=read('provenance.json');
const actual=[];
function walk(prefix=''){
 for(const entry of fs.readdirSync(directory+prefix,{withFileTypes:true})){
  const name=prefix+entry.name;
  if(entry.isDirectory())walk(name+'/');
  else if(entry.isFile()&&name!=='provenance.json')actual.push(name);
  else assert(entry.isFile(),'unexpected non-file entry');
 }
}
walk();assert.deepEqual(actual.sort(),Object.keys(provenance.files).sort(),'complete file inventory');
for(const [name,expected]of Object.entries(provenance.files)){
 assert(!name.split('/').includes('..')&&!name.startsWith('/'),'relative evidence path');
 assert.equal(hash(fs.readFileSync(directory+name)),expected,name);
}
const ci=read('ci/audit.json');
assert.equal(hash(gunzipSync(fs.readFileSync(directory+'ci/current.log.gz'))),ci.raw_sha256,'decompressed native CI log');
const miri=read('ci/full-miri16.json');
assert.equal(hash(gunzipSync(fs.readFileSync(directory+'ci/full-miri16.log.gz'))),miri.full_log_sha256,'decompressed complete Miri log');
const profile=read('runtime-profile-audited-info.json');
assert.equal(hash(fs.readFileSync(directory+'runtime-profile-audited.json')),profile.raw_sha256,'profile raw');
assert.equal(hash(fs.readFileSync(directory+'runtime-profile-audited.log')),profile.log_sha256,'profile log');
console.log(JSON.stringify({source:provenance.source_revision,verified_files:actual.length,decompressed_native_log:true,profile_raw_and_log:true,scope:'Byte integrity only; run statistical, build-identity, Miri and acceptance audits separately'}));
