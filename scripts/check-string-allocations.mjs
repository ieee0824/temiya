// Assert observable allocation budgets, not private storage variant names.
import { readFileSync } from 'node:fs';
const [path] = process.argv.slice(2);
if (!path) throw Error('usage: check-string-allocations.mjs PROBE_JSON');
const result = JSON.parse(readFileSync(path));
if (!Number.isSafeInteger(result.iterations) || result.iterations < 1 ||
    result.js_string_bytes !== result.vec_bytes || result.timing_instrumentation !== false ||
    result.cases?.length !== 3) throw Error('invalid allocation observation');
for (const name of ['tiny', 'short', 'long']) {
  const row = result.cases.find(row => row.case === name);
  if (!row || row.vec_clone.allocation_requests < result.iterations ||
      row.vec_construct.allocation_requests < result.iterations || row.compact_clone.allocation_requests !== 0 ||
      row.compact_clone.requested_bytes !== 0) throw Error(`clone allocation budget failed: ${name}`);
  if ((name === 'tiny' || (name === 'short' && result.pointer_width === 64)) &&
      (row.compact_construct.allocation_requests !== 0 || row.compact_construct.requested_bytes !== 0)) {
    throw Error(`short construction allocation budget failed: ${name}`);
  }
}
console.log('PASS compact UTF-16 construction/clone allocation budgets and footprint');
