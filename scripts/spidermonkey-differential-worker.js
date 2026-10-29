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
  const errorTypes = [TypeError, SyntaxError, ReferenceError, RangeError, EvalError, URIError, Error]
    .map(type => [type.name, type.prototype]);
  const effects = [], jobs = [], jobErrors = [], events = [], symbols = new Map();
  const numberBytes = new DataView(new ArrayBuffer(8));
  const charCode = Function.call.bind(String.prototype.charCodeAt);
  let eligible = true;
  function utf16(text) {
    const units = [];
    for (let i = 0; i < text.length; i++) units.push(charCode(text, i));
    return units;
  }
  function value(item) {
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
        eligible = false;
        return {type: 'unsupported', kind: 'object'};
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
