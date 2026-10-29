#!/bin/bash
set -euo pipefail
export PATH=/usr/local/cargo/bin:$PATH
trap 'taskset --all-tasks --pid --cpu-list 0-3 1647005 > /tmp/temiya-147-perf/recheck-agent-affinity-restored.log; taskset --all-tasks --pid --cpu-list 0-3 1646982 >> /tmp/temiya-147-perf/recheck-agent-affinity-restored.log' EXIT
export TEMIYA_REVISION=3163c4a33fc7748d36135102572b01eacf69e84a
export BENCH_NOTES='One prespecified full replay of before/after plus identical-executable control after five initial slower decisions. Both frozen workers and original eleven bodies/counts/oracles unchanged; 15 samples and three alternating pairs. Guest CPU0; own agent threads1-3; all builds/tests/profiles local are terminal. CLI descriptor fix31862e8 has byte-identical public Engine worker SHA5b4b309e.'
taskset --cpu-list 0 node scripts/compare-omoikane-workloads.mjs --temiya-baseline /tmp/temiya-omoikane-workloads-v2 /tmp/temiya-147-perf/current-worker /tmp/temiya-147-perf/baseline-info.json 15 3 > /tmp/temiya-147-perf/beforeafter-recheck.json 2> /tmp/temiya-147-perf/beforeafter-recheck.progress
taskset --cpu-list 0 node scripts/compare-omoikane-workloads.mjs --temiya-baseline /tmp/temiya-omoikane-workloads-v2 /tmp/temiya-147-perf/current-worker /tmp/temiya-147-perf/current-info.json 15 3 > /tmp/temiya-147-perf/same-binary-control.json 2> /tmp/temiya-147-perf/same-binary-control.progress
