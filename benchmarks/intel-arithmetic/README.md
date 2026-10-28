# Original arithmetic instruction profile on Intel Xeon 6973P-C

Run [40617376161](https://github.com/ieee0824/temiya/actions/runs/40617376161), copy 4, artifact 12786290967 records actual Intel(R) Xeon(R) 6973P-C, Linux x64, four logical CPUs, rustc 1.99.1 and Valgrind 3.22.0. Source `87c64d3cf73999393cddcbb114d0a1075a54db2b` has the same production/test/example/Cargo paths as main `60eb4d3d8583076667d3fac67c33eabc2e458d14`. Baseline is `56164251391bea02c75bb1d8bbf2483dc03545ee`. Both diagnostic executables use optimized release, no features, debug line tables and `-C dwarf-version=4`; exact binary hashes are in provenance. The measured workflow is retained verbatim.

The original pinned omoikane fixture is unchanged: arithmetic starts at 1, runs 2,000,000 iterations, and returns 64. Each worker completed one checked warmup and one checked sample in actual Bytecode mode: four validated results. The original setup, invocation, source hashes and an independent BigInt calculation are checked offline. The earlier failed profile attempt used an erroneous initial 0 in the independent oracle; its raw evidence remains in `../regexp-cold-ascii/hosted/intel6973-final-40614644680-copy2`. Correcting that diagnostic never changed the workload, engine, oracle64 or native comparison threshold.

## Observed instruction contributions

| Whole instrumented process | Instruction references |
| --- | ---: |
| baseline5616425 | 7,322,533,045 |
| current87c64d3 | 7,154,510,797 |

The current exclusive annotation assigns 1,696,002,689 references (23.71%) to the VM run function at `src/bytecode/vm.rs`; compiler-inlined library code is shown in additional rows. The direct binary-dispatch row has 592,000,240 (8.27%) and its interpreter-source inlined arithmetic row has 364,000,098 (5.09%). These are instruction attribution rows, not measured hardware cycles or pure dispatch cost.

Binding rows remain substantial: `Binding::read` 240,000,090 (3.35%), `Binding::write` 176,000,154 (2.46%), `Environment::update_number_binding` 228,000,000 (3.19%), and `set_function_binding_at` 128,000,128 (1.79%), with further compiler-inlined library rows. Actual profile data therefore points to VM/value/binding work in addition to arithmetic.

The annotated integer-remainder helper contains 160,000,000 instruction references across its guards, conversions, remainder/sign operation and closing line (about 2.24% of the current process). The remainder/sign return line alone has 56,000,000 (0.78%); the floating remainder fallback line has no attributed references for this input. These inline line counts overlap the binary arithmetic function attribution above and must not be added to it. Division latency and branch prediction are not measured by Callgrind; a small instruction share does not prove a small native time share. This diagnostic does not establish a CPU-specific microarchitectural cause.

The lower whole-process instruction total is diagnostic evidence only. It includes startup/setup and both complete original passes; it is not a native speed ratio. Baseline already contains the earlier typed Number implementation. This comparison cannot attribute all earlier arithmetic improvement to the RegExp/string changes.

## Separate native performance acceptance

Native run40607319755 x64 copy1 measured actual Intel6973P-C with the same production source and pinned actual omoikane Boa007556ad. Arithmetic candidate/Boa ratios 0.852346 / 0.852616 / 0.804638 all beat the original median/MAD threshold. Baseline/Boa also wins all three; candidate/baseline and same-binary arithmetic controls are all within noise. Additional matched run40614644680 records arithmetic candidate/Boa ratios0.868631 /0.582859 /0.884288, again three faster decisions. Its complete original-eleven actual-Boa comparison wins all33; its baseline and same-binary reports retain slower observations. Full native data, memory observations and independent timing rechecks are in `../regexp-cold-ascii`, including earlier inconclusive/negative cohorts. Native timing and instruction diagnostics are separate evidence.

The other three jobs in the successful profile run and twenty jobs from five preceding corrected-oracle acquisition runs had unavailable target hardware. Their availability records are retained; a successful skipped job is never a profile or speed pass.

Verify stored bytes, decompressed bytes, exact input/provenance, the independent oracle and consistency among raw Callgrind totals, annotations, logs and actual worker results:

```sh
node benchmarks/intel-arithmetic/verify.mjs
node benchmarks/regexp-cold-ascii/verify-integrity.mjs
```

This completes the remaining instruction-profile evidence for #510. Broader parse/startup/objects/async/GC groups, application-wide performance and SpiderMonkey superiority remain separate unfinished work under #427/#120.
