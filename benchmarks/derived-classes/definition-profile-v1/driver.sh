#!/bin/bash
set -euo pipefail
cd /tmp/temiya-derived-classes
for pid in 3281392 3281415 3281782; do test -d /proc/$pid; done
trap 'for pid in 3281392 3281415 3281782; do taskset --all-tasks --pid --cpu-list 0-3 "$pid" >> /tmp/temiya-155-definition-profile-affinity-restored.log; done' EXIT
for pid in 3281392 3281415 3281782; do
 taskset --all-tasks --pid --cpu-list 1-3 "$pid" >> /tmp/temiya-155-definition-profile-affinity-pinned.log
 test "$(awk '/^Cpus_allowed_list:/{print $2}' /proc/$pid/status)" = 1-3
done
taskset --cpu-list 0 node /tmp/temiya-155-profile-definition.mjs
