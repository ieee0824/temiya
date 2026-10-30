# Public instance field inputs

Issue #157 is not implemented by this snapshot. The fixed selection contains all
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
