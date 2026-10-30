# Immutable base-class prototype candidate

Runtime e11890a12fbd67a395dafad3e43b763e14dd29de reuses the traced instance prototype only for direct base-class construction. Alternate new targets retain their observable prototype lookup.

Local ARM64 base4 and derived5 suites preserve the frozen sources, counts, oracles,15 samples and3 alternating pairs. Both suites retain candidate/Boa, before/after306c32c, identical-candidate and baseline/Boa reports. All216 worker runs and3456 checked values are retained and reconstructed by verify-class-prototype.mjs.

Base/Boa:9 significant wins,3 within noise. Base before/after:12 within noise. Base self-control:1 apparent speedup and11 within noise. Baseline/Boa:9 wins,3 within noise. Derived/Boa:14 wins,1 within noise; before/after and self each15 within noise; baseline/Boa12 wins,3 within noise. A significant before/after improvement is not demonstrated. Hosted acceptance remains separate.

Native tests:777 passed,0 failed,26 ignored. Fmt, release Clippy across all targets/features, and i686 type checking pass;32-bit execution is not claimed. Both Ast and Auto fixed203-file captures retain214 pass/187 fail across401 variants, and all outcomes and failed diagnostic signatures match306c32c. Base originals retain276 pass/18 dependency failures per backend; PR gate216 pass.
