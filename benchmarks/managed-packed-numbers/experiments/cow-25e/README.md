# Initial owned-descriptor trial, source25e46c7

Not accepted for merge. All10 reports each validate66 workers/1056 values;
source/build/compiler/adapter/control identities and statistics are independently
checked. No sample, pair or unfavorable observation is discarded.

| Cohort | Candidate/main | Candidate/actual modified Boa | Same binary |
| --- | --- | --- | --- |
| Local ARM64 guestCPU0 |1 shorter/32 noise/0 slower|30 faster/3 noise|not measured|
| Hosted AMD9V45 copy1 |20 noise/13 slower|32 faster/1 noise|33 noise|
| Hosted AMD9V45 copy2 |33 noise|33 faster|33 noise|

Array comparisons are all noise on ARM64 (ratios0.961–0.986); hosted copy1
has one slower array pair (1.136), and copy2 has three noise decisions.
Copy1 has further regressions in property, allocation, string and prototype
cases. Its primitive-string-method/Boa pair2 ratio0.922 fails the10% gate.
Different cohort outcomes do not establish a cause or erase any result.

The initial get interface returns Cow<StoredDescriptor>, materializing a full
owned descriptor just to read a packed Number. A small value/reference view
will be evaluated as a distinct runtime change to remove this read-path work.
That mechanism is an investigation hypothesis, not proof that it caused all
hosted regressions. Requested allocation bytes improved by9x; this does not
qualify execution speed.

The direct numeric-to-Object GC overwrite test was strengthened after timing,
without changing runtime code, and both native/Miri integration tests passed.
The example/worker/library code used in timing remains source25e46c7. Full
security results are recorded separately for the actual CI source4e13dc1.
