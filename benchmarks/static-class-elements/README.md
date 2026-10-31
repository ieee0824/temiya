# Static class element inputs

The selection is frozen before implementing #158. It includes every file in the
pinned 52,133-file reference catalog whose metadata has class-static-fields-public
or class-static-block, without filtering on execution outcomes. The union has
276 files. Source hashes and the corpus revision are recorded in
selection-freeze.json. Private elements and other dependencies remain selected
when they share these metadata features.

The baseline is the immutable #157 runtime snapshot 38983db, not a claim that
#157 has already merged. The static-element implementation and its performance
qualification are pending.

Five authored workloads in workloads-v2 retain 10000 iterations and expected
50005000. All were validated twice against the actual pinned Boa embedding.
The rejected v1 repeated-exception input and its reference failure are retained;
v2 exercises interruption on the final iteration. Oracle probes establish
correct output only, not timing qualification.

default-class-v1 retains the following anonymous default-exported class fix.
Both backends preserve all 462 initial passes and add one official module test;
84 failures remain. verify-default-class.mjs reconstructs all 547 outcomes per
backend and checks the 838 passing native tests. New-source performance remains
separate from the preceding hosted measurement.

hosted-initial-v1 retains all48reports of source31a13af on Xeon6973P-C and
Xeon8370C. Both static suites fail the fixed minimum2/3gate; copy1 also fails
a class workload. Adverse identical-executable controls remain recorded. Run
verify-hosted.mjs to independently reconstruct the unqualified history.

native-integrated-v1 retains current runtimec4b65d6:844native passes and all
15Boa comparisons faster beyond the fixed threshold. Same-executable controls
are15noise; preceding-source comparisons are11improvements4noise. Run
verify-native-integrated.mjs to check all raw values and77compiled source
hashes. Hosted, complete-corpus and safety qualification remain separate.
