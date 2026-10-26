use std::{path::PathBuf, time::Duration};

use temiya::differential::{
    CommandEngine, DifferentialEngine, DifferentialResult, DifferentialRunner, KnownDifference,
    Normalizer, Observation, SafeInputGenerator, minimize_source, save_reproducer,
};

struct FakeEngine {
    name: &'static str,
    result: Observation,
}

impl DifferentialEngine for FakeEngine {
    fn name(&self) -> &str {
        self.name
    }

    fn evaluate(&mut self, _: &str, _: Duration) -> Observation {
        self.result.clone()
    }
}

#[test]
fn normalized_observations_match_across_engines() {
    let mut normalizer = Normalizer::new();
    normalizer.add_rule(r"time=\d+", "time=<dynamic>").unwrap();
    let mut runner = DifferentialRunner::new(Duration::from_secs(1));
    runner.set_normalizer(normalizer);
    runner.add_engine(FakeEngine {
        name: "a",
        result: Observation::Success("value  \r\ntime=123\n".into()),
    });
    runner.add_engine(FakeEngine {
        name: "b",
        result: Observation::Success("value\ntime=999".into()),
    });
    assert!(matches!(
        runner.run("print(1)"),
        DifferentialResult::Match(_)
    ));
}

#[test]
fn mismatches_and_known_differences_are_distinct() {
    let mut runner = DifferentialRunner::new(Duration::from_secs(1));
    runner.add_engine(FakeEngine {
        name: "a",
        result: Observation::Success("1".into()),
    });
    runner.add_engine(FakeEngine {
        name: "b",
        result: Observation::Error("TypeError".into()),
    });
    assert!(matches!(
        runner.run("print(1)"),
        DifferentialResult::Mismatch(_)
    ));
    runner.add_known_difference(KnownDifference::new("typed-array-edge", "Uint8Array").unwrap());
    assert!(matches!(
        runner.run("new Uint8Array(0)"),
        DifferentialResult::KnownDifference { id, .. } if id == "typed-array-edge"
    ));
}

#[test]
fn command_adapter_enforces_timeout_and_captures_output() {
    let mut echo = CommandEngine::new("cat", "sh", ["-c", "cat"]);
    assert_eq!(
        echo.evaluate("hello", Duration::from_secs(1)),
        Observation::Success("hello".into())
    );
    let mut sleeper = CommandEngine::new("sleep", "sh", ["-c", "sleep 1"]);
    assert_eq!(
        sleeper.evaluate("", Duration::from_millis(10)),
        Observation::Timeout
    );
}

#[test]
fn mismatch_is_minimized_and_saved() {
    let source = "const unused = 1;\nBAD();\nconst trailing = 2;";
    let minimized = minimize_source(source, |candidate| candidate.contains("BAD"));
    assert_eq!(minimized, "BAD();");

    let path = PathBuf::from(format!(
        "/tmp/temiya-differential-{}.js",
        std::process::id()
    ));
    save_reproducer(&path, &minimized).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "BAD();");
    std::fs::remove_file(path).unwrap();
}

#[test]
fn safe_generation_is_seed_reproducible() {
    let mut first = SafeInputGenerator::new(42);
    let mut second = SafeInputGenerator::new(42);
    for _ in 0..20 {
        let source = first.generate();
        assert_eq!(source, second.generate());
        assert!(!source.contains("while"));
        assert!(!source.contains("eval"));
    }
}
