// Capture exact pinned bodies and their shared setup without executing loops.
// Each emitted JSON is consumed unchanged by both public embedding workers.
import { spawnSync } from 'node:child_process';
import { createContext, runInContext } from 'node:vm';
import { createHash } from 'node:crypto';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { revision, fixtureSha256, oracle } from './omoikane-benchmark-reference.mjs';

const [checkout, destination] = process.argv.slice(2);
if (!checkout || !destination) throw Error('usage: extract-omoikane-workloads.mjs CHECKOUT NEW_OUTPUT_DIRECTORY');
const read = path => {
  const result = spawnSync('git', ['-C', checkout, 'show', `${revision}:${path}`], { encoding: 'utf8' });
  if (result.status !== 0) throw Error(result.stderr);
  return result.stdout;
};
const source = read('tests/js_benchmark/shapes.js');
const license = read('LICENSE');
if (createHash('sha256').update(source).digest('hex') !== fixtureSha256) throw Error('fixture hash mismatch');
const context = createContext({});
runInContext(source, context, { timeout: 1000 });
const bodies = [];
context.bench = (name, iterations, body) => {
  bodies.push({ name, iterations, body: Function.prototype.toString.call(body) });
  return name;
};
runInContext('runBenchmarks()', context, { timeout: 1000 });
if (context.BENCH_FIXTURE_VERSION !== 2 || bodies.length !== oracle.length) throw Error('fixture changed');
const helpersStart = source.indexOf('function makeShapes()');
const helpersEnd = source.indexOf('globalThis.runBenchmarks =');
if (helpersStart < 0 || helpersEnd <= helpersStart) throw Error('missing shared setup');
const helpers = source.slice(helpersStart, helpersEnd);
const notice = `/* Derived from ieee0824/omoikane@${revision}\n${license}*/\n`;
const sha256 = text => createHash('sha256').update(text).digest('hex');
const workloads = bodies.map((body, index) => {
  const [name, iterations, expected] = oracle[index];
  if (body.name !== name || body.iterations !== iterations ||
      context.BENCH_EXPECTED[name][0] !== iterations || context.BENCH_EXPECTED[name][1] !== expected) {
    throw Error(`fixture disagrees with independent oracle for ${name}`);
  }
  // Instantiate the shared receiver once in a stable closure. In particular,
  // keep it out of the timed body and out of the application's global slots.
  const setup = `${notice}var __temiyaBenchmarkBody = (function () {\n${helpers}\n` +
    `if (Object.getPrototypeOf(protoReceiver) !== ProtoBase.prototype || typeof protoReceiver.at !== 'function' || protoReceiver.at(3) !== 3) throw 'invalid prototype fixture';\n` +
    `return ${body.body};\n})();\n`;
  const invocation = `__temiyaBenchmarkBody(${iterations});\n`;
  return { name, iterations, expected, setup, invocation,
    body_sha256: sha256(body.body), setup_sha256: sha256(setup), invocation_sha256: sha256(invocation) };
});
// Refuse an existing directory instead of silently replacing evidence.
mkdirSync(destination);
for (const workload of workloads) writeFileSync(join(destination, `${workload.name}.json`), JSON.stringify(workload, null, 2) + '\n', { flag: 'wx' });
const manifest = { revision, fixture_version: 2, fixture_sha256: sha256(source),
  oracle_sha256: sha256(read('tests/engine_benchmark_results.rs')),
  shared_setup_sha256: sha256(helpers),
  scope: 'Unmodified bodies and iteration counts; original makeShapes/ProtoBase/protoReceiver setup instantiated once in a stable closure, with prototype identity checked; host timers replace the JS harness',
  workloads: workloads.map(({ setup, invocation, ...metadata }) => metadata),
};
writeFileSync(join(destination, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n', { flag: 'wx' });
console.log(JSON.stringify(manifest));
