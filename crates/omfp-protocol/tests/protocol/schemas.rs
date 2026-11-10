//! JSON Schemas of method params and results, for deriving agent tools.

use omfp_protocol::{METHODS, method_schemas};
use serde_json::{Value, json};

// spec: AGT-007
#[test]
fn every_method_has_params_and_result_schemas() {
    let schemas = method_schemas();
    let object = schemas.as_object().expect("schemas keyed by method name");
    let mut names: Vec<&str> = object.keys().map(String::as_str).collect();
    let mut expected = METHODS.to_vec();
    names.sort_unstable();
    expected.sort_unstable();
    assert_eq!(names, expected);
    for (name, entry) in object {
        assert!(entry["params"].is_object(), "{name} params");
        assert!(entry["result"].is_object(), "{name} result");
    }
}

// spec: AGT-007
#[test]
fn params_schemas_describe_the_wire_fields() {
    let schemas = method_schemas();
    let prompt = &schemas["thread.prompt"]["params"];
    assert_eq!(prompt["type"], json!("object"));
    let mut required: Vec<&str> = prompt["required"]
        .as_array()
        .expect("required list")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    required.sort_unstable();
    assert_eq!(required, ["text", "threadId"]);
    assert!(prompt["properties"]["behavior"].is_object());

    let subscribe = &schemas["subscribe"]["params"];
    assert!(subscribe["properties"]["since"].is_object());
}

// spec: AGT-007
#[test]
fn method_schemas_golden() {
    insta::assert_json_snapshot!("method_schemas", method_schemas());
}
