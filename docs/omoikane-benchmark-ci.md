# Omoikane comparison on a hosted runner

The [measurement workflow](../.github/workflows/omoikane-benchmark.yml) records
the original eleven public omoikane workloads on one Ubuntu 24.04 runner.
It checks out the application at
`007556adccb8fb4329236932ca8175f8e8d119d2`; the extractor verifies the fixture
hashes and independent expected results. It builds the actual application
embedding with its pinned Boa dependencies and feature configuration.

The measured Temiya revision is the selected workflow ref, or the exact PR
head for a label-triggered run. The baseline is `baseline_revision` for manual
runs, or the PR's base commit. The build records resolved full commit hashes,
compiler identity and executable hashes. Both Temiya workers are release builds
with no default features; all three workers use the same stable Rust compiler.

Each worker has a separate Cargo target directory. The application builder
enforces its existing free-space and target-size guard. All builds finish before
timing starts; no builds or benchmark workers run concurrently within the job.
The before/after comparison finishes before the Boa comparison starts. Each uses
15 timed samples, one checked warmup and three alternating pairs with the existing
512 MiB and 180-second process limits. The job has a 90-minute timeout.

After this workflow is on the default branch, run it manually with:

```sh
gh workflow run omoikane-benchmark.yml --repo ieee0824/temiya \
  --ref BRANCH --field baseline_revision=BASELINE_COMMIT
```

For a PR containing this workflow, adding the `benchmark-omoikane` label also
starts a run. Ordinary pushes do not start expensive measurements. To measure a
new PR head, remove and re-add the label. Runs for the same PR/ref cancel older
measurement runs; do not combine their partial reports.

The artifact `omoikane-comparison-RUN_ID-ATTEMPT` retains raw reports, per-worker
progress, the input manifest and build provenance for 30 days, including partial
output on failure. Download it with `gh run download RUN_ID --repo
ieee0824/temiya`. The job summary reports output validity and both all-eleven
speed gates separately. A successful measurement job does **not** mean either
speed gate passed. Incomplete or invalid results cannot establish a speedup.

Hosted runners can also have timing variation. Read the per-pair medians, MADs
and noise thresholds, and retain outliers. This comparison does not establish
full application compatibility or performance on arbitrary JavaScript; it does
not measure SpiderMonkey.
