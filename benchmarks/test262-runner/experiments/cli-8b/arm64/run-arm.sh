#!/bin/bash
set -euo pipefail
export PATH=/usr/local/cargo/bin:$PATH
trap 'taskset --all-tasks --pid --cpu-list 0-3 1647005 > /tmp/temiya-145-cli-perf/agent-affinity-restored.log; taskset --all-tasks --pid --cpu-list 0-3 1646982 >> /tmp/temiya-145-cli-perf/agent-affinity-restored.log' EXIT
export TEMIYA_REVISION=8b5972e773fa4dd2701cb835a741606c06d121a3
export BENCH_NOTES='ARM64 guestCPU0; agent threads on CPUs1-3; release features[]; all builds and functional runs completed before timing; unchanged original eleven bodies/counts/oracles, 15 samples and three alternating pairs.'
taskset --cpu-list 0 node scripts/compare-omoikane-workloads.mjs --temiya-baseline /tmp/temiya-omoikane-workloads-v2 /tmp/temiya-145-cli-perf/current-worker /tmp/temiya-145-perf/baseline-info.json 15 3 > /tmp/temiya-145-cli-perf/beforeafter.json 2> /tmp/temiya-145-cli-perf/beforeafter.progress
taskset --cpu-list 0 node scripts/compare-omoikane-workloads.mjs /tmp/temiya-omoikane-workloads-v2 /tmp/temiya-145-cli-perf/current-worker /tmp/temiya-omoikane-worker-repro/build-info.json 15 3 > /tmp/temiya-145-cli-perf/boa.json 2> /tmp/temiya-145-cli-perf/boa.progress
