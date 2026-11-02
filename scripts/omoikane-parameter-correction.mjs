// Explicit correctness-only variant of the original pinned reference.
import {readFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import assert from 'node:assert/strict';
import {revision} from './omoikane-benchmark-reference.mjs';
export const correctionSha256 = '2f857c116ef1550028070d8564b3d0e6c380e6b093d2ea70b6e2a56c7c2d526d';
export function verifyParameterCorrection(git) {
  const patch = readFileSync(new URL('../benchmarks/parameter-environments/reference-correction-v1/correction.patch', import.meta.url));
  if (createHash('sha256').update(patch).digest('hex') !== correctionSha256) throw Error('correction patch changed');
  const files = ['engine/boa/core/ast/src/function/parameters.rs', 'engine/boa/core/engine/src/bytecompiler/declarations.rs'];
  const status = files.map(p => ' M ' + p).join('\n');
  // git() trims leading whitespace; compare consistently, retaining all paths.
  if (git(['status', '--porcelain']) !== status.trim() || git(['diff', '--binary']) !== patch.toString().trim()) {
    throw Error('requires exactly the recorded parameter correctness correction; no other changes');
  }
  return {id: 'parameter-correctness-v1', patch_sha256: correctionSha256, files,
    scope: 'corrected pinned vendored Boa; not the unmodified reference or stock Boa'};
}

export function verifyParameterCorrectionReceipt(info) {
  assert.equal(info.omoikane_revision, revision);
  assert.equal(info.checkout_clean, false);
  assert.equal(info.workload_kind, 'sync-parameter-corrected');
  assert.equal(info.profile, 'release');
  assert.deepEqual(info.cargo_args, ['build', '--locked', '--release']);
  assert.equal(info.rustflags, '');
  assert.equal(info.reference_correction?.id, 'parameter-correctness-v1');
  assert.equal(info.reference_correction?.patch_sha256, correctionSha256);
  assert.deepEqual(info.reference_correction?.files, [
    'engine/boa/core/ast/src/function/parameters.rs',
    'engine/boa/core/engine/src/bytecompiler/declarations.rs',
  ]);
  assert.equal(info.original_lock_sha256, 'f3988c4d9b0ecc28560df18df1cae7760c7c55a016318ea3e31f3ebbfcaea211');
  assert.equal(info.adapter_lock_sha256, '6b4debb1e4368b52de4586b7b30c3d4b6b8882a1bd4ee87d78aa488a252517e7');
  assert.equal(info.worker_source_sha256, 'efba2d69a8c594a14c508ee3c9bf97a1799de98a438f14d449fe9be07f351a06');
  assert.equal(info.execution_mode, 'interpreter; baseline-jit feature absent');
  assert.deepEqual(info.features, {
    omoikane: [],
    boa_engine: ['annex-b', 'default', 'experimental', 'float16', 'intl', 'intl_bundled', 'temporal', 'xsum'],
    regress: ['backend-pikevm', 'default', 'std', 'utf16'],
  });
}
