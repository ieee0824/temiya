# Test262 harness

The harness is pinned to commit
`d53ebc8b1931d9f1541e43d59650776e4b766248`. Run `scripts/test262` to fetch
that exact revision into `target/test262`; `TEST262_DIR` selects another cache.

`test262/manifest.json` records the same revision and exact-path skips. Every
skip has a stable reason identifier and a human explanation, so reports can be
reproduced and aggregated without interpreting free-form logs.

`temiya::test262::TestCase` parses the frontmatter subset used for includes,
features, flags, and negative phase/type expectations. It expands tests into
sloppy/strict variants in stable order, or a single module/raw/flag-constrained
variant. A strict Script receives the directive before parsing, ensuring parse
negative tests are exercised in the correct source variant.
