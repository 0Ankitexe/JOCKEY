use jocky_forensic::contracts::{LookupError, Registry, RegistryErrorKind, builtin_registry};
use serde_json::{Value, json};

fn document() -> Value {
    serde_json::from_str(include_str!("../contracts/catalogue.v1.json")).unwrap()
}

#[test]
fn catalogue_cache_iteration_and_lookup_are_real_and_deterministic() {
    let registry = builtin_registry().unwrap();
    assert!(std::ptr::eq(registry, builtin_registry().unwrap()));
    let names = registry
        .functions()
        .map(|f| f.qualified_name())
        .collect::<Vec<_>>();
    assert_eq!(names.len(), 10);
    assert!(names.windows(2).all(|n| n[0] < n[1]));
    assert_eq!(
        registry
            .lookup("forensic.process.list")
            .unwrap()
            .result
            .to_string(),
        "list<process_record>"
    );
    assert_eq!(
        registry.lookup("forensic.missing.call"),
        Err(LookupError::UnknownFunction)
    );
    assert_eq!(
        registry.lookup("forensic..call"),
        Err(LookupError::InvalidName)
    );
    let mut value = document();
    value["functions"].as_array_mut().unwrap().reverse();
    assert_eq!(
        Registry::from_json(&value.to_string())
            .unwrap()
            .functions()
            .map(|f| f.qualified_name())
            .collect::<Vec<_>>(),
        names
    );
}

#[test]
fn duplicate_keys_and_consistency_failures_are_typed() {
    use RegistryErrorKind::*;
    assert_eq!(
        Registry::from_json("{\"functions\":[],\"functions\":[]}")
            .unwrap_err()
            .kind,
        DuplicateJsonKey
    );
    assert_eq!(Registry::from_json("{").unwrap_err().kind, InvalidJson);
    let mut value = document();
    let first = value["functions"][0].clone();
    value["functions"].as_array_mut().unwrap().push(first);
    assert_eq!(
        Registry::from_json(&value.to_string()).unwrap_err().kind,
        DuplicateFunction
    );
    let mut value = document();
    value["functions"][1]["parameters"][1]["name"] = json!("target");
    assert_eq!(
        Registry::from_json(&value.to_string()).unwrap_err().kind,
        DuplicateParameter
    );
    let mut value = document();
    let first = value["functions"][0]["possible_errors"][0].clone();
    value["functions"][0]["possible_errors"]
        .as_array_mut()
        .unwrap()
        .push(first);
    assert_eq!(
        Registry::from_json(&value.to_string()).unwrap_err().kind,
        DuplicateErrorCode
    );
}

#[test]
fn byte_nesting_and_type_limits_reject_without_panicking() {
    assert_eq!(
        Registry::from_json(&" ".repeat(1_048_577))
            .unwrap_err()
            .kind,
        RegistryErrorKind::ResourceLimit
    );
    assert_eq!(
        Registry::from_json(&format!("{}0{}", "[".repeat(97), "]".repeat(97)))
            .unwrap_err()
            .kind,
        RegistryErrorKind::ResourceLimit
    );
    let mut value = document();
    let mut ty = json!({"kind":"named","name":"int"});
    for _ in 0..64 {
        ty = json!({"kind":"list","element":ty});
    }
    value["functions"][0]["result"] = ty;
    assert_eq!(
        Registry::from_json(&value.to_string()).unwrap_err().kind,
        RegistryErrorKind::ResourceLimit
    );
}

#[test]
fn synthetic_lab_and_zero_parameter_records_are_inert_valid_metadata() {
    let registry =
        Registry::from_json(include_str!("fixtures/valid/consistency-lab-registry.json")).unwrap();
    let contract = registry.lookup("forensic.lab.inspect").unwrap();
    assert!(contract.lab_only);
    assert!(contract.parameters.is_empty());
    assert_eq!(
        serde_json::to_value(contract).unwrap()["availability"],
        "unavailable"
    );
}

#[test]
fn exact_registry_boundaries_and_above_boundaries() {
    let base = document()["functions"][0].clone();
    let mut value = json!({"schema_version":"1.0.0","functions":[base.clone()]});
    let mut text = value.to_string();
    text.push_str(&" ".repeat(1_048_576 - text.len()));
    assert!(Registry::from_json(&text).is_ok());
    text.push(' ');
    assert_eq!(
        Registry::from_json(&text).unwrap_err().kind,
        RegistryErrorKind::ResourceLimit
    );
    value["functions"] = json!(
        (0..512)
            .map(|i| {
                let mut f = base.clone();
                f["name"] = json!(format!("f{i}"));
                f
            })
            .collect::<Vec<_>>()
    );
    assert_eq!(
        Registry::from_json(&value.to_string())
            .unwrap()
            .functions()
            .len(),
        512
    );
    value["functions"]
        .as_array_mut()
        .unwrap()
        .push(base.clone());
    assert_eq!(
        Registry::from_json(&value.to_string()).unwrap_err().reason,
        "registry-functions"
    );
    value = json!({"schema_version":"1.0.0","functions":[base.clone()]});
    value["functions"][0]["parameters"] = json!(
        (0..64)
            .map(|i| json!({"name":format!("p{i}"),"type":{"kind":"named","name":"int"}}))
            .collect::<Vec<_>>()
    );
    assert!(Registry::from_json(&value.to_string()).is_ok());
    value["functions"][0]["parameters"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"overflow","type":{"kind":"named","name":"int"}}));
    assert_eq!(
        Registry::from_json(&value.to_string()).unwrap_err().reason,
        "registry-parameters"
    );
    for (field, good, bad) in [
        ("name", "a".repeat(64), "a".repeat(65)),
        (
            "module",
            format!("{}.{}", "a".repeat(64), "b".repeat(63)),
            format!("{}.{}", "a".repeat(64), "b".repeat(64)),
        ),
        (
            "version",
            format!("0.1.0-{}", "a".repeat(58)),
            format!("0.1.0-{}", "a".repeat(59)),
        ),
    ] {
        value = json!({"schema_version":"1.0.0","functions":[base.clone()]});
        value["functions"][0][field] = json!(good);
        assert!(Registry::from_json(&value.to_string()).is_ok(), "{field}");
        value["functions"][0][field] = json!(bad);
        assert!(Registry::from_json(&value.to_string()).is_err(), "{field}");
    }
    value = json!({"schema_version":"1.0.0","functions":[base]});
    let mut ty = json!({"kind":"named","name":"int"});
    for _ in 1..64 {
        ty = json!({"kind":"list","element":ty});
    }
    value["functions"][0]["result"] = ty;
    assert!(Registry::from_json(&value.to_string()).is_ok());
}

#[test]
fn consistency_fixture_manifest_is_exercised() {
    let cases: Value = serde_json::from_str(include_str!("fixtures/registry-cases.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let text = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(case["file"].as_str().unwrap()),
        )
        .unwrap();
        let result = Registry::from_json(&text);
        if case["error"].is_null() {
            assert!(result.is_ok());
        } else {
            assert_eq!(
                format!("{:?}", result.unwrap_err().kind),
                case["error"].as_str().unwrap()
            );
        }
    }
}
