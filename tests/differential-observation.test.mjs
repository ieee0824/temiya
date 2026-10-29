import test from 'node:test';
import assert from 'node:assert/strict';
import {validateObservation} from '../scripts/differential-observation.mjs';
const observation = value => ({schema: 1, kind: 'differential-observation', comparison_eligible: true,
  completion: {kind: 'normal', value}, effects: [], jobs: [], job_errors: [], events: []});
const undefinedValue = {type: 'undefined'};
const data = (id, properties = []) => ({type: 'object', id, kind: 'ordinary', prototype: 'Object',
  extensible: true, properties});
const property = (name, value) => ({key: {type: 'string', utf16: [name.charCodeAt(0)]},
  descriptor: {kind: 'data', value, writable: true, enumerable: true, configurable: true}});

test('missing values cannot match each other as successful observations', () => {
  validateObservation(observation(undefinedValue));
  assert.throws(() => validateObservation(observation(undefined)), /typed value/);
  assert.throws(() => validateObservation(observation({type: 'undefined', ignored: 'extra'})), /fields/);
  assert.throws(() => validateObservation(observation({type: 'unknown'})), /unknown value/);
});
test('binary64 and UTF-16 preserve their exact normalized domains', () => {
  for (const bits of ['0000000000000000', '8000000000000000', '7ff0000000000000', 'NaN'])
    validateObservation(observation({type: 'number', bits}));
  for (const bits of ['nan', '7ff8000000000001', 'short', 1234567890123456])
    assert.throws(() => validateObservation(observation({type: 'number', bits})));
  validateObservation(observation({type: 'string', utf16: [0, 55296, 65535]}));
  assert.throws(() => validateObservation(observation({type: 'string', utf16: [65536]})), /UTF-16/);
  assert.throws(() => validateObservation(observation({type: 'bigint', decimal: 1})));
  assert.throws(() => validateObservation(observation({type: 'bigint', decimal: '-0'})));
});
test('identity references require a real graph definition and immutable brands', () => {
  validateObservation(observation(data(0, [property('x', {type: 'reference', id: 0})])));
  assert.throws(() => validateObservation(observation({type: 'reference', id: 7})), /no observed object/);
  assert.throws(() => validateObservation(observation(data(0, [property('x', data(0))]))), /cycle/);
  const changed = observation(data(0));
  changed.effects = [[{...data(0), kind: 'array'}]];
  changed.events = [{kind: 'effect', values: changed.effects[0]}];
  assert.throws(() => validateObservation(changed), /object kind changed/);
  assert.throws(() => validateObservation(observation(data(0, [property('x', undefinedValue),
    property('x', undefinedValue)]))), /duplicate property/);
});
test('symbol identity cannot change its UTF-16 description', () => {
  const changed = observation({type: 'symbol', id: 0, description_utf16: [97]});
  changed.effects = [[{type: 'symbol', id: 0, description_utf16: [98]}]];
  changed.events = [{kind: 'effect', values: changed.effects[0]}];
  assert.throws(() => validateObservation(changed), /symbol identity changed/);
});
test('side effects and job logs must reproduce the ordered event projection', () => {
  const base = observation(undefinedValue);
  base.effects = [[undefinedValue]];
  assert.throws(() => validateObservation(base), /effect\/event/);
  base.events = [{kind: 'effect', values: base.effects[0]}];
  validateObservation(base);
  base.jobs = [[97], [98]];
  base.events.push({kind: 'job', label_utf16: [98]}, {kind: 'job', label_utf16: [97]});
  assert.throws(() => validateObservation(base), /job\/event/);
});
test('unsupported accessors and Engine faults cannot claim comparison eligibility', () => {
  const accessor = observation(data(0, [{key: {type: 'string', utf16: [120]},
    descriptor: {kind: 'accessor', get: true, set: false}}]));
  assert.throws(() => validateObservation(accessor), /unsupported/);
  accessor.comparison_eligible = false; validateObservation(accessor);
  const fault = observation(undefinedValue);
  fault.completion = {kind: 'throw', error: {type: 'InternalError', thrown: null}};
  assert.throws(() => validateObservation(fault), /unsupported/);
  fault.comparison_eligible = false; validateObservation(fault);
  fault.completion.error = {type: 'ThrownValue', thrown: null};
  assert.throws(() => validateObservation(fault), /missing thrown value/);
});
test('depth and property budgets reject structurally unbounded snapshots', () => {
  let nested = data(32);
  for (let id = 31; id >= 0; id--) nested = data(id, [property('x', nested)]);
  assert.throws(() => validateObservation(observation(nested)), /depth limit/);
  assert.throws(() => validateObservation(observation(data(0,
    Array.from({length: 4097}, () => property('x', undefinedValue))))), /property limit/);
});
