# Regression-gate integration evidence

[CI run 40918333546](https://github.com/ieee0824/temiya/actions/runs/40918333546)
tested source `8d951964a5da55bddae07d10a41fbf44804483b4` on stable and Rust 1.85.
Both toolchains executed the unchanged pinned String `at` and `charAt` originals,
four strict/sloppy variants. Seven actual captures per toolchain retain reference,
candidate, resource-bound mismatch, full partitions, successful runner output and
an intentionally rejected stale-skip policy with its raw capture and diagnostics.

The archives preserve every downloaded artifact file. `evidence.json` identifies
compressed archives, decoded files and independently recomputed capture summaries;
the full CI log is retained too. Actual executable hashes match the provided build
information. Generated PR selections contain only actually passing variants.

Run `node benchmarks/test262-regression-gate/verify.mjs` to decode and verify all
226 files and independently reconstruct the 14 actual captures. This does not
rerun JavaScript or claim full-corpus compatibility. The production full reference,
PR selection and periodic workflow have separate acceptance requirements.

`production-policy.json` preserves the exact 128-file/214-variant policy used
by the archived production capture. Its SHA-256 is checked before replay.
Current policy additions are tested against new captures; they cannot retroactively
change the selection required from this historical capture.
