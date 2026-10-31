#!/bin/bash
set -euo pipefail
export PATH=/usr/local/cargo/bin:$PATH
cd /tmp/temiya-static-class-elements
root=/tmp/temiya-158-integrated-v1
for pid in 3281415 3281392 3281782; do test -d /proc/$pid; done
trap 'for pid in 3281415 3281392 3281782; do taskset --all-tasks --pid --cpu-list 0-3 "$pid" >> /tmp/temiya-158-integrated-v1/agent-affinity-restored.log; done' EXIT
for pid in 3281415 3281392 3281782; do
 taskset --all-tasks --pid --cpu-list 1-3 "$pid" >> "$root/agent-affinity-pinned.log"
 test "$(awk '/^Cpus_allowed_list:/{print $2}' /proc/$pid/status)" = 1-3
done
export TEMIYA_REVISION=c4b65d6cf7620dbec60f66caf4a7a3cea124d5aa
export BENCH_NOTES='Integrated static direct evaluation and primary instance-field optimizations c4b65d6, compared with default-classa689619. Frozen5sources/counts/oracles unchanged.15samples3alternatingpairs. CPU0workers/controller; agents1-3. No local builds/tests/audits. Retain every outcome.'
taskset --cpu-list 0 node scripts/compare-class-workloads.mjs --static benchmarks/static-class-elements/workloads-v2 "$root/worker" /tmp/temiya-boa-reference-v1/build-info.json 15 3 > "$root/static-boa.json" 2> "$root/static-boa.progress"
taskset --cpu-list 0 node scripts/compare-class-workloads.mjs --static --temiya-baseline benchmarks/static-class-elements/workloads-v2 "$root/worker" "$root/worker-info.json" 15 3 > "$root/static-same-binary.json" 2> "$root/static-same-binary.progress"

taskset --cpu-list 0 node scripts/compare-class-workloads.mjs --static --temiya-baseline benchmarks/static-class-elements/workloads-v2 "$root/worker" /tmp/temiya-158-default-class-v1/worker-info.json 15 3 > "$root/static-beforeafter.json" 2> "$root/static-beforeafter.progress"
export TEMIYA_REVISION=a6896195e1b2310403941f5569f877008be7aac6
taskset --cpu-list 0 node scripts/compare-class-workloads.mjs --static benchmarks/static-class-elements/workloads-v2 /tmp/temiya-158-default-class-v1/worker /tmp/temiya-boa-reference-v1/build-info.json 15 3 > "$root/static-baseline-boa.json" 2> "$root/static-baseline-boa.progress"
