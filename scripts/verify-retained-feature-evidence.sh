#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."

# Preserve every retained-evidence check; shard only independent families.
case "${1:-}" in
  classes)
    node benchmarks/base-classes/verify-native-loop.mjs
    node benchmarks/base-classes/verify-native-originals.mjs
    node benchmarks/base-classes/verify-loop-originals.mjs
    node benchmarks/base-classes/verify-hosted-f1c.mjs
    node benchmarks/base-classes/verify-hosted-loop.mjs
    node benchmarks/derived-classes/verify-initial.mjs
    node benchmarks/derived-classes/verify-arguments.mjs
    node benchmarks/derived-classes/verify-native-prototypes.mjs
    node benchmarks/derived-classes/verify-dependencies.mjs
    node benchmarks/derived-classes/verify-full.mjs
    node benchmarks/derived-classes/verify-view-index.mjs
    node benchmarks/derived-classes/verify-delegation.mjs
    node benchmarks/derived-classes/verify-anonymous-env.mjs
    node benchmarks/derived-classes/verify-function-metadata.mjs
    node benchmarks/derived-classes/verify-class-bridge.mjs
    node benchmarks/derived-classes/verify-class-prototype.mjs
    node benchmarks/derived-classes/verify-class-loop-capture.mjs
    node benchmarks/derived-classes/verify-class-loop-runtime.mjs
    node benchmarks/derived-classes/verify-hosted.mjs
    node benchmarks/base-classes/verify-full-f1c.mjs
    node benchmarks/base-classes/verify-full-loop.mjs
    ;;
  fields)
    node benchmarks/super-properties/verify-dependencies.mjs
    node benchmarks/super-properties/verify-performance.mjs
    node benchmarks/super-properties/verify-full.mjs
    node benchmarks/super-properties/verify-hosted.mjs
    node benchmarks/instance-fields/verify-initial.mjs
    node benchmarks/instance-fields/verify-field-capture.mjs
    node benchmarks/instance-fields/verify-native-asi.mjs
    node benchmarks/static-class-elements/verify-initial.mjs
    node benchmarks/static-class-elements/verify-default-class.mjs
    node benchmarks/static-class-elements/verify-hosted.mjs
    node benchmarks/static-class-elements/verify-native-integrated.mjs
    node benchmarks/static-class-elements/verify-integrated-correctness.mjs
    node benchmarks/static-class-elements/verify-full.mjs
    node benchmarks/instance-fields/verify-native-cache.mjs
    node benchmarks/instance-fields/verify-arrow-read-v1.mjs
    node benchmarks/instance-fields/verify-direct-fields-v1.mjs
    node benchmarks/instance-fields/verify-fixed-direct.mjs
    node benchmarks/instance-fields/verify-validation.mjs
    node benchmarks/instance-fields/verify-full.mjs
    node benchmarks/instance-fields/verify-hosted.mjs
    ;;
  private)
    node benchmarks/private-fields/verify-initial.mjs
    node benchmarks/private-fields/verify-initial.mjs private-expression-correctness-v1
    node benchmarks/private-fields/verify-private-expression.mjs
    node benchmarks/private-elements/verify-private-plans.mjs
    node benchmarks/private-elements/verify-initial-ast.mjs
    node benchmarks/private-elements/verify-full.mjs
    node benchmarks/private-elements/verify-hosted.mjs
    node benchmarks/private-elements/verify-fixed.mjs
    node benchmarks/private-fields/verify-private-expression.mjs bound-private-diagnostic-v1
    node benchmarks/private-fields/verify-private-expression.mjs bound-constructor-diagnostic-v1
    node benchmarks/private-fields/verify-private-expression.mjs local-bindings-diagnostic-v1
    node benchmarks/private-fields/verify-private-expression.mjs identifier-values-diagnostic-v1
    node benchmarks/private-fields/verify-full.mjs
    node benchmarks/private-fields/verify-initial.mjs default-private-correctness-v1
    node benchmarks/private-fields/verify-initial.mjs stored-constructor-correctness-v1
    node benchmarks/private-fields/verify-initial.mjs identifier-values-correctness-v1
    node benchmarks/test262-asan-compat/verify.mjs
    ;;
  iterators)
    node benchmarks/iterator-operations/verify-validation.mjs
    node benchmarks/iterator-operations/verify-full.mjs
    node benchmarks/iterator-operations/verify-fixed.mjs
    node benchmarks/iterator-operations/verify-hosted.mjs
    node benchmarks/iterator-operations/verify-profiles.mjs
    node benchmarks/iterator-operations/verify-key-profiles.mjs
    node benchmarks/iterator-operations/security-dense-marks-v1/verify.mjs benchmarks/iterator-operations/security-dense-marks-v1 18d3885b09a5b9c491b57ba5e35c36b094afd441
    ;;
  bindings)
    node benchmarks/array-binding/verify-workloads.mjs
    node benchmarks/array-binding/verify-fixed.mjs
    node benchmarks/array-binding/fixed-array-data-properties-v1/verify.mjs
    node benchmarks/array-binding/fixed-inline-thrown-v1/verify.mjs
    node benchmarks/array-binding/fixed-cache32-v1/verify.mjs
    node benchmarks/array-binding/verify-hosted.mjs
    node benchmarks/array-binding/verify-full.mjs
    node benchmarks/array-binding/parent-integration-diagnostic-v1/verify.mjs
    node benchmarks/array-binding/array-data-properties-diagnostic-v1/verify.mjs
    node benchmarks/array-binding/security-current-integrated-v1/verify.mjs benchmarks/array-binding/security-current-integrated-v1 6c65967973ad4de72986db751d7c7e30547251c2
    node benchmarks/array-binding/receiver-frame-diagnostics-v1/verify.mjs
    node benchmarks/array-binding/prepared-parameters-diagnostic-v1/verify.mjs
    node benchmarks/array-binding/ordinary-array-step-diagnostic-v1/verify.mjs
    node benchmarks/array-binding/array-finish-diagnostic-v1/verify.mjs
    node benchmarks/array-binding/reference-message-diagnostic-v1/verify.mjs
    node benchmarks/array-binding/outlined-slots-diagnostic-v1/verify.mjs
    node benchmarks/array-binding/cache32-diagnostic-v1/verify.mjs
    node benchmarks/array-binding/compact-error-diagnostic-v1/verify.mjs
    node benchmarks/array-binding/inline-thrown-error-diagnostic-v1/verify.mjs
    node benchmarks/array-binding/security-array-data-properties-v1/verify.mjs benchmarks/array-binding/security-array-data-properties-v1 cd6c0294bba1a2f4ac2059dfb56cb6cd289aba8a
    node benchmarks/array-binding/security-inline-thrown-v1/verify.mjs benchmarks/array-binding/security-inline-thrown-v1 7a154b17dd6982913047b364e7bf4c293103798a
    node benchmarks/array-binding/security-cache32-v1/verify.mjs benchmarks/array-binding/security-cache32-v1 9f84bc5c6416911df51d76cf10b936ad3c3e003f
    ;;
  objects)
    node benchmarks/parameter-environments/integrated-correctness-v1/verify.mjs
    node benchmarks/parameter-environments/reference-diagnostics-v1/verify.mjs
    node benchmarks/parameter-environments/large-preparation-v1/verify.mjs
    node benchmarks/parameter-environments/verify-selection.mjs
    node benchmarks/parameter-environments/oracles-initial-v1/verify.mjs
    node benchmarks/parameter-environments/correctness-v1/verify.mjs
    node benchmarks/object-binding/verify-qualified.mjs
    node benchmarks/object-binding/verify-bytecode-experiments.mjs
    node benchmarks/object-binding/full-single-catch-v1/verify.mjs
    node benchmarks/object-binding/full-initial-catch-v1/verify.mjs
    node benchmarks/object-binding/security-single-catch-v1/verify.mjs benchmarks/object-binding/security-single-catch-v1 e83240f33e8b2953fe728bfc7fb40842b210367a
    node benchmarks/object-binding/security-initial-catch-v1/verify.mjs benchmarks/object-binding/security-initial-catch-v1 d58d4d41e93ce2fcc7fdd220cf5e7ae63e1c540e
    node benchmarks/object-binding/security-pinned-context-v1/verify.mjs benchmarks/object-binding/security-pinned-context-v1 2c09b52f7b9692ea4781932bc923ecf0ff60a3c9
    node benchmarks/object-binding/full-pinned-context-v1/verify.mjs
    node benchmarks/object-binding/full-function-name-v1/verify.mjs
    node benchmarks/object-binding/security-function-name-v1/verify.mjs benchmarks/object-binding/security-function-name-v1 dc9bd56ca7ff473774b62218d0cbb4b8de5aeaf9
    node benchmarks/object-binding/nayuta-allocation-review-v1/verify.mjs
    node benchmarks/object-binding/nullish-message-correctness-v1/verify.mjs
    node benchmarks/object-binding/rest-accessor-correctness-v1/verify.mjs
    node benchmarks/object-binding/rest-data-correctness-v1/verify.mjs
    node benchmarks/object-binding/full-direct-get-v1/verify.mjs
    node benchmarks/object-binding/security-direct-get-v1/verify.mjs benchmarks/object-binding/security-direct-get-v1 180773483841e2c163d6ee42d3ece9ae1a981645
    node benchmarks/object-binding/single-catch-correctness-v1/verify.mjs
    node benchmarks/object-binding/fresh-array-diagnostics-v1/verify.mjs
    node benchmarks/object-binding/full-inline-parent-v1/verify.mjs
    node benchmarks/object-binding/allocation-diagnostics-v1/verify.mjs
    node benchmarks/object-binding/security-prepared-guards-v1/verify.mjs benchmarks/destructuring-assignment/security-vm-v1 eb29c4169c0fdfd9246546f76b5874097a53ef62
    node benchmarks/destructuring-assignment/full-vm-v1/verify.mjs
    node benchmarks/destructuring-assignment/correctness-v1/verify.mjs
    node benchmarks/destructuring-assignment/vm-correctness-v1/verify.mjs
    node benchmarks/destructuring-assignment/vm-gc-correctness-v1/verify.mjs
    node benchmarks/destructuring-assignment/vm-instructions-v1/verify.mjs
    node benchmarks/destructuring-assignment/verify-selection.mjs
    node benchmarks/destructuring-assignment/verify-oracles.mjs
    node benchmarks/object-binding/verify-workloads.mjs
    node benchmarks/object-binding/verify-profiles.mjs
    node benchmarks/object-binding/verify-hosted.mjs
    node benchmarks/object-binding/verify-symbol-regression.mjs
    node benchmarks/object-binding/verify-full.mjs
    node benchmarks/object-binding/security-inline-parent-v1/verify.mjs benchmarks/object-binding/security-inline-parent-v1 fe67ff56a766d834bc70c4e8219f4fad47b06723
    node benchmarks/object-binding/inline-parent-correctness-v1/verify.mjs
    node benchmarks/object-binding/latest-parent-diagnostic-v1/verify.mjs
    node benchmarks/object-binding/security-current-integrated-v1/verify.mjs benchmarks/object-binding/security-current-integrated-v1 010ac60ddeb68ce702e1f3b2a4cfdd87345f2568
    ;;
  *)
    echo "usage: $0 {classes|fields|private|iterators|bindings|objects}" >&2
    exit 2
    ;;
esac
