# Object binding preparation (#163)

Runtime implementation has not started. Commit 75d6b2a freezes the complete
17,415-variant selection and six authored workloads before candidate timing.
The selection retains array, assignment, async and generator dependencies.
Every workload executes 10,000 iterations with expected sum 50,005,000.
The actual pinned Boa binary validates each oracle twice. This provides no
candidate speed qualification.

Run `node benchmarks/object-binding/verify-workloads.mjs` to verify the frozen
inputs, selection, reference identity and saved Boa output hashes.

Coverage planned: declarations and parameters, nested patterns, computed key
conversion order, defaults and TDZ, rest key exclusions, symbols, enumerability,
getter order/abrupt completions, null/undefined and primitive sources. Merge
after #161 and #162, with independent correctness and unchanged speed gates.
