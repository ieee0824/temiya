//! Checked component measurements, with explicit API and timing boundaries.
use std::{cell::Cell, env, hint::black_box, rc::Rc, time::Instant};
use temiya::{
    Engine,
    embedding::ScriptBackend,
    gc::{Handle, Heap, Trace, Tracer},
    parser::parse_script,
    runtime::Value,
};

const PARSE_SOURCE: &str = "function 足す(a,b){return a+b;} 足す(20,22);";
const OBJECT_SETUP: &str = "function objects(){var total=0;for(var i=0;i<256;i++){var o={value:i};o.value+=1;Object.defineProperty(o,'double',{get:function(){return this.value*2;}});total+=o.double;delete o.value;total+=('value' in o)?1:0;}return total;}";
const CASES: [&str; 6] = [
    "parser-script",
    "engine-compile",
    "engine-startup",
    "objects-properties",
    "gc-full-cycle",
    "gc-minor-barrier",
];

struct Node {
    edge: Option<Handle<Node>>,
    drops: Rc<Cell<u64>>,
}
impl Trace for Node {
    fn trace(&self, tracer: &mut Tracer<'_>) {
        if let Some(edge) = self.edge {
            tracer.mark(edge);
        }
    }
}
impl Drop for Node {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}
fn check(condition: bool, message: &str) -> Result<(), Box<dyn std::error::Error>> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn duration(start: Instant) -> Result<u64, Box<dyn std::error::Error>> {
    Ok(u64::try_from(start.elapsed().as_nanos())?)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if cfg!(debug_assertions) {
        return Err("build this worker in release mode".into());
    }
    if cfg!(feature = "jit") {
        return Err("build this worker without features".into());
    }
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 2 || !CASES.contains(&args[0].as_str()) {
        return Err("usage: measure_components CASE SAMPLES".into());
    }
    let samples: usize = args[1].parse()?;
    if !(1..=1000).contains(&samples) {
        return Err("samples must be in 1..=1000".into());
    }
    let name = args[0].as_str();
    let engine = Engine::default();
    let realm = engine.create_realm();
    if name == "objects-properties" {
        let setup = engine.compile(&realm, OBJECT_SETUP)?;
        engine.evaluate(&setup)?;
    }
    let (group, batch, expected, scope) = match name {
        "parser-script" => (
            "parse",
            1024,
            2048,
            "parse_script plus AST checks and drop; no evaluation",
        ),
        "engine-compile" => (
            "parse",
            128,
            5376,
            "sum of Engine::compile intervals; evaluation and Script drop excluded",
        ),
        "engine-startup" => (
            "startup",
            64,
            2688,
            "sum of Engine construction plus create_realm intervals; correctness probe and teardown excluded",
        ),
        "objects-properties" => (
            "objects",
            256,
            65792,
            "parse+evaluate objects() invocation and Script drop; function setup excluded",
        ),
        "gc-full-cycle" => (
            "gc",
            16,
            4096,
            "standalone Heap graph allocation, rooted tracing, cycle reclamation and validation; not Engine GC",
        ),
        "gc-minor-barrier" => (
            "gc",
            64,
            128,
            "standalone Heap old-to-young barrier, minor/full collection and validation; not Engine GC",
        ),
        _ => unreachable!(),
    };
    let pass = || -> Result<(u64, u64), Box<dyn std::error::Error>> {
        let mut observed = 0u64;
        let mut timed = 0u64;
        let start = Instant::now();
        match name {
            "parser-script" => {
                for _ in 0..batch {
                    let program = parse_script(black_box(PARSE_SOURCE))?;
                    check(program.body.len() == 2, "AST body count changed")?;
                    observed += u64::try_from(program.body.len())?;
                    drop(black_box(program));
                }
            }
            "engine-compile" => {
                for _ in 0..batch {
                    let start = Instant::now();
                    let script = engine.compile(&realm, black_box(PARSE_SOURCE))?;
                    timed = timed
                        .checked_add(duration(start)?)
                        .ok_or("timer overflow")?;
                    check(
                        script.backend() == ScriptBackend::Bytecode,
                        "compile probe did not select Bytecode",
                    )?;
                    let result = engine.evaluate(&script)?;
                    let Value::Number(value) = result.value() else {
                        return Err("compile probe result type".into());
                    };
                    check(*value == 42.0, "compile probe result changed")?;
                    observed += 42;
                }
            }
            "engine-startup" => {
                for _ in 0..batch {
                    let start = Instant::now();
                    let fresh = Engine::default();
                    let fresh_realm = fresh.create_realm();
                    timed = timed
                        .checked_add(duration(start)?)
                        .ok_or("timer overflow")?;
                    let script = fresh.compile(&fresh_realm, "40+2;")?;
                    check(
                        fresh
                            .evaluate(&script)?
                            .value()
                            .same_value(&Value::Number(42.0)),
                        "fresh Realm probe failed",
                    )?;
                    observed += 42;
                }
            }
            "objects-properties" => {
                let script = engine.compile(&realm, "objects();")?;
                let result = engine.evaluate(&script)?;
                drop(script);
                let elapsed = duration(start)?;
                let Value::Number(value) = result.value() else {
                    return Err("objects result type".into());
                };
                check(*value == 65792.0, "objects oracle failed")?;
                return Ok((elapsed, *value as u64));
            }
            "gc-full-cycle" => {
                let drops = Rc::new(Cell::new(0));
                for _ in 0..batch {
                    let mut heap = Heap::new();
                    let mut roots = (0..256)
                        .map(|_| {
                            heap.allocate(Node {
                                edge: None,
                                drops: drops.clone(),
                            })
                        })
                        .collect::<Vec<_>>();
                    for index in 0..256 {
                        let next = roots[(index + 1) % 256].handle();
                        heap.with_mut(roots[index].handle(), |node| node.edge = Some(next))
                            .ok_or("missing graph node")?;
                    }
                    let root = roots.pop().ok_or("missing root")?;
                    let weak = root.handle().downgrade();
                    roots.clear();
                    let alive = heap.collect();
                    check(
                        alive.reclaimed == 0 && alive.retained == 256,
                        "rooted cycle was not retained",
                    )?;
                    heap.verify()?;
                    drop(root);
                    let dead = heap.collect();
                    check(
                        dead.reclaimed == 256
                            && dead.retained == 0
                            && weak.upgrade(&heap).is_none(),
                        "unrooted cycle not reclaimed",
                    )?;
                    heap.verify()?;
                }
                observed = drops.get();
            }
            "gc-minor-barrier" => {
                let drops = Rc::new(Cell::new(0));
                for _ in 0..batch {
                    let mut heap = Heap::new();
                    let old = heap.allocate(Node {
                        edge: None,
                        drops: drops.clone(),
                    });
                    heap.collect();
                    let young = heap.allocate(Node {
                        edge: None,
                        drops: drops.clone(),
                    });
                    let target = young.handle();
                    heap.with_mut(old.handle(), |node| node.edge = Some(target))
                        .ok_or("old owner missing")?;
                    heap.write_barrier(old.handle(), target)?;
                    heap.verify_write_barriers()?;
                    drop(young);
                    let stats = heap.collect_minor();
                    check(
                        stats.reclaimed == 0 && heap.with(target, |_| ()).is_some(),
                        "barrier did not retain young target",
                    )?;
                    heap.verify()?;
                    drop(old);
                    check(
                        heap.collect().reclaimed == 2 && heap.live_allocations() == 0,
                        "final graph not reclaimed",
                    )?;
                    heap.verify()?;
                }
                observed = drops.get();
            }
            _ => unreachable!(),
        }
        check(observed == expected, "component oracle failed")?;
        Ok((
            if matches!(name, "engine-compile" | "engine-startup") {
                timed
            } else {
                duration(start)?
            },
            observed,
        ))
    };
    let warmup = pass()?;
    let measured = (0..samples)
        .map(|_| pass())
        .collect::<Result<Vec<_>, _>>()?;
    println!(
        "{}",
        serde_json::json!({
            "engine":"Temiya", "profile":"release", "features":[], "architecture":env::consts::ARCH,
            "name":name,"group":group,"batch":batch,"expected":expected,"scope":scope,
            "source":if name=="parser-script"||name=="engine-compile" {PARSE_SOURCE}else if name=="objects-properties" {OBJECT_SETUP}else{""},
            "warmup_ns":warmup.0,"warmup_result":warmup.1,
            "samples_ns":measured.iter().map(|(ns,_)|*ns).collect::<Vec<_>>(),
            "observed_results":measured.iter().map(|(_,value)|*value).collect::<Vec<_>>(),
            "validated_passes":samples+1,
            "gc_api":if group=="gc" {Some("temiya::gc::Heap (standalone)")}else{None},
        })
    );
    Ok(())
}
