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

integrated-correctness-v1 retains every before/after variant for the merged
instance/static optimizations. Both547-variant backend lists match exactly,
including84failures; all463preceding passes are preserved. Run
verify-integrated-correctness.mjs to reconstruct the results and844native tests.

hosted-integrated-v1 retains all48 reports from c4b65d6. Both hosts pass the
static workloads but record only one significant class-construct win out of
three, so this source is not merge-qualified. verify-hosted.mjs independently
reconstructs both this result and the initial unqualified capture.

constructor-assignments-v1 records source612d759, which skips the unobservable
activation for strict base constructors containing only simple parameter-to-
property assignments. Ordinary setter dispatch and strict assignment failures
are preserved. All848 native tests pass, with26 ignored; Clippy passes. Both
fixed547-variant backend selections retain exactly463 passes and84 failures.
The complete fixed capture, binary/source identities and native base-suite raw
samples are retained. The shared-host native base suite records12/12 significant
Boa wins, but hosted qualification and full-corpus/safety checks remain pending.
Callgrind instruction counts decrease from323336903 to289996124; these profiler
counts are diagnostic and are not wall-clock performance qualification.

hosted-constructors-v1 retains all48 reports from source612d759 on two
unselected hosts (EPYC9V45 and9V74). Each workload meets the frozen minimum
two significant Boa wins out of three on each host. Candidate totals are105wins
on copy1 and104wins/1noise on copy2; derived-definition has2/3wins on copy2.
Class construction has3/3wins on both. Controls and historical failures remain
retained; verify-hosted.mjs reconstructs every report without waiving gates.

full-constructors-v1 retains the complete52133-file100211-variant run of
source612d759:27200pass64114fail18skip8879timeout, no missing results.
verify-full.mjs independently replays all eight known-pass gates. A direct
comparison with preceding main also preserves all26826 passes and adds374;
the comparison and raw records are retained. Native field controls additionally
retain all1008 previous passes in each backend:1252pass2054fail792timeout over
4098 variants, with identical Ast/Auto outcomes, in field-controls.tar.gz.
Safety and final-head CI remain independent merge requirements.
