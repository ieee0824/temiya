use temiya::test262::{
    Executor, HarnessError, Manifest, NegativePhase, ObservedFailure, ResultStatus,
    TEST262_REVISION, TestCase, TestVariant, run_test,
};

const MANIFEST: &str = include_str!("../test262/manifest.json");

struct FakeExecutor {
    observed: Result<(), ObservedFailure>,
    calls: Vec<(TestVariant, Vec<String>, String)>,
}

impl Executor for FakeExecutor {
    fn execute(
        &mut self,
        source: &str,
        variant: TestVariant,
        includes: &[String],
    ) -> Result<(), ObservedFailure> {
        self.calls
            .push((variant, includes.to_vec(), source.to_owned()));
        self.observed.clone()
    }
}

#[test]
fn manifest_revision_and_skip_reasons_are_reproducible() {
    let manifest = Manifest::from_json(MANIFEST).unwrap();
    assert_eq!(manifest.revision, TEST262_REVISION);
    let skip = manifest
        .skip("test/language/expressions/dynamic-import/basic.js")
        .unwrap();
    assert_eq!(skip.reason, "unsupported-host-hook");
    assert!(!skip.detail.is_empty());
}

#[test]
fn frontmatter_expands_default_and_constrained_variants() {
    let default = TestCase::parse(
        "test/default.js",
        "/*---\nincludes: [assert.js, sta.js]\nfeatures: [Symbol]\nflags: []\n---*/\nassert(true);",
    )
    .unwrap();
    assert_eq!(
        default.variants(),
        vec![TestVariant::Sloppy, TestVariant::Strict]
    );
    assert!(
        default
            .source_for(TestVariant::Strict)
            .starts_with("\"use strict\";")
    );
    assert_eq!(default.metadata.includes, ["assert.js", "sta.js"]);

    let module = TestCase::parse("test/module.js", "/*---\nflags: [module]\n---*/").unwrap();
    assert_eq!(module.variants(), vec![TestVariant::Module]);
    let raw = TestCase::parse("test/raw.js", "/*---\nflags: [raw]\n---*/").unwrap();
    assert_eq!(raw.variants(), vec![TestVariant::Sloppy]);
}

#[test]
fn negative_phase_and_error_type_must_both_match() {
    let case = TestCase::parse(
        "test/negative.js",
        "/*---\nflags: [onlyStrict]\nnegative:\n  phase: parse\n  type: SyntaxError\n---*/\ninvalid",
    )
    .unwrap();
    let manifest = Manifest::from_json(MANIFEST).unwrap();
    let mut executor = FakeExecutor {
        observed: Err(ObservedFailure {
            phase: NegativePhase::Parse,
            error_type: "SyntaxError".into(),
        }),
        calls: Vec::new(),
    };
    let result = run_test(&case, &manifest, &mut executor);
    assert_eq!(result[0].status, ResultStatus::Passed);
    assert_eq!(executor.calls[0].0, TestVariant::Strict);

    executor.observed = Err(ObservedFailure {
        phase: NegativePhase::Runtime,
        error_type: "SyntaxError".into(),
    });
    assert!(matches!(
        run_test(&case, &manifest, &mut executor)[0].status,
        ResultStatus::Failed(_)
    ));
}

#[test]
fn skips_do_not_invoke_the_executor() {
    let case = TestCase::parse(
        "test/language/expressions/dynamic-import/basic.js",
        "/*---\nflags: []\n---*/",
    )
    .unwrap();
    let manifest = Manifest::from_json(MANIFEST).unwrap();
    let mut executor = FakeExecutor {
        observed: Ok(()),
        calls: Vec::new(),
    };
    let results = run_test(&case, &manifest, &mut executor);
    assert!(
        results
            .iter()
            .all(|result| matches!(result.status, ResultStatus::Skipped { .. }))
    );
    assert!(executor.calls.is_empty());
}

#[test]
fn malformed_metadata_and_manifest_are_rejected() {
    assert!(matches!(
        TestCase::parse("bad.js", "no frontmatter"),
        Err(HarnessError(_))
    ));
    assert!(Manifest::from_json(r#"{"revision":"main","skips":[]}"#).is_err());
    assert!(TestCase::parse("bad.js", "/*---\nnegative:\n  phase: parse\n---*/",).is_err());
}
