// Exercise validation through real worker binaries, including a failure after
// a correct warmup. No synthetic result is accepted as engine performance.
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import assert from 'node:assert/strict';

const workers = process.argv.slice(2);
if (!workers.length) throw Error('usage: check-workload-workers.mjs WORKER [WORKER ...]');
const directory = mkdtempSync(join(tmpdir(), 'temiya-worker-check-'));
try {
  const cases = [
    { name: 'correct', setup: 'var value = 42;', invocation: 'value;', expected: 42, success: true },
    { name: 'wrong-warmup', setup: 'var value = 42;', invocation: 'value;', expected: 41, success: false },
    { name: 'wrong-after-warmup', setup: 'var calls = 0;', invocation: '++calls;', expected: 1, success: false },
    { name: 'throw-after-warmup', setup: 'var calls = 0;', invocation: "if (++calls > 1) throw 'later'; 1;", expected: 1, success: false },
    { name: 'wrong-type', setup: '', invocation: "'42';", expected: 42, success: false },
  ];
  for (const worker of workers) {
    for (const test of cases) {
      const file = join(directory, `${test.name}.json`);
      writeFileSync(file, JSON.stringify(test));
      const result = spawnSync(worker, [file, '2'], { encoding: 'utf8', timeout: 10_000 });
      assert.equal(Boolean(result.error), false, `${worker} ${test.name}: ${result.error}`);
      if (test.success) {
        assert.equal(result.status, 0, result.stderr);
        const output = JSON.parse(result.stdout);
        assert.equal(output.validated_passes, 3);
        assert.equal(output.expected, 42);
        assert.equal(output.samples_ns.length, 2);
        assert.equal(output.scope, 'parse+evaluate invocation; function setup excluded');
      } else {
        assert.notEqual(result.status, 0, `${test.name} silently accepted`);
        assert.equal(result.stdout, '', `${test.name} emitted a timing report`);
      }
    }
    console.log(`PASS worker correctness validation: ${worker}`);
  }
} finally {
  rmSync(directory, { recursive: true });
}
