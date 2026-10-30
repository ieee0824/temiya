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
