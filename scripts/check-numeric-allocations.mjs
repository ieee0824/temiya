import {readFileSync} from 'node:fs';
const [path] = process.argv.slice(2);
if (!path) throw Error('usage: check-numeric-allocations.mjs PROBE_JSON');
const r = JSON.parse(readFileSync(path));
const names = ['decimal', 'separated-decimal', 'hexadecimal', 'separated-hexadecimal', 'legacy-octal', 'bigint', 'separated-bigint'];
if (!Number.isSafeInteger(r.iterations) || r.iterations < 1 || r.timing_instrumentation !== false || r.cases?.length !== names.length) throw Error('invalid observation');
for (const name of names) {
  const x = r.cases.find(x => x.case === name);
  if (!x || x.validated !== true || typeof x.source !== 'string' ||
      !['reference', 'current'].every(op => ['allocation_requests', 'requested_bytes'].every(key => Number.isSafeInteger(x[op]?.[key]) && x[op][key] >= 0)) ||
      x.reference.allocation_requests < r.iterations) throw Error(`invalid counts: ${name}`);
  if (name.startsWith('separated-')) {
    if (x.reference.allocation_requests !== x.current.allocation_requests || x.reference.requested_bytes !== x.current.requested_bytes) throw Error(`separator allocation changed: ${name}`);
  } else if (x.reference.allocation_requests - x.current.allocation_requests !== r.iterations ||
      x.reference.requested_bytes - x.current.requested_bytes < r.iterations * x.source.length) {
    throw Error(`normalization allocation not eliminated: ${name}`);
  }
}
console.log('PASS numeric normalization allocation budgets');
