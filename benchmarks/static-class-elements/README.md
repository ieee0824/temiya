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
