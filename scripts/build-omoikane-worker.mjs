// Build a standalone adapter while retaining the application's pinned lock and
// path dependencies. Python 3.11+ is needed for tomllib and the disk-space guard.
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { copyFileSync, mkdirSync, readFileSync, realpathSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { revision } from './omoikane-benchmark-reference.mjs';

const [checkoutArg, buildArg, targetArg, kind = 'sync'] = process.argv.slice(2);
if (!checkoutArg || !buildArg || !targetArg || !['sync','async'].includes(kind)) throw Error('usage: build-omoikane-worker.mjs CHECKOUT NEW_BUILD_DIR TARGET_DIR [sync|async]');
const checkout = realpathSync(checkoutArg), build = resolve(buildArg), target = resolve(targetArg);
const python = process.env.PYTHON ?? 'python3';
function run(command, args, options = {}) {
  const result = spawnSync(command, args, { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, ...options });
  if (result.status !== 0) throw Error(`${command} failed: ${result.error ?? result.stderr ?? result.status}`);
  return result.stdout;
}
const git = args => run('git', ['-c', `safe.directory=${checkout}`, '-C', checkout, ...args]).trim();
if (git(['rev-parse', 'HEAD']) !== revision || git(['status', '--porcelain']) !== '') {
  throw Error(`requires a clean omoikane checkout at ${revision}`);
}
for (const name of ['RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER']) {
  if (process.env[name]) throw Error(`unset ${name} for the reference build`);
}
const source = resolve(dirname(fileURLToPath(import.meta.url)), kind === 'async' ? '../benchmarks/omoikane-async-worker/main.rs' : '../benchmarks/omoikane-worker/main.rs');
mkdirSync(build); // Refuse an existing build, preserving any previous evidence.
mkdirSync(join(build, 'src'));
copyFileSync(source, join(build, 'src/main.rs'));
copyFileSync(join(checkout, 'Cargo.lock'), join(build, 'Cargo.lock'));
writeFileSync(join(build, 'Cargo.toml'), `[package]
name = "omoikane-workload-worker"
version = "0.0.0"
edition = "2024"

[dependencies]
omoikane = { path = ${JSON.stringify(checkout)} }
serde_json = "=1.0.151"

[patch.crates-io]
regress = { path = ${JSON.stringify(join(checkout, 'engine/regress'))} }
`);
const env = { ...process.env, CARGO_TARGET_DIR: target };
const metadataText = run('cargo', ['metadata', '--format-version', '1'], { cwd: build, env });
writeFileSync(join(build, 'cargo-metadata.json'), metadataText);
// Resolution adds the adapter and removes unused dependencies; it must not
// substitute any version/source/checksum from the original lockfile.
run(python, ['-c', `import sys,tomllib
from pathlib import Path
def packages(path):
    data=tomllib.loads(Path(path).read_text())
    return {(p['name'],p['version'],p.get('source'),p.get('checksum')) for p in data['package']}
extra=packages(sys.argv[2])-packages(sys.argv[1])
assert extra=={('omoikane-workload-worker','0.0.0',None,None)}, 'dependency drift: '+repr(extra)
`, join(checkout, 'Cargo.lock'), join(build, 'Cargo.lock')]);
const metadata = JSON.parse(metadataText);
const features = {};
for (const [name, path] of [['omoikane', 'Cargo.toml'], ['boa_engine', 'engine/boa/core/engine/Cargo.toml'], ['regress', 'engine/regress/Cargo.toml']]) {
  const pkg = metadata.packages.find(pkg => pkg.name === name);
  if (!pkg || realpathSync(pkg.manifest_path) !== join(checkout, path)) throw Error(`wrong ${name} source`);
  features[name] = metadata.resolve.nodes.find(node => node.id === pkg.id).features;
}
const expectedBoa = ['annex-b', 'default', 'experimental', 'float16', 'intl', 'intl_bundled', 'temporal', 'xsum'];
if (JSON.stringify(features.boa_engine) !== JSON.stringify(expectedBoa) || features.omoikane.length) {
  throw Error('application/Boa feature configuration changed');
}
run(python, [join(checkout, 'scripts/cargo-space-guard.py'), 'run', '--target-dir', target,
  '--required-free-gib', '14', '--maximum-target-gib', '12', '--', 'cargo', 'build', '--locked', '--release'],
  { cwd: build, env, stdio: 'inherit' });
if (git(['status', '--porcelain']) !== '') throw Error('application checkout changed during build; inspect before using results');
const executable = join(target, 'release/omoikane-workload-worker');
const hash = path => createHash('sha256').update(readFileSync(path)).digest('hex');
const info = { recorded_at: new Date().toISOString(), workload_kind:kind, omoikane_revision: revision, checkout_clean: true,
  rustc: run('rustc', ['-Vv']).trim(), cargo: run('cargo', ['-V']).trim(),
  profile: 'release', cargo_args: ['build', '--locked', '--release'], rustflags: '', features,
  original_lock_sha256: hash(join(checkout, 'Cargo.lock')), adapter_lock_sha256: hash(join(build, 'Cargo.lock')),
  worker_source_sha256: hash(join(build, 'src/main.rs')), executable, executable_sha256: hash(executable),
  execution_mode: 'interpreter; baseline-jit feature absent',
};
writeFileSync(join(build, 'build-info.json'), JSON.stringify(info, null, 2) + '\n');
console.log(JSON.stringify(info));
