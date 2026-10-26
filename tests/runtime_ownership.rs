#[path = "../examples/support/ownership.rs"]
mod ownership;

#[test]
fn public_handles_keep_objects_closures_and_escaped_prototypes_usable() {
    for case in ownership::CASES {
        let observation =
            ownership::observe(case).unwrap_or_else(|error| panic!("{}: {error}", case.name));
        assert!(observation.alive_with_handle, "{observation:?}");
        assert!(observation.alive_with_clone, "{observation:?}");
        // Cyclic retention is measured by the strict CLI gate. Do not assert
        // that a known leak must persist after the collector is integrated.
        if observation.name == "acyclic-object" {
            assert!(
                !observation.alive_after_drop,
                "acyclic control leaked: {observation:?}"
            );
        }
    }
}
