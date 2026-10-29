#!/bin/bash
set -euo pipefail
export PATH=/usr/local/cargo/bin:$PATH
trap 'taskset --all-tasks --pid --cpu-list 0-3 1647005 > /tmp/temiya-145-record-perf/agent-affinity-restored.log; taskset --all-tasks --pid --cpu-list 0-3 1646982 >> /tmp/temiya-145-record-perf/agent-affinity-restored.log' EXIT
export TEMIYA_REVISION=01858c32c0123d980fa313dc24a08137efaf0abf
export BENCH_NOTES='ARM64 guestCPU0; agent threads on CPUs1-3; release features[]; all builds and functional runs completed before timing; unchanged original eleven bodies/counts/oracles, 15 samples and three alternating pairs.'
taskset --cpu-list 0 node scripts/compare-omoikane-workloads.mjs --temiya-baseline /tmp/temiya-omoikane-workloads-v2 /tmp/temiya-145-record-perf/current-worker /tmp/temiya-145-perf/baseline-info.json 15 3 > /tmp/temiya-145-record-perf/beforeafter.json 2> /tmp/temiya-145-record-perf/beforeafter.progress
taskset --cpu-list 0 node scripts/compare-omoikane-workloads.mjs /tmp/temiya-omoikane-workloads-v2 /tmp/temiya-145-record-perf/current-worker /tmp/temiya-omoikane-worker-repro/build-info.json 15 3 > /tmp/temiya-145-record-perf/boa.json 2> /tmp/temiya-145-record-perf/boa.progress
