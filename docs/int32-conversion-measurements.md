# Bounded ToUint32 conversion experiment: ARM64 measurements

These measurements retain the experiment in #485. They do not establish a
speedup or resolve the object-allocation regression. The experimental runtime
is not included in this record; #485 remains under evaluation.

Both [before/after](../benchmarks/int32-conversion-arm64-beforeafter.json) and
[actual Boa](../benchmarks/int32-conversion-arm64-boa.json) use the original
11 pinned omoikane workloads, 15 timed samples and one checked warmup, three
alternating pairs, and the 512 MiB / 180-second worker watchdog. Each report has
66 successful workers and 1056 checked values. Every median, MAD, ratio, fixed
threshold and outcome was independently recomputed. The original manifest,
result oracles, worker coverage, compiler and binary hashes were checked; see
[provenance](../benchmarks/int32-conversion-arm64-provenance.json).

The measured runtime was `0f22a62b77a9a4416b98b4d1fc86f8c2d2ae0dfa`,
compared against `75b1ab4a1d5380540fd44a3c8649d9cdde87615c`. This experiment
predates the Engine backend integration in #487. Results apply to this ARM64
machine only; its CPU model is reported as unknown.

Of 33 before/after pairs, 32 were within the fixed noise threshold. Object
allocation pair 0 was slower beyond threshold: after/before median ratio
1.5696814994474904, threshold 0.4551446173844484. Both other object-allocation
pairs were within their thresholds. The cause is unknown; the flagged observation
is retained and must not be reclassified by changing the threshold or omitting
it. All 33 actual Boa comparisons were slower beyond threshold for Temiya.
Neither all-workload speed gate passed. Hosted x64 measurements are still in
progress and cannot establish the cause of this ARM64 observation by themselves.
