const fs = require('fs'), cp = require('child_process'), crypto = require('crypto');
const directory = '/tmp/temiya-147-perf/functional';
fs.mkdirSync(directory, { recursive: true });
const executable = '/tmp/temiya-gc-migration-target/debug/temiya-test262';
const corpus = '/tmp/tmp.SEtioL4USh/test262';
const revision = cp.execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
const sha = crypto.createHash('sha256').update(fs.readFileSync(executable)).digest('hex');
function run(name, selection, backend) {
  const result = cp.spawnSync(executable, ['--corpus', corpus, '--pinned', '--backend', backend, ...selection], { encoding: 'utf8', timeout: 60000 });
  if (result.error) throw result.error;
  const report = JSON.parse(result.stdout);
  Object.assign(report, { source_revision: revision, executable_sha256: sha, build_profile: 'debug', features: ['jit'], selection, selection_scope: 'Fixed original files only. Failures, skips and unexecuted variants are never passes; no whole-feature/corpus qualification.' });
  fs.writeFileSync(directory + '/' + name + '.json', JSON.stringify(report, null, 2) + '\n');
  console.log(name, { exit: result.status, counts: report.counts });
}
for (const name of ['phase-regressions', 'harness-dependencies-ast', 'harness-dependencies-auto', 'all-prior-failures']) {
  const old = JSON.parse(fs.readFileSync('/tmp/temiya-145-cli-perf/functional/' + name + '.json'));
  run(name, old.selection, old.backend.toLowerCase());
}
const selected = [
  'test/built-ins/Array/length/define-own-prop-length-overflow-realm.js',
  ...['detached-buffer.js', 'invoked-as-accessor.js', 'invoked-as-func.js', 'length.js', 'name.js', 'prop-desc.js', 'return-bytelength.js', 'this-is-not-object.js'].map(name => 'test/built-ins/ArrayBuffer/prototype/byteLength/' + name),
  ...['script-decl-func-dups.js', 'script-decl-func-err-non-configurable.js', 'script-decl-func-err-non-extensible.js', 'script-decl-func.js', 'script-decl-lex-restricted-global.js', 'script-decl-lex-var.js', 'script-decl-var-err.js', 'script-decl-var.js'].map(name => 'test/language/global-code/' + name),
];
for (const backend of ['ast', 'auto']) run('host-regressions-' + backend, selected, backend);
const list = path => cp.execFileSync('git', ['-C', corpus, 'ls-tree', '-r', '--name-only', 'd53ebc8b1931d9f1541e43d59650776e4b766248', path], { encoding: 'utf8' }).trim().split('\n');
run('byteLength-full', list('test/built-ins/ArrayBuffer/prototype/byteLength'), 'auto');
run('global-script-declarations', list('test/language/global-code').filter(file => /\/script-decl-(lex|var|func)/.test(file)), 'auto');
