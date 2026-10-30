# Class-loop capture proof before runtime activation

Source c654c020daca34565ad181dcb63c3bbd7e6b0b70 adds conservative complete-class capture analysis. Local base4/derived5 captures retain216 runs and3456 checked outputs against actual Boa, e11890a, the identical candidate, and e118/Boa controls. Every before/after comparison is within noise. Investigation found the argument-observable runtime entry still disabled the proven loop reuse, so these records do not demonstrate the intended optimization executing. The subsequent runtime-entry correction is a separate source and requires fresh validation.

Base/Boa has11 significant wins and1 noisy comparison; self12 noisy; baseline/Boa9 wins and3 noisy. Derived/Boa has15 wins; self15 noisy; baseline/Boa14 wins and1 noisy. Native tests778 pass,0 fail,26 ignored. i686 is type checking only. Both401-variant captures retain214 pass/187 fail and identical failure signatures; base originals276 pass/18 dependencies per backend; PR216 pass.

The archive retains the rejected mutable-key Clippy log and rejected packaging. Identity replay rejected an incorrectly copied candidate build-info as the baseline; corrected packaging uses frozen e118 info. No measurement records changed. verify-class-loop-capture.mjs independently checks both suites and all provenance before accepting the corrected archive.
