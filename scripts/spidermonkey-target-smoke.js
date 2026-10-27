// Executed by the pinned SpiderMonkey shell, never by the benchmark host.
(function () {
  const build = getBuildConfiguration();
  assertEq(build.debug, false);
  assertEq(build['intl-api'], true);
  assertEq((function (a, b) { return a + b; })(20, 22), 42);
  assertEq(Object.is(-0 % 7, -0), true);
  assertEq(Number.isNaN(0 / 0), true);
  assertEq('😀'.length, 2);
  assertEq(new Intl.NumberFormat('en-US', {useGrouping: false}).format(42), '42');
  let typeError = false, syntaxError = false;
  try { null.x; } catch (error) { typeError = error instanceof TypeError; }
  try { eval('let = ;'); } catch (error) { syntaxError = error instanceof SyntaxError; }
  assertEq(typeError, true);
  assertEq(syntaxError, true);
  print(JSON.stringify({build, jit_options: getJitCompilerOptions(), checks: 9}));
})();
