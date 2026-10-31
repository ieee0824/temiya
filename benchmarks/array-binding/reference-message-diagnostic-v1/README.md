# reference-message-diagnostic-v1

Build complete reference diagnostics with one UTF-8 allocation, preserving replacement of unpaired UTF-16 surrogates. TDZ-close instructions decrease by 1.64%; all six workload observations are retained. All-target tests: 932 passed, 26 ignored; Clippy and focused strict Miri pass.

One instruction-count capture per fixed input. All adverse observations are retained. These diagnostics do not establish native speed qualification.
