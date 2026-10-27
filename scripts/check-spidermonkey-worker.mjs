// Exercise the actual native shell adapter, including failure propagation.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const [shell] = process.argv.slice(2);
if (!shell) throw Error('usage: check-spidermonkey-worker.mjs NATIVE_SHELL');
const target = JSON.parse(fs.readFileSync(new URL('../benchmarks/targets/spidermonkey.json', import.meta.url)));
const adapter = fileURLToPath(new URL('./spidermonkey-workload-worker.js', import.meta.url));
const root = fs.mkdtempSync(path.join(os.tmpdir(), 'SpiderMonkey worker check '));
const cases = [
  {name:'normal', setup:'function f(){var n=0;for(var i=0;i<10000;i++)n++;return n;}', invocation:'f();', expected:10000, pass:true},
  {name:'negative-zero', setup:'function f(){var n=0;for(var i=0;i<10000;i++)n++;return -0;}', invocation:'f();', expected:-0, pass:true},
  {name:'wrong-result', setup:'', invocation:'41;', expected:42, pass:false},
  {name:'signed-zero-mismatch', setup:'', invocation:'-0;', expected:0, pass:false},
  {name:'syntax-error', setup:'', invocation:'let = ;', expected:42, pass:false},
  {name:'runtime-error', setup:'', invocation:'null.x;', expected:42, pass:false},
];
const checks = [];
for (const [mode, flags] of Object.entries(target.modes)) {
  for (const fixture of cases) {
    const file = path.join(root, fixture.name+'.json');
    // JSON.stringify(-0) is 0; preserve the actual signed-zero boundary input.
    const json = JSON.stringify(fixture);
    fs.writeFileSync(file, Object.is(fixture.expected,-0) ? json.replace('"expected":0','"expected":-0') : json);
    const result = spawnSync(shell, [...flags, adapter, mode, file, '2'], {encoding:'utf8',timeout:60000});
    if (result.error || result.signal || (result.status === 0) !== fixture.pass) {
      throw Error(`${mode}/${fixture.name}: ${result.error ?? result.stderr ?? result.status}`);
    }
    if (fixture.pass) {
      const report = JSON.parse(result.stdout);
      if (report.validated_passes !== 3 || report.samples_ns.length !== 2 || report.mode !== mode ||
          report.samples_ns.some(n => !Number.isSafeInteger(n) || n <= 0)) throw Error('invalid worker report');
    }
    checks.push({mode, name:fixture.name, expected_success:fixture.pass, exit_status:result.status});
  }
}
console.log(JSON.stringify({checks},null,2));
