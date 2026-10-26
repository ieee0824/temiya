use std::{env, hint::black_box, time::Instant};

use temiya::{
    benchmark::Measurement,
    gc::{Heap, Trace, Tracer},
    parser::parse_script,
    runtime::{Object, Realm, ScriptRecord, Value},
};

struct Leaf(u64);
impl Trace for Leaf {
    fn trace(&self, _: &mut Tracer<'_>) {}
}

fn main() {
    if env::var("TEMIYA_RUN_BENCHMARKS").as_deref() != Ok("1") {
        return;
    }
    let samples = env::var("TEMIYA_BENCH_SAMPLES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(15);
    let iterations = env::var("TEMIYA_BENCH_ITERATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(100);
    let parser_source = "function sum(a, b) { return a + b; } sum(20, 22);";
    let macro_source = "(() => { let n = 0; for (let i = 0; i < 100; i++) n += i; return n; })();";

    let measurements = [
        measure("parser/script", "sum-function", samples, iterations, || {
            black_box(parse_script(black_box(parser_source)).unwrap());
        }),
        measure(
            "interpreter/macro",
            "sum-loop-100",
            samples,
            iterations,
            || {
                let realm = Realm::ordinary();
                black_box(
                    ScriptRecord::parse(realm, black_box(macro_source))
                        .unwrap()
                        .evaluate()
                        .unwrap(),
                );
            },
        ),
        measure("gc/full", "allocate-100", samples, iterations, || {
            let mut heap = Heap::new();
            let mut roots = (0..100)
                .map(|value| heap.allocate(Leaf(value)))
                .collect::<Vec<_>>();
            black_box(heap.with(roots[0].handle(), |leaf| leaf.0));
            roots.clear();
            black_box(heap.collect());
        }),
        measure(
            "builtins/object",
            "properties-100",
            samples,
            iterations,
            || {
                let object = Object::new();
                for index in 0..100 {
                    let key = index.to_string();
                    object
                        .create_data_property(key.as_str().into(), Value::Number(index as f64))
                        .unwrap();
                }
                black_box(object.get(&"99".into()).unwrap());
            },
        ),
    ];

    println!("{{");
    println!(
        "  \"revision\": {:?},",
        env::var("TEMIYA_REVISION").unwrap_or_default()
    );
    println!(
        "  \"rustc\": {:?},",
        env::var("TEMIYA_RUSTC").unwrap_or_default()
    );
    println!("  \"target\": {:?},", env::consts::ARCH);
    println!("  \"iterations\": {iterations},");
    println!("  \"measurements\": [");
    for (index, measurement) in measurements.iter().enumerate() {
        let summary = measurement.summary();
        println!(
            "    {{\"name\": {:?}, \"input\": {:?}, \"median_ns\": {}, \"mad_ns\": {}, \"samples_ns\": {:?}}}{}",
            measurement.name,
            measurement.input,
            summary.median_ns,
            summary.mad_ns,
            measurement.samples_ns,
            if index + 1 == measurements.len() {
                ""
            } else {
                ","
            }
        );
    }
    println!("  ]");
    println!("}}");

    fn measure(
        name: &str,
        input: &str,
        samples: usize,
        iterations: usize,
        mut operation: impl FnMut(),
    ) -> Measurement {
        let samples_ns = (0..samples)
            .map(|_| {
                let start = Instant::now();
                for _ in 0..iterations {
                    operation();
                }
                u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
            })
            .collect();
        Measurement::new(name, input, samples_ns).unwrap()
    }
}
