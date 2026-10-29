use std::{collections::HashSet, path::Path};

use serde_json::{Value as Json, json};
use temiya::{
    Engine,
    embedding::{EmbedError, ScriptBackend},
    parser::parse_script,
    runtime::Value,
};

fn matrix() -> Json {
    serde_json::from_str(include_str!("../spec-support.json")).unwrap()
}

fn validate(matrix: &Json) -> Result<(), String> {
    let require = |condition: bool, message: &str| {
        if condition {
            Ok(())
        } else {
            Err(message.to_owned())
        }
    };
    require(matrix["schema"] == 2, "unsupported schema")?;
    let mut ids = HashSet::new();
    for collection in ["features", "capabilities", "components"] {
        let entries = matrix[collection].as_array().ok_or("missing collection")?;
        require(!entries.is_empty(), "empty collection")?;
        for entry in entries {
            let id = entry["id"].as_str().ok_or("missing id")?;
            require(!id.is_empty() && ids.insert(id), "empty/duplicate id")?;
            let status = entry["status"].as_str().ok_or("missing status")?;
            require(
                matches!(
                    status,
                    "implemented" | "partial" | "unconnected" | "unimplemented" | "out-of-scope"
                ),
                "invalid status",
            )?;
            require(
                entry["rationale"].as_str().is_some_and(|s| !s.is_empty()),
                "missing rationale",
            )?;
            let issues = entry["issues"].as_array().ok_or("missing issues")?;
            require(
                !issues.is_empty() && issues.iter().all(|i| i.as_u64().is_some_and(|n| n > 0)),
                "invalid issues",
            )?;
            let tests = entry["tests"].as_array().ok_or("missing tests")?;
            if matches!(status, "implemented" | "partial" | "unconnected") {
                require(
                    !tests.is_empty(),
                    "claimed support lacks regression evidence",
                )?;
            }
            for test in tests {
                let path = test.as_str().ok_or("invalid evidence path")?;
                require(
                    !Path::new(path).is_absolute() && !path.split('/').any(|part| part == ".."),
                    "evidence must be repository relative",
                )?;
                require(
                    Path::new(env!("CARGO_MANIFEST_DIR")).join(path).is_file(),
                    "missing evidence file",
                )?;
            }
            if collection != "components" {
                require(
                    entry["reference"].as_str().is_some_and(|s| !s.is_empty()),
                    "missing reference",
                )?;
                // Focused external suites cannot turn a whole feature into pass.
                require(
                    entry["test262"]["status"] == "not-run",
                    "unsupported conformance claim",
                )?;
            }
            if collection == "capabilities" {
                let evidence_status = match entry["evidence_kind"].as_str() {
                    Some("absence") => "unimplemented",
                    Some("stub" | "unconnected-boundary") => "unconnected",
                    Some("examples" | "known-gap") => "partial",
                    _ => return Err("invalid evidence kind".into()),
                };
                require(
                    status == evidence_status,
                    "observation cannot imply complete support",
                )?;
                require(
                    matches!(
                        entry["kind"].as_str(),
                        Some("syntax" | "method" | "global" | "constructor" | "accessor")
                    ),
                    "invalid capability kind",
                )?;
                let probes = entry["probes"].as_array().ok_or("missing probes")?;
                require(!probes.is_empty(), "empty probes")?;
                require(
                    tests.iter().any(|t| t == "tests/spec_support.rs"),
                    "probe runner is not linked",
                )?;
                for probe in probes {
                    require(
                        probe["source"].as_str().is_some_and(|s| !s.is_empty()),
                        "missing probe source",
                    )?;
                    let paths = probe["expected"]
                        .as_object()
                        .ok_or("missing path expectations")?;
                    require(
                        paths.len() == 4
                            && ["parser", "ast", "bytecode", "auto"]
                                .iter()
                                .all(|p| paths.contains_key(*p)),
                        "missing or unknown execution path",
                    )?;
                    require(
                        matches!(paths["parser"].as_str(), Some("accept" | "reject")),
                        "invalid parser result",
                    )?;
                    for path in ["ast", "bytecode", "auto"] {
                        require(
                            matches!(
                                paths[path].as_str(),
                                Some(
                                    "true"
                                        | "false"
                                        | "bytecode-unsupported"
                                        | "engine-error"
                                        | "TypeError"
                                        | "ReferenceError"
                                        | "SyntaxError"
                                        | "RangeError"
                                )
                            ),
                            "invalid runtime result",
                        )?;
                        require(
                            path == "bytecode" || paths[path] != "bytecode-unsupported",
                            "fallback path cannot count as bytecode rejection",
                        )?;
                    }
                    if paths.values().any(|v| v == "engine-error") {
                        require(
                            probe["diagnostic_contains"]
                                .as_str()
                                .is_some_and(|s| !s.is_empty()),
                            "engine failure must identify the unconnected boundary",
                        )?;
                    }
                }
            }
        }
    }
    let capabilities: HashSet<_> = matrix["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    for feature in matrix["features"].as_array().unwrap() {
        if let Some(members) = feature.get("capabilities") {
            let members = members.as_array().ok_or("invalid group members")?;
            let mut seen = HashSet::new();
            require(!members.is_empty(), "empty group members")?;
            for member in members {
                let id = member.as_str().ok_or("invalid group member id")?;
                require(
                    capabilities.contains(id) && seen.insert(id),
                    "unknown/duplicate group member",
                )?;
            }
        }
    }
    Ok(())
}

#[test]
fn support_matrix_has_valid_statuses_unique_ids_and_evidence() {
    validate(&matrix()).unwrap();
}

#[test]
fn selected_editions_and_snapshots_are_immutable_identifiers() {
    let matrix = matrix();
    assert_eq!(matrix["target"]["standard"], "ECMA-262");
    assert_eq!(matrix["target"]["edition"], 17);
    assert_eq!(matrix["target"]["year"], 2026);
    let revision = matrix["proposal_snapshot"]["revision"].as_str().unwrap();
    assert_eq!(revision.len(), 40);
    assert!(revision.bytes().all(|byte| byte.is_ascii_hexdigit()));
}

#[test]
fn annex_b_proposals_and_host_hooks_are_all_tracked() {
    let matrix = matrix();
    let areas: Vec<_> = matrix["features"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["area"].as_str().unwrap())
        .collect();
    assert!(areas.contains(&"Annex B"));
    assert!(areas.contains(&"host hooks"));
    assert!(areas.iter().any(|area| area.contains("proposal")));
}

#[test]
fn invalid_or_inflated_evidence_is_rejected() {
    for (pointer, replacement) in [
        ("/features/0/status", json!("pass")),
        ("/features/0/issues", json!([])),
        ("/capabilities/0/tests", json!([])),
        ("/capabilities/0/tests/0", json!("../README.md")),
        ("/capabilities/0/probes", json!([])),
        ("/capabilities/0/test262/status", json!("pass")),
        ("/capabilities/0/status", json!("implemented")),
        ("/capabilities/0/probes/0/expected", json!({"auto":"true"})),
        (
            "/capabilities/0/probes/0/expected/auto",
            json!("bytecode-unsupported"),
        ),
    ] {
        let mut bad = matrix();
        *bad.pointer_mut(pointer).unwrap() = replacement;
        assert!(validate(&bad).is_err(), "accepted mutation {pointer}");
    }
    let mut bad = matrix();
    bad["capabilities"][1]["id"] = bad["capabilities"][0]["id"].clone();
    assert!(validate(&bad).is_err());
    let mut bad = matrix();
    bad["features"][0]["capabilities"] = json!(["missing-capability"]);
    assert!(validate(&bad).is_err());
}

fn observe(backend: ScriptBackend, source: &str) -> (String, String) {
    let engine = Engine::builder().script_backend(backend).build();
    let realm = engine.create_realm();
    let script = match engine.compile(&realm, source) {
        Ok(script) => script,
        Err(EmbedError::Bytecode(error)) => {
            return (
                if error.message().contains("unsupported") {
                    "bytecode-unsupported"
                } else {
                    "unexpected-bytecode-error"
                }
                .into(),
                error.to_string(),
            );
        }
        Err(error) => return ("unexpected-compile-error".into(), error.to_string()),
    };
    match engine.evaluate(&script) {
        Ok(result) => match result.value() {
            Value::Boolean(value) => (value.to_string(), String::new()),
            value => ("unexpected-value".into(), format!("{value:?}")),
        },
        Err(error) => {
            let kind = format!("{:?}", error.kind());
            (
                if kind == "Engine" {
                    "engine-error".into()
                } else {
                    kind
                },
                error.message().into(),
            )
        }
    }
}

#[test]
fn individual_probes_run_through_parser_and_public_engine_backends() {
    let matrix = matrix();
    validate(&matrix).unwrap();
    let mut failures = Vec::new();
    let mut observations = Vec::new();
    for entry in matrix["capabilities"].as_array().unwrap() {
        let id = entry["id"].as_str().unwrap();
        for (index, probe) in entry["probes"].as_array().unwrap().iter().enumerate() {
            let source = probe["source"].as_str().unwrap();
            let parsed = if parse_script(source).is_ok() {
                "accept"
            } else {
                "reject"
            };
            if probe["expected"]["parser"] != parsed {
                failures.push(format!("{id}/parser: {parsed}"));
            }
            observations.push(json!({"id":id,"probe":index,"source":source,"path":"parser","expected":probe["expected"]["parser"],"observed":parsed}));
            for (path, backend) in [
                ("ast", ScriptBackend::Ast),
                ("bytecode", ScriptBackend::Bytecode),
                ("auto", ScriptBackend::Auto),
            ] {
                let (observed, detail) = observe(backend, source);
                if probe["expected"][path] != observed {
                    failures.push(format!(
                        "{id}/{path}: expected {}, observed {observed}: {detail}\n{source}",
                        probe["expected"][path]
                    ));
                }
                if observed == "engine-error"
                    && !detail.contains(
                        probe["diagnostic_contains"]
                            .as_str()
                            .unwrap_or("missing boundary"),
                    )
                {
                    failures.push(format!("{id}/{path}: unrelated engine failure: {detail}"));
                }
                observations.push(json!({"id":id,"probe":index,"source":source,"path":path,"expected":probe["expected"][path],"observed":observed,"detail":detail}));
            }
        }
    }
    if let Some(path) = std::env::var_os("SPEC_SUPPORT_REPORT") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(
                &json!({"schema":1,"observations":observations,"failures":failures}),
            )
            .unwrap(),
        )
        .unwrap();
    }
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
    eprintln!(
        "{} capabilities, {} observations, no skipped probes; not a Test262 conformance result",
        matrix["capabilities"].as_array().unwrap().len(),
        observations.len()
    );
}
