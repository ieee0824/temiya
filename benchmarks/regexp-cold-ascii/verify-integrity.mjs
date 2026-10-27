// Offline archive checks; no engine execution or fresh timing.
import fs from 'node:fs';
import path from 'node:path';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';

const root = path.dirname(fileURLToPath(import.meta.url));
const inventory = JSON.parse(fs.readFileSync(path.join(root, 'inventory.json')));
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const files = [];
function walk(dir) {
  for (const entry of fs.readdirSync(dir, {withFileTypes: true})) {
    const file = path.join(dir, entry.name);
    if (entry.isDirectory()) walk(file);
    else files.push(path.relative(root, file));
  }
}
walk(root);
files.sort();
const recorded = Object.keys(inventory.files).sort();
if (JSON.stringify(files.filter(p => p !== 'inventory.json')) !== JSON.stringify(recorded)) {
  throw Error('Missing or unrecorded archive file');
}
for (const file of recorded) {
  if (hash(fs.readFileSync(path.join(root, file))) !== inventory.files[file]) {
    throw Error(`Archive bytes changed: ${file}`);
  }
}
let reports = 0;
for (const file of recorded.filter(p => /\/(cold-report|cold|cold-same-binary)\.json$/.test(p))) {
  execFileSync(process.execPath, [path.join(root, 'verify-cold-report.mjs'), path.join(root, file)]);
  reports++;
}
const macros = execFileSync(process.execPath, [
  path.join(root, '../regexp-cache/verify-recorded-timing.mjs'), path.join(root, 'additional'),
], {encoding: 'utf8'});
process.stdout.write(macros);
console.log(JSON.stringify({files: recorded.length, cold_reports: reports,
  qualification: 'Integrity and recorded evidence verification, not speed qualification'}));
