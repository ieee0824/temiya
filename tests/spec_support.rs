use std::{collections::HashSet, path::Path};

#[test]
fn support_matrix_has_valid_statuses_unique_ids_and_evidence() {
    let source = include_str!("../spec-support.json");
    let matrix: serde_json::Value = serde_json::from_str(source).unwrap();
    let features = matrix["features"].as_array().unwrap();
    let mut ids = HashSet::new();
    for feature in features {
        let id = feature["id"].as_str().unwrap();
        assert!(ids.insert(id), "duplicate feature id: {id}");
        let status = feature["status"].as_str().unwrap();
        assert!(matches!(
            status,
            "implemented" | "unimplemented" | "out-of-scope"
        ));
        assert!(!feature["reference"].as_str().unwrap().is_empty());
        assert!(!feature["rationale"].as_str().unwrap().is_empty());
        let tests = feature["tests"].as_array().unwrap();
        if status == "implemented" {
            assert!(
                !tests.is_empty(),
                "implemented feature lacks evidence: {id}"
            );
        }
        for test in tests {
            let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(test.as_str().unwrap());
            assert!(
                path.is_file(),
                "missing evidence for {id}: {}",
                path.display()
            );
        }
    }
}

#[test]
fn selected_editions_and_snapshots_are_immutable_identifiers() {
    let matrix: serde_json::Value =
        serde_json::from_str(include_str!("../spec-support.json")).unwrap();
    assert_eq!(matrix["target"]["standard"], "ECMA-262");
    assert_eq!(matrix["target"]["edition"], 17);
    assert_eq!(matrix["target"]["year"], 2026);
    let revision = matrix["proposal_snapshot"]["revision"].as_str().unwrap();
    assert_eq!(revision.len(), 40);
    assert!(revision.bytes().all(|byte| byte.is_ascii_hexdigit()));
}

#[test]
fn annex_b_proposals_and_host_hooks_are_all_tracked() {
    let matrix: serde_json::Value =
        serde_json::from_str(include_str!("../spec-support.json")).unwrap();
    let areas = matrix["features"]
        .as_array()
        .unwrap()
        .iter()
        .map(|feature| feature["area"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(areas.contains(&"Annex B"));
    assert!(areas.contains(&"host hooks"));
    assert!(areas.iter().any(|area| area.contains("proposal")));
}
