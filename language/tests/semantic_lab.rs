use jocky_forensic::contracts::{ImplementationAvailability, Registry, RegistryErrorKind};
use jocky_language::{
    SourceFile, Span, parse_source,
    semantic::{CheckFailure, DiagnosticCode, Severity, analyze},
};
use jocky_shared::compiler::{Capability, Platform, Privilege};
use serde::Deserialize;
use serde_json::Value;
use std::path::Path;

const LAB: &str =
    include_str!("../../forensic-lib/tests/fixtures/valid/consistency-lab-registry.json");
#[test]
fn lab_privilege_dominates_user_and_elevated_dependencies_transitively() {
    let mut doc: Value = serde_json::from_str(include_str!(
        "../../forensic-lib/contracts/catalogue.v1.json"
    ))
    .unwrap();
    let mut lab: Value = serde_json::from_str(LAB).unwrap();
    lab["functions"][0]["supported_platforms"] = serde_json::json!(["ubuntu"]);
    doc["functions"]
        .as_array_mut()
        .unwrap()
        .push(lab["functions"][0].clone());
    let registry = Registry::from_json(&doc.to_string()).unwrap();
    let text = "module mixed target ubuntu profile lab fn helper(target: endpoint) -> list<driver_record> { forensic.driver.list(target) return forensic.lab.inspect() } fn main(target: endpoint) -> forensic_result { helper(target) return forensic.system.profile(target) } run main on selected_endpoints";
    let checked = analyze(SourceFile::new("not-a-host", text), &registry).unwrap();
    let entry = checked.entry_metadata();
    assert_eq!(entry.required_privilege(), Privilege::LabOnly);
    assert_eq!(
        entry.capabilities(),
        [Capability::System, Capability::Drivers]
    );
    assert_eq!(entry.supported_platforms(), [Platform::Ubuntu]);
    assert_eq!(
        entry.unavailable_dependencies().collect::<Vec<_>>(),
        [
            "forensic.driver.list",
            "forensic.lab.inspect",
            "forensic.system.profile"
        ]
    );
    let rejected = text.replace("profile lab", "");
    let CheckFailure::Semantic(errors) =
        analyze(SourceFile::new("lab", &rejected), &registry).unwrap_err()
    else {
        panic!()
    };
    assert_eq!(errors.items().len(), 1);
    assert_eq!(errors.items()[0].code(), DiagnosticCode::LabProfileRequired);
}

#[derive(Deserialize)]
struct Case {
    file: String,
    code: String,
    span: Span,
    entry_lab: bool,
}

#[test]
fn explicit_opt_in_is_required_in_direct_indirect_dead_and_uncalled_code() {
    let registry = Registry::from_json(LAB).unwrap();
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/semantic/lab/cases.json")).unwrap();
    assert_eq!(cases.len(), 6);
    for case in cases {
        let text = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/semantic/lab")
                .join(&case.file),
        )
        .unwrap();
        let source = SourceFile::new("lab/windows/ubuntu/nonexistent", &text);
        assert!(parse_source(source).unwrap().profile.is_none());
        let CheckFailure::Semantic(diagnostics) = analyze(source, &registry).unwrap_err() else {
            panic!()
        };
        let errors = diagnostics
            .items()
            .iter()
            .filter(|d| d.severity() == Severity::Error)
            .collect::<Vec<_>>();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code().as_str(), case.code);
        assert_eq!(errors[0].span, case.span);
        let opted = text.replacen(
            "target windows | ubuntu\n",
            "target windows | ubuntu\nprofile lab\n",
            1,
        );
        let checked = analyze(SourceFile::new("still-not-a-file", &opted), &registry).unwrap();
        let profile = checked.profile().unwrap();
        assert_eq!(&opted[profile.span.start..profile.span.end], "profile lab");
        assert_eq!(
            &opted[profile.name_span.start..profile.name_span.end],
            "lab"
        );
        assert_eq!(checked.selected_targets(), Platform::ALL);
        let lab_function = checked
            .function_metadata()
            .iter()
            .find(|m| m.required_privilege() == Privilege::LabOnly)
            .unwrap();
        assert_eq!(lab_function.capabilities(), [Capability::Drivers]);
        assert_eq!(
            lab_function.unavailable_dependencies().collect::<Vec<_>>(),
            ["forensic.lab.inspect"]
        );
        assert_eq!(
            checked.entry_metadata().required_privilege(),
            if case.entry_lab {
                Privilege::LabOnly
            } else {
                Privilege::User
            }
        );
        let contract = registry.lookup("forensic.lab.inspect").unwrap();
        assert_eq!(
            contract.availability,
            ImplementationAvailability::Unavailable
        );
        assert!(contract.lab_only && contract.read_only);
        assert_eq!(
            Registry::from_json(LAB)
                .unwrap()
                .lookup("forensic.lab.inspect")
                .unwrap(),
            contract
        );
    }
}

#[test]
fn unknown_repeated_misplaced_profiles_fail_without_a_checked_program() {
    let ordinary = include_str!("fixtures/semantic/lab/lab-without-profile.jky");
    let registry = Registry::from_json(LAB).unwrap();
    for text in [
        ordinary.replacen("fn main", "profile standard\nfn main", 1),
        ordinary.replacen("fn main", "profile lab\nprofile lab\nfn main", 1),
        ordinary.replacen("target windows", "profile lab\ntarget windows", 1),
        ordinary.replacen("run main", "profile lab\nrun main", 1),
    ] {
        let CheckFailure::Syntax(errors) =
            analyze(SourceFile::new("profile lab", &text), &registry).unwrap_err()
        else {
            panic!()
        };
        assert!(
            errors
                .items()
                .iter()
                .any(|d| d.category.as_str() == "invalid-profile-declaration")
        );
    }
}

#[test]
fn contradictory_or_unknown_privileges_never_become_a_registry() {
    for (privilege, lab) in [
        ("user", true),
        ("elevated", true),
        ("lab_only", false),
        ("administrator", false),
    ] {
        let mut document: Value = serde_json::from_str(LAB).unwrap();
        document["functions"][0]["required_privilege"] = privilege.into();
        document["functions"][0]["lab_only"] = lab.into();
        assert_eq!(
            Registry::from_json(&document.to_string()).unwrap_err().kind,
            RegistryErrorKind::SchemaViolation
        );
    }
}

#[test]
fn lab_profile_does_not_relax_platform_or_type_requirements() {
    let mut document: Value = serde_json::from_str(LAB).unwrap();
    document["functions"][0]["supported_platforms"] = serde_json::json!(["ubuntu"]);
    let registry = Registry::from_json(&document.to_string()).unwrap();
    let text = include_str!("fixtures/semantic/lab/lab-without-profile.jky")
        .replace("target windows | ubuntu", "target windows profile lab")
        .replace("forensic.lab.inspect()", "forensic.lab.inspect(1)");
    let CheckFailure::Semantic(errors) =
        analyze(SourceFile::new("lab", &text), &registry).unwrap_err()
    else {
        panic!()
    };
    assert!(
        errors
            .items()
            .iter()
            .any(|d| d.code() == DiagnosticCode::UnsupportedTarget)
    );
    assert!(
        errors
            .items()
            .iter()
            .any(|d| d.code() == DiagnosticCode::ArgumentCountMismatch)
    );
    assert!(
        !errors
            .items()
            .iter()
            .any(|d| d.code() == DiagnosticCode::LabProfileRequired)
    );
}
