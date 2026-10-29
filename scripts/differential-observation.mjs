// Validate the semantic protocol before equal JSON can establish a match.
import assert from 'node:assert/strict';
export const canonicalObservation = value => JSON.stringify(value, (_, item) =>
  item && typeof item === 'object' && !Array.isArray(item)
    ? Object.fromEntries(Object.entries(item).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)) : item);
const keys = (object, expected) => {
  assert(object && typeof object === 'object' && !Array.isArray(object), 'record required');
  assert.deepEqual(Object.keys(object).sort(), [...expected].sort(), 'invalid semantic record fields');
};
const utf16 = units => assert(Array.isArray(units) && units.every(unit =>
  Number.isInteger(unit) && unit >= 0 && unit <= 65535), 'invalid UTF-16 code units');
const identity = id => assert(Number.isSafeInteger(id) && id >= 0, 'invalid identity');

export function validateObservation(report) {
  assert.equal(report.schema, 1);
  assert.equal(report.kind, 'differential-observation');
  assert.equal(typeof report.comparison_eligible, 'boolean');
  for (const name of ['effects', 'jobs', 'job_errors', 'events']) assert(Array.isArray(report[name]));
  let unsupported = false;
  const objects = new Map(), symbols = new Map(), references = new Set();
  function value(node, active = new Set(), budget = {properties: 4096}) {
    assert(node && typeof node.type === 'string', 'typed value required');
    switch (node.type) {
      case 'undefined': case 'null': keys(node, ['type']); break;
      case 'boolean': keys(node, ['type', 'value']); assert.equal(typeof node.value, 'boolean'); break;
      case 'string': keys(node, ['type', 'utf16']); utf16(node.utf16); break;
      case 'number': {
        keys(node, ['type', 'bits']);
        assert.equal(typeof node.bits, 'string');
        assert(node.bits === 'NaN' || /^[a-f0-9]{16}$/.test(node.bits), 'invalid binary64 encoding');
        if (node.bits !== 'NaN') {
          const bits = BigInt('0x' + node.bits);
          assert((bits & 0x7ff0000000000000n) !== 0x7ff0000000000000n ||
            (bits & 0x000fffffffffffffn) === 0n, 'noncanonical NaN');
        }
        break;
      }
      case 'bigint': keys(node, ['type', 'decimal']); assert.equal(typeof node.decimal, 'string'); assert(/^(0|-?[1-9][0-9]*)$/.test(node.decimal)); break;
      case 'symbol': {
        keys(node, ['type', 'id', 'description_utf16']); identity(node.id);
        if (node.description_utf16 !== null) utf16(node.description_utf16);
        const description = canonicalObservation(node.description_utf16);
        if (symbols.has(node.id)) assert.equal(symbols.get(node.id), description, 'symbol identity changed');
        symbols.set(node.id, description); break;
      }
      case 'unsupported':
        keys(node, ['type', 'kind']); assert(typeof node.kind === 'string' && node.kind); unsupported = true; break;
      case 'reference': keys(node, ['type', 'id']); identity(node.id); references.add(node.id); break;
      case 'object': {
        keys(node, ['type', 'id', 'kind', 'prototype', 'extensible', 'properties']); identity(node.id);
        assert(node.id < 1024 && active.size < 32, 'snapshot identity/depth limit');
        assert(!active.has(node.id), 'cycle must use a reference');
        assert(['ordinary', 'array'].includes(node.kind));
        assert(node.prototype === null || ['Object', 'Array'].includes(node.prototype));
        if (objects.has(node.id)) assert.equal(objects.get(node.id), node.kind, 'object kind changed');
        objects.set(node.id, node.kind);
        if (typeof node.extensible !== 'boolean') {
          assert.equal(node.extensible?.type, 'unsupported'); value(node.extensible);
        }
        assert(Array.isArray(node.properties) && node.properties.length <= budget.properties, 'snapshot property limit');
        budget.properties -= node.properties.length;
        active.add(node.id);
        const names = new Set();
        for (const property of node.properties) {
          keys(property, ['key', 'descriptor']);
          assert(['string', 'symbol'].includes(property.key?.type), 'invalid property key');
          value(property.key, active, budget);
          const name = property.key.type === 'symbol' ? 'symbol:' + property.key.id : canonicalObservation(property.key);
          assert(!names.has(name), 'duplicate property key'); names.add(name);
          const descriptor = property.descriptor;
          if (descriptor?.type === 'unsupported') { value(descriptor); continue; }
          if (descriptor?.kind === 'accessor') {
            keys(descriptor, ['kind', 'get', 'set']);
            assert.equal(typeof descriptor.get, 'boolean'); assert.equal(typeof descriptor.set, 'boolean');
            unsupported = true;
          } else {
            keys(descriptor, ['kind', 'value', 'writable', 'enumerable', 'configurable']);
            assert.equal(descriptor.kind, 'data');
            for (const flag of ['writable', 'enumerable', 'configurable']) assert.equal(typeof descriptor[flag], 'boolean');
            value(descriptor.value, active, budget);
          }
        }
        active.delete(node.id); break;
      }
      default: throw Error('unknown value type: ' + node.type);
    }
  }
  function error(node) {
    keys(node, ['type', 'thrown']);
    assert(['ThrownValue', 'TypeError', 'SyntaxError', 'ReferenceError', 'RangeError', 'EvalError',
      'URIError', 'Error', 'AggregateError', 'Engine', 'InternalError'].includes(node.type), 'unknown exception type');
    if (node.type === 'ThrownValue') assert(node.thrown !== null, 'missing thrown value');
    if (node.type === 'Engine' || node.type === 'InternalError') unsupported = true;
    if (node.thrown !== null) value(node.thrown);
  }
  if (report.completion?.kind === 'normal') {
    keys(report.completion, ['kind', 'value']); value(report.completion.value);
  } else {
    keys(report.completion, ['kind', 'error']);
    assert(['throw', 'compile-error'].includes(report.completion.kind)); error(report.completion.error);
  }
  for (const values of report.effects) { assert(Array.isArray(values)); for (const item of values) value(item); }
  for (const job of report.jobs) utf16(job);
  for (const item of report.job_errors) error(item);
  const effects = [], jobs = [], errors = [];
  for (const event of report.events) {
    if (event.kind === 'effect') {
      keys(event, ['kind', 'values']); assert(Array.isArray(event.values));
      for (const item of event.values) value(item); effects.push(event.values);
    } else if (event.kind === 'job') {
      keys(event, ['kind', 'label_utf16']); utf16(event.label_utf16); jobs.push(event.label_utf16);
    } else {
      keys(event, ['kind', 'error']);
      assert(['job-error', 'unhandled-rejection', 'checkpoint-error'].includes(event.kind));
      error(event.error); errors.push(event.error);
    }
  }
  assert.equal(canonicalObservation(effects), canonicalObservation(report.effects), 'effect/event logs disagree');
  assert.equal(canonicalObservation(jobs), canonicalObservation(report.jobs), 'job/event logs disagree');
  assert.equal(canonicalObservation(errors), canonicalObservation(report.job_errors), 'job-error/event logs disagree');
  for (const id of references) assert(objects.has(id), 'reference has no observed object');
  assert(!unsupported || !report.comparison_eligible, 'unsupported observation claimed eligible');
  return {completion: report.completion, effects: report.effects,
    jobs: report.jobs, job_errors: report.job_errors, events: report.events};
}
