# outlined-slots-diagnostic-v1

Outline uncommon boxed-primitive, String and typed-view payloads. On aarch64 ObjectState shrinks 368 to 312 bytes and ObjectData 456 to 400 bytes; objects using these payloads incur an additional Box allocation. Five fixed workloads reduce instruction counts slightly; parameters increase by 0.067%. All-target tests: 932 passed, 26 ignored; Clippy passes; strict Miri passes for managed boxing/GC and all six array-buffer-view tests.

One instruction-count capture per fixed input. All adverse observations are retained. These diagnostics do not establish native speed qualification.
