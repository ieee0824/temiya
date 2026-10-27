// Verify an actually built shell against the selected source and execution modes.
import fs from 'node:fs';
import path from 'node:path';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import os from 'node:os';
import {fileURLToPath} from 'node:url';
const [checkoutArg, binaryArg, outputArg] = process.argv.slice(2);
if (!checkoutArg || !binaryArg || !outputArg) throw Error('usage: CHECKOUT SHELL OUTPUT.json');
const checkout = path.resolve(checkoutArg), binary = path.resolve(binaryArg);
const target = JSON.parse(fs.readFileSync(new URL('../benchmarks/targets/spidermonkey.json', import.meta.url)));
const git = args => execFileSync('git', ['-c', `safe.directory=${checkout}`, '-C', checkout, ...args], {encoding:'utf8'}).trim();
if (git(['rev-parse','HEAD']) !== target.revision || git(['status','--porcelain','--untracked-files=no']) !== '') throw Error('source identity changed');
const smoke = new URL('./spidermonkey-target-smoke.js', import.meta.url);
const modes = [];
for (const [name, flags] of Object.entries(target.modes)) {
  const output = execFileSync(binary, [...flags, '-f', fileURLToPath(smoke)], {encoding:'utf8', timeout:60000, maxBuffer:1024*1024});
  const observed = JSON.parse(output.trim());
  if (name === 'interpreter' && ['ion.enable','baseline.enable','blinterp.enable'].some(k => observed.jit_options[k] !== 0)) throw Error('JIT disable flags not effective');
  modes.push({name, flags, observed, workload_jit_execution: name === 'interpreter' ? 'disabled by verified settings' : 'not measured; enabled settings are not execution evidence'});
}
fs.writeFileSync(outputArg, JSON.stringify({target, source_clean:true, binary_sha256:createHash('sha256').update(fs.readFileSync(binary)).digest('hex'), machine:{platform:os.platform(),architecture:os.arch(),release:os.release()}, modes, qualification:'Shell selection and nine CLI smoke checks per mode only; no timing, full Test262, workload JIT execution or Temiya conformance claim'}, null, 2)+'\n');
