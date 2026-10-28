import fs from 'node:fs';
import {createHash} from 'node:crypto';

const [logPath, statusPath, expectedHead, outputPath] = process.argv.slice(2);
const check = (ok, reason) => { if (!ok) throw Error(reason); };
const log = fs.readFileSync(logPath, 'utf8');
const status = JSON.parse(fs.readFileSync(statusPath));
check(status.headSha === expectedHead && status.status === 'completed' && status.conclusion === 'success', 'source/status');
check(status.jobs.length === 19 && status.jobs.every(j => j.status === 'completed' && j.conclusion === 'success'), 'all jobs');
const shards = new Map();
for (const line of log.split('\n')) {
  const parts = line.split('\t');
  const match = parts[0].match(/^Miri library tests \(shard (\d+)\)$/);
  if (!match || parts[1] !== 'Check library tests under Miri') continue;
  const id = Number(match[1]);
  const shard = shards.get(id) ?? {id, plan:null, current:null, executed:[], actualNames:[]};
  shards.set(id, shard);
  const msg = parts.slice(2).join('\t').replace(/^\S+\s/, '').replace(/\x1b\[[0-9;]*m/g, '').trim();
  if (msg.startsWith('{"shard":')) {
    const value = JSON.parse(msg);
    if (value.selected) {
      check(!shard.plan && value.shard === id && value.count === 16 && value.available === 119, 'plan');
      shard.plan = value;
    } else if (value.passed !== undefined) {
      check(value.shard === id && value.passed === shard.executed.length && value.skipped === 0, 'summary');
      shard.summary = value;
    }
  } else if (msg.startsWith('Running exact Miri test: ')) {
    check(shard.plan && !shard.current, 'test sequence');
    shard.current = msg.slice('Running exact Miri test: '.length);
    check(shard.current === shard.plan.selected[shard.executed.length], 'selection order');
    shard.actualNames = [];
  } else {
    const result = msg.match(/^test (\S+) \.\.\. ok$/);
    if (result) shard.actualNames.push(result[1]);
    if (msg.startsWith('test result:')) {
      check(shard.current && /^test result: ok\. 1 passed; 0 failed; 0 ignored;/.test(msg), 'exact result');
      check(shard.actualNames.length === 1 && shard.actualNames[0] === shard.current, 'actual test name');
      shard.executed.push(shard.current);
      shard.current = null;
    }
  }
}
check(shards.size === 16, 'sixteen shards');
const names = [...shards.values()].flatMap(s => s.executed);
const unique = new Set(names);
check(names.length === 119 && unique.size === 119, '119 distinct exact tests');
const all = [...unique].sort();
for (let id = 0; id < 16; id++) {
  const shard = shards.get(id);
  check(shard && !shard.current && shard.summary &&
    JSON.stringify(shard.plan.selected) === JSON.stringify(all.filter((_, n) => n % 16 === id)) &&
    shard.executed.length === shard.plan.selected.length, 'complete partition');
}
const report = {
  verified_at:new Date().toISOString(), source_revision:status.headSha,
  run_id:status.databaseId, run_url:status.url, conclusion:status.conclusion,
  miri:{distinct_tests:119,exact_success_results:119,skipped:0,
    plans:[...shards.values()].sort((a,b) => a.id-b.id).map(s => s.plan),
    executed_tests:all,all_shards_and_aggregate:'success'},
  asan:'success',dependency_audit:'success',
  full_log_sha256:createHash('sha256').update(log).digest('hex'),
  scope:'Exact library Miri proof for the identified source only. Native cfg(not(miri)) diagnostics and external integrations have separate evidence.'
};
fs.writeFileSync(outputPath, JSON.stringify(report, null, 2)+'\n');
console.log(JSON.stringify({source:expectedHead,distinct:119,exact_passed:119,ignored:0,all19jobs:'success'}));
