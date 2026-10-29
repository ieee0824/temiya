// Run with the verified pinned native shell, not a browser/Node substitution.
(function () {
  'use strict';
  const [requestPath, mode] = scriptArgs;
  if (scriptArgs.length !== 2 || !['interpreter', 'default'].includes(mode))
    throw Error('usage: SHELL FLAGS WORKER REQUEST_JSON interpreter|default');
  const jit = getJitCompilerOptions();
  if (mode === 'interpreter' && ['ion.enable', 'baseline.enable', 'blinterp.enable'].some(k => jit[k] !== 0))
    throw Error('interpreter flags not effective');
  const request = JSON.parse(read(requestPath));
  if (typeof request.source !== 'string') throw Error('source must be a string');
  const stringify = JSON.stringify, run = evaluate, checkpoint = drainJobQueue;
  const getPrototype = Object.getPrototypeOf;
  const ownKeys = Reflect.ownKeys, descriptorOf = Object.getOwnPropertyDescriptor;
  const extensible = Object.isExtensible, isArray = Array.isArray;
  const objectPrototype = Object.prototype, arrayPrototype = Array.prototype;
  const errorTypes = [TypeError, SyntaxError, ReferenceError, RangeError, EvalError, URIError, Error]
    .map(type => [type.name, type.prototype]);
  const effects = [], jobs = [], jobErrors = [], events = [], symbols = new Map(), objects = new Map();
  const numberBytes = new DataView(new ArrayBuffer(8));
  const charCode = Function.call.bind(String.prototype.charCodeAt);
  let eligible = true;
  function utf16(text) {
    const units = [];
    for (let i = 0; i < text.length; i++) units.push(charCode(text, i));
    return units;
  }
  function value(item) {
    return valueAt(item, [], {properties: 4096});
  }
  function unsupported(kind) {
    eligible = false;
    return {type: 'unsupported', kind};
  }
  function objectValue(item, active, budget) {
    if (typeof item === 'function') return unsupported('function');
    if (!objects.has(item)) {
      if (objects.size >= 1024) return unsupported('identity-limit');
      objects.set(item, objects.size);
    }
    const id = objects.get(item);
    if (active.includes(item)) return {type: 'reference', id};
    if (active.length >= 32) return unsupported('depth-limit');
    let prototype, keys;
    try {
      const observed = getPrototype(item);
      if (observed === null) prototype = null;
      else if (observed === objectPrototype) prototype = 'Object';
      else if (observed === arrayPrototype) prototype = 'Array';
      else return unsupported('object-prototype');
    } catch (_) { return unsupported('prototype-reflection'); }
    try { keys = ownKeys(item); } catch (_) { return unsupported('property-reflection'); }
    if (keys.length > budget.properties) return unsupported('property-limit');
    budget.properties -= keys.length;
    active.push(item);
    const properties = [];
    for (const key of keys) {
      const observedKey = valueAt(key, active, budget);
      let descriptor;
      try {
        const d = descriptorOf(item, key);
        if (!d) descriptor = unsupported('descriptor-reflection');
        else if ('get' in d || 'set' in d) {
          unsupported('accessor');
          descriptor = {kind: 'accessor', get: d.get !== undefined, set: d.set !== undefined};
        } else descriptor = {kind: 'data', value: valueAt(d.value, active, budget),
          writable: d.writable, enumerable: d.enumerable, configurable: d.configurable};
      } catch (_) { descriptor = unsupported('descriptor-reflection'); }
      properties.push({key: observedKey, descriptor});
    }
    active.pop();
    let isExtensible;
    try { isExtensible = extensible(item); } catch (_) { isExtensible = unsupported('extensibility-reflection'); }
    return {type: 'object', id, kind: isArray(item) ? 'array' : 'ordinary',
      prototype, extensible: isExtensible, properties};
  }
  function valueAt(item, active, budget) {
    if (item === undefined) return {type: 'undefined'};
    if (item === null) return {type: 'null'};
    switch (typeof item) {
      case 'boolean': return {type: 'boolean', value: item};
      case 'string': return {type: 'string', utf16: utf16(item)};
      case 'number': {
        numberBytes.setFloat64(0, item, false);
        const bits = Number.isNaN(item) ? 'NaN' :
          numberBytes.getUint32(0, false).toString(16).padStart(8, '0') +
          numberBytes.getUint32(4, false).toString(16).padStart(8, '0');
        return {type: 'number', bits};
      }
      case 'bigint': return {type: 'bigint', decimal: item.toString()};
      case 'symbol': {
        if (!symbols.has(item)) symbols.set(item, symbols.size);
        return {type: 'symbol', id: symbols.get(item),
          description_utf16: item.description === undefined ? null : utf16(item.description)};
      }
      default:
        return objectValue(item, active, budget);
    }
  }
  function exception(error) {
    if (error !== null && (typeof error === 'object' || typeof error === 'function')) {
      // Use captured prototype identities, never a user-defined .name getter.
      let current = error;
      for (let i = 0; i < 32 && current !== null; i++) {
        current = getPrototype(current);
        const known = errorTypes.find(([, prototype]) => prototype === current);
        if (known) return {type: known[0], thrown: null};
      }
    }
    return {type: 'ThrownValue', thrown: value(error)};
  }
  globalThis.record = function (...items) {
    const values = items.map(value);
    effects.push(values);
    events.push({kind: 'effect', values});
  };
  const resolved = Promise.resolve();
  const then = Function.call.bind(Promise.prototype.then);
  globalThis.queueJob = function (label, callback) {
    if (typeof label !== 'string' || typeof callback !== 'function')
      throw new TypeError('queueJob requires a string label and callable callback');
    then(resolved, function () {
      const units = utf16(label);
      jobs.push(units);
      events.push({kind: 'job', label_utf16: units});
      try { callback(); }
      catch (error) {
        const observed = exception(error);
        jobErrors.push(observed);
        events.push({kind: 'job-error', error: observed});
      }
    });
  };
  const pendingRejections = new Map();
  setPromiseRejectionTrackerCallback(function (promise, state) {
    if (state === 0) pendingRejections.set(promise, true);
    else pendingRejections.delete(promise);
  });
  let completion;
  try { run(request.source, {execute: false}); }
  catch (error) { completion = {kind: 'compile-error', error: exception(error)}; }
  if (!completion) {
    try { completion = {kind: 'normal', value: value(run(request.source))}; }
    catch (error) { completion = {kind: 'throw', error: exception(error)}; }
  }
  checkpoint();
  // Observe still-unhandled reasons after the user checkpoint. Use the actual
  // Promise reaction API; do not rely on an unverified shell result accessor.
  for (const promise of [...pendingRejections.keys()]) {
    then(promise, undefined, function (reason) {
      const observed = exception(reason);
      jobErrors.push(observed);
      events.push({kind: 'unhandled-rejection', error: observed});
    });
  }
  checkpoint();
  print(stringify({schema: 1, kind: 'differential-observation', engine: 'SpiderMonkey',
    mode, jit_options: jit, comparison_eligible: eligible, completion,
    effects, jobs, job_errors: jobErrors, events}));
})();
