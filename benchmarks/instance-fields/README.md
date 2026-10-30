# Public instance field inputs

These inputs were frozen before the #157 runtime implementation. The fixed selection contains all
2057 files whose pinned reference metadata includes class-fields-public. Cases
with other unsupported dependencies remain in the selection.

The five authored workloads cover base/default construction, derived fields,
computed keys evaluated once per class, own data definition bypassing inherited
setters, and per-instance arrow initializers. Each performs10000 constructions
and returns50005000 with extra state assertions. All five were checked twice
against the actual pinned Boa worker before runtime implementation; the archive
retains20 validated warmup/sample values. These are oracle probes, not timing
acceptance. Performance qualification must retain the existing15 samples, three
alternating pairs and noise rule, plus independent controls and host results.

initial-v1 retains the first implementation and both4098-variant captures per
backend, including all failures/timeouts and429 added passes with0 prior-pass
regressions. Its native Boa/self controls have5wins10noise and15noise. This
source is not merge-qualified. verify-initial.mjs replays that exact history.
