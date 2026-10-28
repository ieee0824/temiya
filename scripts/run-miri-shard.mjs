// Split the actual Miri test listing without dropping, rewriting or ignoring tests.
import {execFileSync} from 'node:child_process';

const [shardText, countText, mode] = process.argv.slice(2);
const shard = Number(shardText), count = Number(countText);
if (process.argv.length > 5 || !/^\d+$/.test(shardText ?? '') || !/^\d+$/.test(countText ?? '') ||
    !Number.isSafeInteger(shard) || !Number.isSafeInteger(count) || count < 1 || count > 32 ||
    shard < 0 || shard >= count || (mode !== undefined && mode !== '--plan')) {
  throw Error('usage: run-miri-shard.mjs SHARD COUNT [--plan]');
}
const cargo = ['+nightly-2026-10-20', 'miri', 'test', '--locked', '--lib'];
const listing = execFileSync('cargo', [...cargo, '--', '--list'], {
  encoding: 'utf8', maxBuffer: 16 * 1024 * 1024, stdio: ['ignore', 'pipe', 'inherit'],
});
const tests = [...listing.matchAll(/^(\S+): test\r?$/gm)].map(match => match[1]).sort();
const total = listing.match(/^(\d+) tests?, \d+ benchmarks?\r?$/m);
if (!total || +total[1] !== tests.length || new Set(tests).size !== tests.length || !tests.length) {
  throw Error('incomplete or duplicate Miri test listing');
}
const selected = tests.filter((_, index) => index % count === shard);
if (!selected.length) throw Error('empty Miri shard');
console.log(JSON.stringify({shard, count, available: tests.length, selected}));
if (mode !== '--plan') {
  for (const name of selected) {
    console.log(`Running exact Miri test: ${name}`);
    const output = execFileSync('cargo', [...cargo, name, '--', '--exact', '--nocapture'], {
      encoding: 'utf8', maxBuffer: 16 * 1024 * 1024, stdio: ['ignore', 'pipe', 'inherit'],
    });
    process.stdout.write(output);
    if (!/^test result: ok\. 1 passed; 0 failed; 0 ignored;/m.test(output)) {
      throw Error(`Miri test did not execute successfully: ${name}`);
    }
  }
  console.log(JSON.stringify({shard, passed: selected.length, skipped: 0}));
}
