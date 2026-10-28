# Managed packed numeric elements, source 25e46c7608366603b68504d73dc3ba78a2acbfc8

This is a new trial on the integrated tracing runtime, not the earlier pre-GC
packed-number trial. Contiguous default numeric descriptors use8-byte values;
other values/attributes/holes/sparse entries restore full descriptors. Numeric
slots have no GC edges; descriptor-backed entries retain full strong tracing.

Native661 pass/21 ignored, Clippy, two exact public integration Miri tests and
selected pinned array Test262 regressions pass. The allocation-only probe
requests585216 bytes before versus65024 after (7 requests each), validates
sum8386560 for4096 entries and releases all cells. The same probe source was
temporarily added to the reference checkout; the runtime/manifests were kept
unchanged and the probe removed before freezing the clean timing worker.

Performance comparisons and full latest-source CI/security are pending. Keep
original11 counts/oracles/15samples/3pairs/512MiB/180seconds/10%-MAD gates and
all observations, including unfavorable/noise outcomes. Allocation bytes are
not a timing or peak-memory result. Earlier failed trials remain preserved.
