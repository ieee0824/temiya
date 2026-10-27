import fs from 'node:fs';
import path from 'node:path';
import {createHash} from 'node:crypto';
import {gunzipSync} from 'node:zlib';
const directory=process.argv[2];
if(!directory)throw Error('Usage: node verify-integrity.mjs <evidence directory>');
const manifest=JSON.parse(fs.readFileSync(path.join(directory,'inventory.json')));
const hash=bytes=>createHash('sha256').update(bytes).digest('hex');
const seen=new Set();
for(const file of manifest.files){
 if(seen.has(file.path))throw Error('Duplicate inventory path');
 seen.add(file.path);
 const bytes=fs.readFileSync(path.join(directory,file.path));
 if(bytes.length!==file.bytes||hash(bytes)!==file.sha256)throw Error('Stored file mismatch: '+file.path);
 if(file.uncompressed){
  const plain=gunzipSync(bytes);
  if(plain.length!==file.uncompressed.bytes||hash(plain)!==file.uncompressed.sha256)throw Error('Decompression mismatch: '+file.path);
 }
}
console.log('Verified '+seen.size+' stored files and all recorded decompressed hashes.');
