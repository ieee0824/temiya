# Function class bridge, local evidence

Runtime source: 306c32c24a6c678276284c6ff898713137447370. The archive retains the fixed derived five-workload comparison against pinned actual Boa, the preceding43d3ff7 runtime, the candidate itself, and preceding-runtime versus Boa controls. Each report has15 samples and3 alternating pairs, with unchanged counts, expected values, timeouts and noise thresholds.

Boa comparisons:14 significant wins and1 within noise. Before/after:1 significant improvement and14 within noise. Self-control:15 within noise. Baseline/Boa:13 significant wins and2 within noise. All120 worker runs and1920 checked results are preserved and independently replayed by verify-class-bridge.mjs. These ARM64 observations do not replace the required independent hosted-machine qualification.

Both AST and Auto fixed203-file selections have401 variants:214 pass,187 fail, no skips or timeouts. Every outcome and failed diagnostic signature matches43d3ff7. Base originals retain276 pass and18 dependency failures per backend. The PR gate passes216 variants. Native tests pass776 with26 ignored; i686 coverage is type checking only.

The first full native attempt failed an obsolete generic compiler-diagnostic assertion. The specific top-level-class diagnostic is now checked, and the complete rerun passes. Both logs are retained. A local comparison attempt stopped during identity validation because cleanup removed the reference executable from its build target; it produced no timing samples. The same locked pinned source restored an ELF with exactly the historical SHA-256. Its recovery record, rejected controller output and restored-affinity logs are retained. The restored reference now also has an independent frozen copy outside the build target.
