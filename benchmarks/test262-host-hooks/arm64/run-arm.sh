#!/bin/bash
set -euo pipefail
export PATH=/usr/local/cargo/bin:$PATH
trap 'taskset --all-tasks --pid --cpu-list 0-3 1647005 > /tmp/temiya-147-perf/agent-affinity-restored.log; taskset --all-tasks --pid --cpu-list 0-3 1646982 >> /tmp/temiya-147-perf/agent-affinity-restored.log' EXIT
export TEMIYA_REVISION=3163c4a33fc7748d36135102572b01eacf69e84a
export BENCH_NOTES='ARM64 guestCPU0; agent threads on CPUs1-3; release features[]; all builds and functional runs completed before timing; unchanged original eleven bodies/counts/oracles, 15 samples and three alternating pairs.'
taskset --cpu-list 0 node scripts/compare-omoikane-workloads.mjs --temiya-baseline /tmp/temiya-omoikane-workloads-v2 /tmp/temiya-147-perf/current-worker /tmp/temiya-147-perf/baseline-info.json 15 3 > /tmp/temiya-147-perf/beforeafter.json 2> /tmp/temiya-147-perf/beforeafter.progress
taskset --cpu-list 0 node scripts/compare-omoikane-workloads.mjs /tmp/temiya-omoikane-workloads-v2 /tmp/temiya-147-perf/current-worker /tmp/temiya-omoikane-worker-repro/build-info.json 15 3 > /tmp/temiya-147-perf/boa.json 2> /tmp/temiya-147-perf/boa.progress
