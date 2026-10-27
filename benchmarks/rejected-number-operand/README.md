# Rejected Number-literal operand instruction

Candidate a80697ecc96ded51ad46d6d6200d9e374b4f398b and workflow-only equivalent
5500e8a023b0df2e94641dbb5c9c05dad153f469 combine a literal RHS load and binary
operation. The code patch is retained for reproduction and is not active code.
The candidate passed local all-feature tests, fixed Test26220 executions,
Clippy, native x64 stable/MSRV CI40563143606 and security40563149715.

Old-main comparisons looked favorable: ARM21 faster/12 noise, AMD9V45 25/8,
AMD7763 24/9; all three actual-Boa original-eleven reports had 33 faster pairs.
Those baselines predate merged inline dispatch and cannot isolate the new
instruction. A separate actual async report had all nine faster pairs on its
recorded host; cross-host async outcomes do not prove an opcode effect.

Direct comparisons against inline source37e09968 or equivalent merged main
9d67c8c instead produced:

- ARM: 2 faster/31 noise/0 slower; all arithmetic pairs noise, ratios .9611/.9609/.9557.
- Intel Platinum8573C: all33 noise; arithmetic .9330/.9255/.9180.
- AMD7763: 1 faster/30 noise/2 slower; arithmetic .9150/.9074/.9131.
  Slower prop-mono pairs 1/2 were 1.1091/1.1060 at the fixed .1 threshold.
Both direct x64 identical-executable controls had all33 noise; the ARM candidate
control has one faster label and32 noise. Controls are variability, not code
improvements. No arithmetic pair clears the unchanged threshold. No cause is
asserted for the slower pairs, and none is discarded.

The candidate is not adopted. Test success or an obsolete baseline comparison
does not justify shipping the extra instruction. The real Xeon6973P-C condition
in #510 remains unmet; the measured Platinum8573C is a different CPU. All eight
declared-target probes for this candidate were unavailable and are preserved.

Actual-Boa macro run40563811264 artifacts12763352792/12764297107, actual async
40563815879 artifact12762341855, and direct Temiya-only run40569637948 artifacts
12764049205/12764724312 are retained. Local ARM records preserve actual source
and native hashes. Direct reports are not Boa comparisons. Every original-eleven
report keeps15 samples plus checked warmup,3 alternating pairs,512MiB/180sec and
all result checks. Source, dependencies, compiler, binary, mode and guards were
independently verified. The patch's earlier pending-validation prose is a
historical snapshot, not the current adoption decision.

The rejected code patch is stored losslessly as rejected-code.patch.gz so that
significant patch-context whitespace remains intact. Decompress it to inspect
or reproduce the experiment; it is not part of the active instruction set.
