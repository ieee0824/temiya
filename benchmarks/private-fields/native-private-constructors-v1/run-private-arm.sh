#!/bin/bash
set -euo pipefail
export PATH=/usr/local/cargo/bin:$PATH
cd /tmp/temiya-private-fields
root=/tmp/temiya-159-private-constructors-v1
for pid in 3281415 3281392 3281782; do test -d /proc/$pid; done
trap 'for pid in 3281415 3281392 3281782; do taskset --all-tasks --pid --cpu-list 0-3 "$pid" >> /tmp/temiya-159-private-constructors-v1/agent-affinity-restored.log; done' EXIT
for pid in 3281415 3281392 3281782; do
 taskset --all-tasks --pid --cpu-list 1-3 "$pid" >> "$root/agent-affinity-pinned.log"
 test "$(awk '/^Cpus_allowed_list:/{print $2}' /proc/$pid/status)" = 1-3
done
export TEMIYA_REVISION=9780432aa34cdb6b81debdb396f0af54873f73d5
export BENCH_NOTES='Private constructor assignment activation elimination compared with b7d287d. Frozen5sources/counts/oracles unchanged.15samples3alternatingpairs. CPU0workers/controller; agents1-3. No local builds/tests/audits. Retain every outcome.'
taskset --cpu-list 0 node scripts/compare-class-workloads.mjs --private benchmarks/private-fields/workloads-v1 "$root/worker" /tmp/temiya-boa-reference-v1/build-info.json 15 3 > "$root/private-boa.json" 2> "$root/private-boa.progress"
taskset --cpu-list 0 node scripts/compare-class-workloads.mjs --private --temiya-baseline benchmarks/private-fields/workloads-v1 "$root/worker" "$root/worker-info.json" 15 3 > "$root/private-same-binary.json" 2> "$root/private-same-binary.progress"

taskset --cpu-list 0 node scripts/compare-class-workloads.mjs --private --temiya-baseline benchmarks/private-fields/workloads-v1 "$root/worker" /tmp/temiya-159-private-plans-v1/worker-info.json 15 3 > "$root/private-beforeafter.json" 2> "$root/private-beforeafter.progress"
export TEMIYA_REVISION=b7d287d95f0a4773cf0b8c2c338866aecfd60f3c
taskset --cpu-list 0 node scripts/compare-class-workloads.mjs --private benchmarks/private-fields/workloads-v1 /tmp/temiya-159-private-plans-v1/worker /tmp/temiya-boa-reference-v1/build-info.json 15 3 > "$root/private-baseline-boa.json" 2> "$root/private-baseline-boa.progress"
