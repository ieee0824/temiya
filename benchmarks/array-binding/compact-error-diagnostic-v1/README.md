# compact-error-diagnostic-v1

Experimental compact shared errors, source54e5ea9. Error size shrinks40to24bytes and Value result40to32bytes on aarch64, but all six instruction-count observations regress, including TDZ-close+1.44%. Preserve this adverse candidate; it is not native-qualified.935tests,26ignored,Clippy and3focused strict Miri tests pass.

One instruction-count capture per fixed input. All adverse observations are retained. These diagnostics do not establish native speed qualification.
