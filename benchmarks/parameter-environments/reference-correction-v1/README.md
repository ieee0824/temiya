This is an explicit correctness correction to the pinned Omoikane/Boa reference
`007556adccb8fb4329236932ca8175f8e8d119d2`, not an upstream fix or stock Boa.
The original reference, inputs, and existing 70-workload measurements remain intact.

The two-file patch keeps parameter and body scopes distinct, preserves copied
parameter values, and detects expressions inside binding patterns while retaining
the independent `Function.length` rule. It preserves the pinned fork's existing
variable-name collection and makes no unrelated optimization.

Build a separate checkout with exactly this patch using:

```sh
node scripts/build-omoikane-worker.mjs CHECKOUT NEW_BUILD_DIR TARGET_DIR sync-parameter-corrected
```

The builder verifies the pinned Git revision, exact patch bytes, changed paths,
unchanged dependency versions, and original features before and after building.
It uses the original synchronous worker, release profile, dependency lock, and
capacity guard. Its receipt explicitly records `checkout_clean: false` and the
correction identity. Default synchronous/asynchronous builds still require a
clean original checkout. Original-reference auditors must not accept this receipt
as an unmodified reference.

`compare-class-workloads.mjs --parameters` and
`audit-class-workload-records.mjs --parameters` use the original eight frozen
inputs and the existing watchdog, 15 samples, three alternating pairs, integer
oracles and noise classifier. The corrected reference has its own comparison
kind and name. An unmodified reference receipt cannot satisfy this route;
legacy routes still require their original clean reference. Identical-Temiya
controls use the ordinary before/after route. The hosted workflow builds both
reference variants before timing, retains their separate receipts, and measures
the inherited 70 cases against the original reference.

Release correctness is retained in `release-validation`: all eight original
inputs validate twice. Replay with `node benchmarks/parameter-environments/reference-correction-v1/verify.mjs`.
Hosted native speed measurements for this corrected pinned fork are pending. The upstream debug experiments in `../reference-patch-diagnostics-v1`
are separate correctness evidence and do not establish native speed.
