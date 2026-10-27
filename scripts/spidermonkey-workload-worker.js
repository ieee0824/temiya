// Run only with the pinned native SpiderMonkey shell. Each evaluate parses a
// fresh Script; no CacheEntry or loadBytecode shortcut is used.
(function () {
  const [mode, workloadPath, samplesText] = scriptArgs;
  const samples = Number(samplesText);
  if (scriptArgs.length !== 3 || !['interpreter', 'default'].includes(mode) ||
      !Number.isInteger(samples) || samples < 1 || samples > 1000) {
    throw Error('usage: SHELL [FLAGS] WORKER MODE WORKLOAD_JSON SAMPLES');
  }
  const build = getBuildConfiguration(), jitOptions = getJitCompilerOptions();
  if (build.debug) throw Error('optimized non-debug shell required');
  if (mode === 'interpreter' && ['ion.enable', 'baseline.enable', 'blinterp.enable'].some(k => jitOptions[k] !== 0)) {
    throw Error('interpreter flags not effective');
  }
  const workload = JSON.parse(read(workloadPath));
  if (typeof workload.setup !== 'string' || typeof workload.invocation !== 'string' ||
      !Number.isFinite(workload.expected)) throw Error('invalid workload');
  const start = dateNow();
  evaluate(workload.setup);
  const setup_ns = Math.round((dateNow() - start) * 1e6);
  function run() {
    const start = dateNow();
    const value = evaluate(workload.invocation);
    const ns = Math.round((dateNow() - start) * 1e6);
    if (!Object.is(value, workload.expected)) throw Error('incorrect workload result: ' + value);
    if (!Number.isSafeInteger(ns) || ns <= 0) throw Error('non-positive or invalid clock interval');
    return ns;
  }
  const warmup_ns = run(), samples_ns = [];
  for (let i = 0; i < samples; i++) samples_ns.push(run());
  print(JSON.stringify({
    engine: 'SpiderMonkey', mode, profile: 'release',
    scope: 'parse+evaluate invocation; function setup excluded',
    expected: workload.expected, validated_passes: samples + 1,
    startup_ns: null, setup_ns, warmup_ns, samples_ns,
    startup_note: 'Native process startup excluded; measured by parent only if separately reported',
    clock: 'shell dateNow: sub-ms wall clock; milliseconds converted to integer ns; non-positive intervals rejected',
    jit_options: jitOptions,
    workload_jit_execution: mode === 'interpreter' ? 'disabled by verified settings' : 'not measured; enabled settings are not execution evidence',
    sandbox: null,
  }));
})();
