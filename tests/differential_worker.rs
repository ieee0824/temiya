use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Stdio},
};

fn observe(source: &str, backend: &str) -> Value {
    let mut child = Command::new(env!("CARGO_BIN_EXE_temiya-differential-worker"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            serde_json::to_string(&json!({"source":source,"backend":backend}))
                .unwrap()
                .as_bytes(),
        )
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn actual_engine_preserves_number_utf16_and_primitive_types() {
    for backend in ["ast", "auto"] {
        let result = observe(
            "record(-0,0,NaN,Infinity,-Infinity,'\\uD800',1n,true,null,void 0);42",
            backend,
        );
        assert_eq!(result["comparison_eligible"], true);
        let values = &result["effects"][0];
        for (index, bits) in [
            (0, "8000000000000000"),
            (1, "0000000000000000"),
            (2, "NaN"),
            (3, "7ff0000000000000"),
            (4, "fff0000000000000"),
        ] {
            assert_eq!(values[index]["bits"], bits);
        }
        assert_eq!(values[5], json!({"type":"string","utf16":[55296]}));
        assert_eq!(values[6], json!({"type":"bigint","decimal":"1"}));
        assert_eq!(values[7], json!({"type":"boolean","value":true}));
        assert_eq!(values[8], json!({"type":"null"}));
        assert_eq!(values[9], json!({"type":"undefined"}));
        assert_eq!(result["completion"]["value"]["bits"], "4045000000000000");
    }
}

#[test]
fn actual_public_job_queue_preserves_nested_fifo_and_side_effect_order() {
    for backend in ["ast", "auto"] {
        let result = observe(
            "record('sync');queueJob('a',function(){record(1);queueJob('c',function(){record(3);});});queueJob('b',function(){record(2);});42",
            backend,
        );
        assert_eq!(result["comparison_eligible"], true);
        assert_eq!(result["jobs"], json!([[97], [98], [99]]));
        assert_eq!(result["effects"].as_array().unwrap().len(), 4);
        let kinds: Vec<_> = result["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["kind"].as_str().unwrap())
            .collect();
        assert_eq!(
            kinds,
            ["effect", "job", "effect", "job", "effect", "job", "effect"]
        );
        assert_eq!(result["job_errors"], json!([]));
    }
}

#[test]
fn throws_and_compile_errors_are_not_infrastructure_successes() {
    let thrown = observe("record('before');throw 7;", "auto");
    assert_eq!(thrown["completion"]["kind"], "throw");
    assert_eq!(thrown["completion"]["error"]["type"], "ThrownValue");
    assert_eq!(
        thrown["completion"]["error"]["thrown"]["bits"],
        "401c000000000000"
    );
    assert_eq!(thrown["effects"].as_array().unwrap().len(), 1);
    let parsed = observe("var = ;", "auto");
    assert_eq!(parsed["completion"]["kind"], "compile-error");
    let unsupported = observe("function f(){};f", "auto");
    assert_eq!(unsupported["comparison_eligible"], false);
}

#[test]
fn data_snapshots_preserve_cycles_aliases_and_later_mutations() {
    for backend in ["ast", "auto"] {
        let result = observe(
            "var o={x:1};o.self=o;record(o,o);o.x=2;record(o);o",
            backend,
        );
        assert_eq!(result["comparison_eligible"], true);
        let first = &result["effects"][0][0];
        let repeated = &result["effects"][0][1];
        let changed = &result["effects"][1][0];
        assert_eq!(first["id"], repeated["id"]);
        assert_eq!(first["id"], changed["id"]);
        assert_eq!(
            first["properties"][0]["descriptor"]["value"]["bits"],
            "3ff0000000000000"
        );
        assert_eq!(
            changed["properties"][0]["descriptor"]["value"]["bits"],
            "4000000000000000"
        );
        assert_eq!(
            first["properties"][1]["descriptor"]["value"],
            json!({"type":"reference","id":0})
        );
    }
}

#[test]
fn array_holes_descriptors_and_native_error_types_are_observed() {
    let result = observe("[1,,3]", "auto");
    assert_eq!(result["comparison_eligible"], true);
    assert_eq!(result["completion"]["value"]["kind"], "array");
    assert_eq!(
        result["completion"]["value"]["properties"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    let thrown = observe("throw new TypeError('message');", "auto");
    assert_eq!(thrown["comparison_eligible"], true);
    assert_eq!(
        thrown["completion"]["error"],
        json!({"type":"TypeError","thrown":null})
    );
    let getter = observe("({get x(){record('getter');return 1;}})", "auto");
    assert_eq!(getter["comparison_eligible"], false);
    assert_eq!(getter["effects"], json!([]));
}
