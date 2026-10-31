# inline-thrown-error-diagnostic-v1

Carry explicit JavaScript throws inline without a Box or allocated diagnostic; retain owned messages for general native errors and bounded shared reference diagnostics.936tests,26ignored,Clippy and4focused strict Miri tests pass. All six instruction counts improve0.46–1.19%; normal RuntimeError and Value-result layouts remain40bytes on aarch64. This is not native qualification.

One instruction-count capture per fixed input. All adverse observations are retained. These diagnostics do not establish native speed qualification.
