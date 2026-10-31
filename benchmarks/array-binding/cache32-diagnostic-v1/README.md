# cache32-diagnostic-v1

Increase the bounded active-object working set from 16 to 32; use 32-bit bucket masks and preserve full handle identity, LRU order, collection clearing and teardown. TDZ-close instructions decrease by 0.87%; all six observations retained. All-target tests: 932 passed, 26 ignored; Clippy and both focused strict Miri identity/bounds tests pass.

One instruction-count capture per fixed input. All adverse observations are retained. These diagnostics do not establish native speed qualification.
