use temiya::benchmark::{ConformanceBaseline, Measurement, RegressionPolicy};

#[test]
fn summaries_use_median_and_mad() {
    let measurement = Measurement::new("parser", "small", vec![90, 100, 110, 10_000]).unwrap();
    let summary = measurement.summary();
    assert_eq!(summary.median_ns, 110);
    assert_eq!(summary.mad_ns, 20);
}

#[test]
fn conformance_counts_require_matching_revisions() {
    let baseline = ConformanceBaseline::new("fixed-revision", 80, 15, 5).unwrap();
    let current = ConformanceBaseline::new("fixed-revision", 82, 13, 5).unwrap();
    let delta = baseline.compare(&current).unwrap();
    assert_eq!(baseline.total(), 100);
    assert_eq!(delta.passed, 2);
    assert_eq!(delta.failed, -2);
    assert!(
        baseline
            .compare(&ConformanceBaseline::new("other", 82, 13, 5).unwrap())
            .is_err()
    );
    assert!(ConformanceBaseline::new("", 0, 0, 0).is_err());
}

#[test]
fn policy_distinguishes_noise_from_regression() {
    let baseline = Measurement::new("parser", "small", vec![98, 100, 102]).unwrap();
    let noisy = Measurement::new("parser", "small", vec![103, 106, 109]).unwrap();
    let slow = Measurement::new("parser", "small", vec![129, 130, 131]).unwrap();
    let policy = RegressionPolicy {
        minimum_slowdown: 0.10,
        noise_multiplier: 3.0,
    };
    assert!(!policy.compare(&baseline, &noisy).unwrap().regression);
    assert!(policy.compare(&baseline, &slow).unwrap().regression);
}

#[test]
fn comparison_rejects_mismatched_inputs() {
    let baseline = Measurement::new("parser", "small", vec![100]).unwrap();
    let current = Measurement::new("parser", "large", vec![100]).unwrap();
    let policy = RegressionPolicy {
        minimum_slowdown: 0.10,
        noise_multiplier: 3.0,
    };
    assert!(policy.compare(&baseline, &current).is_err());
    assert!(Measurement::new("", "input", vec![1]).is_err());
    assert!(Measurement::new("name", "input", Vec::new()).is_err());
}
