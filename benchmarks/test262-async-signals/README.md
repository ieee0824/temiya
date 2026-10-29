# Lazy Test262 checkpoint storage qualification

This follow-up measures 30d6c412277cb7c417277eeff693dfb17ed2c50d against main
858afcce22c9ec50bb94f7630fad0f6f298ce821. It retains the original eleven
application bodies, setup, iteration counts and numeric oracles, with 15 samples,
one checked warmup and three alternating pairs. Each report contains 66 workers,
1056 checked values and 33 decisions. Limits remain 180 seconds / 512 MiB and
the decision gate remains max(10%, three times the sum of relative MADs).

The queue stores rejection checkpoints and the host rejection hook in a lazily
allocated sidecar. Ordinary queues avoid that allocation. All managed checkpoint
edges remain traced, including before queue publication and during execution;
regular jobs still run before rejection notifications. Source and validation
are described in the [follow-up record](../test262-async/signals-followup.md).

## Observed native outcomes

Both hosted machines were accepted without selecting a CPU model. Actual modified
Boa remains Omoikane 007556adccb8fb4329236932ca8175f8e8d119d2 with its original
features and locks, release profile and no baseline JIT.

| Cohort | Candidate / actual Boa | Baseline / actual Boa | Candidate / baseline | Identical-executable control |
| --- | --- | --- | --- | --- |
| AMD EPYC 7763 | 33 faster | 33 faster | 33 within noise | 33 within noise |
| AMD EPYC 9V74 | 33 faster | 33 faster | 33 within noise | 33 within noise |
| ARM64 | 30 faster, 3 inconclusive | 31 faster, 2 inconclusive | 32 within, 1 slower | 33 within noise |

The repeated x64 losses in the initial candidate are absent in these two new
cohorts. The EPYC7763 cohort matches the CPU model that showed the original
String/property regressions; it is a new machine, not the same physical host.
This is a bounded observation, not proof of the microarchitectural cause or of
universal speed superiority. ARM64 retains an object-allocation pair 2 slowdown
(ratio 1.331174, gate 0.244331), and does not qualify every Boa comparison.
No timing replay replaces any observation.

The [initial candidate](../test262-async/README.md) remains separately preserved:
13 native reports, twelve matched modeled-counter runs, all slower/inconclusive
outcomes and the failing reproductions. Its measurements remain associated with
abbeac6 and are not relabeled as this candidate. Modeled counters are diagnostics,
not native speed or hardware-event measurements.

## Validation and provenance

Full native tests pass 695 cases with 25 explicit ignores; the original CLI
selections execute 209 positive variants, and selected Miri passes 21 distinct
queue/Promise/shared-Agent tests. The exact measured commit's CI40865480082 passes
all four jobs, including stable, Rust1.85, formatting/Clippy and C ABI. The
initial qualification verifier independently checks those retained logs.

ARM64 worker SHA-256 is
889f4545b3b2a67dfccf1f1d7081d57f807691b5ae55676dd5e274f26b8f7352.
Hosted compiler/source/feature/manifest/control identities and worker hashes are
cross-checked against the measured-source stable CI worker. `provenance.json`
pins the measured and baseline inputs and every losslessly compressed record.

```sh
node benchmarks/test262-async-signals/verify.mjs
```

User-requested disk-usage inspection occurred near the end of the ARM64
same-executable control, so an overlap cannot be excluded. Bulk removal of old
build caches waited until the entire local timing session had exited successfully.
The source, records and three active reference/candidate executables were preserved
and their SHA-256 values rechecked. The control and operator-event note are both
retained; no samples were removed because of this inspection.

The fixed ARM64 queue-allocation disassembly also records the request size of
the first Rc allocation in Runtime::allocate_job_queue: baseline112 bytes,
initial candidate144, sidecar candidate104. This establishes that storage request
for these ARM64 ELFs only; it does not establish the cause of a native timing
change or total heap savings for every workload.
