mod support;
use jocky_ir::{ExecutionLimits, decode_fixture, decode_ir, execute, prepare};
use support::programs::{self, UBUNTU, WINDOWS};

#[test]
fn ten_reuses_of_verified_inputs_preserve_all_success_failure_and_accounting_bytes() {
    let catalogue = include_bytes!("../../forensic-lib/contracts/catalogue.v1.json");
    let registry = jocky_forensic::contracts::builtin_registry().unwrap();
    let before = jocky_ir::identity::registry_fingerprint(registry).unwrap();
    let triage = programs::triage();
    let recursive = programs::recursive();
    let unsupported = support::verify(
        decode_ir(include_bytes!(
            "../../language/tests/fixtures/ir/golden/cross-platform.ir.json"
        ))
        .unwrap(),
    )
    .unwrap();
    for bytes in [UBUNTU, WINDOWS] {
        let mut failure: serde_json::Value = serde_json::from_slice(bytes).unwrap();
        failure["providers"]["forensic.system.profile"] =
            serde_json::json!({"status":"error","code":"PermissionDenied"});
        for data in [bytes.to_vec(), serde_json::to_vec(&failure).unwrap()] {
            let limits = ExecutionLimits::default();
            let f = decode_fixture(&data, &limits).unwrap();
            for p in [&triage, &recursive, &unsupported] {
                let original = p.to_json().unwrap();
                let expected = execute(prepare(p, &f, &limits).unwrap()).to_json().unwrap();
                for _ in 0..10 {
                    let r = execute(prepare(p, &f, &limits).unwrap());
                    let accounting = *r.accounting();
                    assert_eq!(r.to_json().unwrap(), expected);
                    assert_eq!(r.to_json().unwrap(), expected);
                    assert_eq!(*r.accounting(), accounting);
                    assert_eq!(p.to_json().unwrap(), original);
                }
            }
        }
    }
    assert_eq!(
        jocky_ir::identity::registry_fingerprint(registry).unwrap(),
        before
    );
    assert_eq!(
        std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../forensic-lib/contracts/catalogue.v1.json"
        ))
        .unwrap(),
        catalogue
    );
}
