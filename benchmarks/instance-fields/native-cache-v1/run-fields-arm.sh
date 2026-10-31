#!/bin/bash
set -euo pipefail
export PATH=/usr/local/cargo/bin:$PATH
cd /tmp/temiya-instance-fields
root=/tmp/temiya-157-initializer-cache-v1
for pid in 3281415 3281392 3281782; do test -d /proc/$pid; done
trap 'for pid in 3281415 3281392 3281782; do taskset --all-tasks --pid --cpu-list 0-3 "$pid" >> /tmp/temiya-157-initializer-cache-v1/agent-affinity-restored.log; done' EXIT
for pid in 3281415 3281392 3281782; do
 taskset --all-tasks --pid --cpu-list 1-3 "$pid" >> "$root/agent-affinity-pinned.log"
 test "$(awk '/^Cpus_allowed_list:/{print $2}' /proc/$pid/status)" = 1-3
done
export TEMIYA_REVISION=835b21da13c4ffffb06359790995ce6a0a1a67eb
export BENCH_NOTES='Initializer immutable function code cache. Frozen5sources/counts/oracles unchanged.15samples3alternatingpairs. CPU0workers/controller; verified agents1-3. No local builds/tests/audits. Baseline38983db; retain all outcomes.'
taskset --cpu-list 0 node scripts/compare-class-workloads.mjs --fields benchmarks/instance-fields/workloads-v1 "$root/worker" /tmp/temiya-boa-reference-v1/build-info.json 15 3 > "$root/fields-boa.json" 2> "$root/fields-boa.progress"
taskset --cpu-list 0 node scripts/compare-class-workloads.mjs --fields --temiya-baseline benchmarks/instance-fields/workloads-v1 "$root/worker" "$root/worker-info.json" 15 3 > "$root/fields-same-binary.json" 2> "$root/fields-same-binary.progress"

taskset --cpu-list 0 node scripts/compare-class-workloads.mjs --fields --temiya-baseline benchmarks/instance-fields/workloads-v1 "$root/worker" /tmp/temiya-157-field-asi-v1/worker-info.json 15 3 > "$root/fields-beforeafter.json" 2> "$root/fields-beforeafter.progress"
export TEMIYA_REVISION=38983db0900f8f91b9aa2bcb0baaf04ffdbf2793
taskset --cpu-list 0 node scripts/compare-class-workloads.mjs --fields benchmarks/instance-fields/workloads-v1 /tmp/temiya-157-field-asi-v1/worker /tmp/temiya-boa-reference-v1/build-info.json 15 3 > "$root/fields-baseline-boa.json" 2> "$root/fields-baseline-boa.progress"
