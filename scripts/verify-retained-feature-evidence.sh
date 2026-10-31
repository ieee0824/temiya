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
    ;;
  bindings)
    node benchmarks/array-binding/verify-workloads.mjs
    node benchmarks/array-binding/verify-fixed.mjs
    node benchmarks/array-binding/verify-hosted.mjs
    node benchmarks/array-binding/verify-full.mjs
    node benchmarks/array-binding/parent-integration-diagnostic-v1/verify.mjs
    node benchmarks/array-binding/array-data-properties-diagnostic-v1/verify.mjs
    node benchmarks/array-binding/security-current-integrated-v1/verify.mjs benchmarks/array-binding/security-current-integrated-v1 6c65967973ad4de72986db751d7c7e30547251c2
    ;;
  *)
    echo "usage: $0 {classes|fields|private|iterators|bindings}" >&2
    exit 2
    ;;
esac
