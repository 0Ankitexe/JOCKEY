use jocky_forensic::contracts::{Registry, builtin_registry};
use jocky_language::{SourceFile, semantic::analyze};
use jocky_shared::compiler::{Capability, Platform, Privilege};
use serde_json::Value;

#[test]
fn recursive_components_have_complete_deduplicated_closure_without_execution() {
    let text = "module cycles target windows | ubuntu fn self_call(target: endpoint) -> forensic_result { forensic.process.list(target) return self_call(target) } fn left(target: endpoint) -> forensic_result { forensic.driver.list(target) return right(target) } fn right(target: endpoint) -> forensic_result { forensic.network.connections(target) return left(target) } fn main(target: endpoint) -> forensic_result { self_call(target) left(target) left(target) return forensic.system.profile(target) } fn pure(value: int) -> int { return value } run main on selected_endpoints";
    let checked = analyze(
        SourceFile::new("not/executable", text),
        builtin_registry().unwrap(),
    )
    .unwrap();
    let meta = checked.function_metadata();
    assert_eq!(
        meta.iter().map(|m| m.name()).collect::<Vec<_>>(),
        [
            "cycles.self_call",
            "cycles.left",
            "cycles.right",
            "cycles.main",
            "cycles.pure"
        ]
    );
    assert_eq!(meta[0].capabilities(), [Capability::Processes]);
    for index in [1, 2] {
        assert_eq!(meta[index].required_privilege(), Privilege::Elevated);
        assert_eq!(
            meta[index].capabilities(),
            [Capability::Network, Capability::Drivers]
        );
        assert_eq!(
            meta[index].unavailable_dependencies().collect::<Vec<_>>(),
            ["forensic.driver.list", "forensic.network.connections"]
        );
    }
    assert_eq!(checked.entry_metadata(), &meta[3]);
    assert_eq!(
        meta[3].capabilities(),
        [
            Capability::System,
            Capability::Processes,
            Capability::Network,
            Capability::Drivers
        ]
    );
    assert_eq!(meta[3].required_privilege(), Privilege::Elevated);
    assert_eq!(meta[4].required_privilege(), Privilege::User);
    assert_eq!(meta[4].supported_platforms(), Platform::ALL);
    assert!(meta[4].capabilities().is_empty());
    assert_eq!(meta[4].unavailable_dependencies().len(), 0);
}

#[test]
fn deep_chain_and_dense_shared_graphs_converge_to_the_same_dependencies() {
    for (count, dense) in [(1024, false), (64, true)] {
        let mut text = String::from("module graph target ubuntu\n");
        for i in 0..count {
            text.push_str(&format!("fn f{i}(target: endpoint) -> forensic_result {{ "));
            if dense {
                for next in 0..count {
                    text.push_str(&format!("f{next}(target) "));
                }
            } else if i + 1 < count {
                text.push_str(&format!("f{}(target) ", i + 1));
            }
            if i == count - 1 {
                text.push_str("forensic.driver.list(target) ");
            }
            text.push_str("return forensic.system.profile(target) }\n");
        }
        text.push_str("run f0 on selected_endpoints\n");
        let checked = analyze(
            SourceFile::new("graph-only", &text),
            builtin_registry().unwrap(),
        )
        .unwrap();
        assert_eq!(checked.function_metadata().len(), count);
        for summary in checked.function_metadata() {
            assert_eq!(summary.required_privilege(), Privilege::Elevated);
            assert_eq!(
                summary.capabilities(),
                [Capability::System, Capability::Drivers]
            );
            assert_eq!(
                summary.unavailable_dependencies().collect::<Vec<_>>(),
                ["forensic.driver.list", "forensic.system.profile"]
            );
            assert_eq!(summary.supported_platforms(), Platform::ALL);
        }
    }
}

#[test]
fn all_eight_capabilities_and_composition_are_canonical_under_registry_permutation() {
    let text = "module every target windows | ubuntu fn all(target: endpoint, stamp: timestamp, location: path) -> forensic_result { forensic.event.list(target, stamp, stamp, 1) forensic.driver.list(target) forensic.persistence.list(target) forensic.file.inspect(target, location) let system = forensic.system.profile(target) let processes = forensic.process.list(target) let connections = forensic.network.connections(target) let indicators = forensic.indicator.correlate(system, processes, connections) return forensic_result(system, indicators) } fn main(target: endpoint) -> forensic_result { return forensic.system.profile(target) } run main on selected_endpoints";
    let original = analyze(SourceFile::new("same", text), builtin_registry().unwrap()).unwrap();
    let all = &original.function_metadata()[0];
    assert_eq!(all.capabilities(), Capability::ALL);
    assert_eq!(all.required_privilege(), Privilege::Elevated);
    assert_eq!(
        all.unavailable_dependencies().collect::<Vec<_>>(),
        [
            "forensic.driver.list",
            "forensic.event.list",
            "forensic.file.inspect",
            "forensic.indicator.correlate",
            "forensic.network.connections",
            "forensic.persistence.list",
            "forensic.process.list",
            "forensic.system.profile",
            "forensic_result"
        ]
    );
    assert_eq!(
        original.entry_metadata().capabilities(),
        [Capability::System]
    );
    let mut document: Value = serde_json::from_str(include_str!(
        "../../forensic-lib/contracts/catalogue.v1.json"
    ))
    .unwrap();
    for rotation in 0..10 {
        document["functions"]
            .as_array_mut()
            .unwrap()
            .rotate_left(rotation);
        for function in document["functions"].as_array_mut().unwrap() {
            function["supported_platforms"]
                .as_array_mut()
                .unwrap()
                .reverse();
        }
        let registry = Registry::from_json(&document.to_string()).unwrap();
        let actual = analyze(SourceFile::new("same", text), &registry).unwrap();
        assert_eq!(actual.function_metadata(), original.function_metadata());
        assert_eq!(actual.entry_metadata(), original.entry_metadata());
    }
}

#[test]
fn declaration_order_is_retained_but_does_not_change_named_summaries() {
    let functions = [
        "fn z(target: endpoint) -> forensic_result { return a(target) }",
        "fn a(target: endpoint) -> forensic_result { forensic.process.list(target) return forensic.system.profile(target) }",
    ];
    let text = |reverse: bool| {
        format!(
            "module order target windows {} {} run z on selected_endpoints",
            functions[usize::from(reverse)],
            functions[usize::from(!reverse)]
        )
    };
    let first = analyze(
        SourceFile::new("x", &text(false)),
        builtin_registry().unwrap(),
    )
    .unwrap();
    let second = analyze(
        SourceFile::new("x", &text(true)),
        builtin_registry().unwrap(),
    )
    .unwrap();
    assert_eq!(first.function_metadata()[0], second.function_metadata()[1]);
    assert_eq!(first.function_metadata()[1], second.function_metadata()[0]);
    assert_eq!(first.entry_metadata(), second.entry_metadata());
}
