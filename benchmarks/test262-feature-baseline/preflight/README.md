# Build and integration evidence

Source `d79eded4244a0fac2b8893754720fcb547777fd1`; CI run [40912271681](https://github.com/ieee0824/temiya/actions/runs/40912271681). Stable and MSRV each passed 711 tests with 26 ignored. The local ARM run passed 710 with 26 ignored. Logs retain the existing original-loop dependency boundary: 35 target passes and 11 dependency failures, not 46 passes.

`evidence.json` records decoded SHA-256 identities for the compressed raw logs. `worker-identity.json` records unchanged library/worker source inputs and exact ARM/x64 worker identities against the prior process-isolation performance records. This supports only the existing workload/host measurements; it makes no broader performance claim.

The full fixed-corpus capture is still running. These files are preflight evidence, not the completed feature baseline.
