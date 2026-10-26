// Extract the unmodified array function body from the pinned application fixture.
// stdout is a standalone Script; stderr records provenance. Requires Node.js.
import { spawnSync } from 'node:child_process';
import { createContext, runInContext } from 'node:vm';
import { createHash } from 'node:crypto';

const revision = '007556adccb8fb4329236932ca8175f8e8d119d2';
if (process.argv.length !== 3) throw Error('usage: node scripts/extract-omoikane-array.mjs OMOIKANE_CHECKOUT');
function read(path) {
  const result = spawnSync('git', ['-C', process.argv[2], 'show', `${revision}:${path}`], { encoding: 'utf8' });
  if (result.status !== 0) throw Error(result.stderr);
  return result.stdout;
}
const source = read('tests/js_benchmark/shapes.js');
const license = read('LICENSE');
const context = createContext({});
runInContext(source, context, { timeout: 1000 });
let selected;
context.bench = (name, iterations, body) => {
  if (name === 'array') {
    if (selected) throw Error('duplicate array workload');
    selected = { iterations, body: Function.prototype.toString.call(body) };
  }
  return name;
};
// Capture bodies without running any timed workload in Node.js.
runInContext('runBenchmarks()', context, { timeout: 1000 });
if (context.BENCH_FIXTURE_VERSION !== 2 || selected?.iterations !== 500000 ||
    context.BENCH_EXPECTED.array[0] !== 500000 || context.BENCH_EXPECTED.array[1] !== 124999750000) {
  throw Error('unexpected fixture or expected result');
}
const script = `/* Derived from ieee0824/omoikane@${revision}\n${license}*/\n(${selected.body})(${selected.iterations});\n`;
const sha256 = text => createHash('sha256').update(text).digest('hex');
process.stderr.write(JSON.stringify({ revision, fixture_version: 2, workload: 'array',
  iterations: selected.iterations, expected: 124999750000,
  fixture_sha256: sha256(source), script_sha256: sha256(script),
  scope: 'Exact function body and iteration count; original timing harness replaced by host measurement',
}) + '\n');
process.stdout.write(script);
