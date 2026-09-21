use jocky_forensic::contracts::builtin_registry;
use jocky_language::{
    SourceFile,
    lowering::{BuildOptions, lower},
    semantic::analyze,
};

#[derive(serde::Deserialize)]
struct Golden {
    name: String,
    source: String,
    golden: String,
}
#[test]
fn reviewed_compiler_goldens_are_deterministic_and_round_trip() {
    let cases: Vec<Golden> =
        serde_json::from_str(include_str!("fixtures/ir/golden/manifest.json")).unwrap();
    assert!(cases.len() >= 12);
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    for case in cases {
        let text = std::fs::read_to_string(root.join(&case.source)).unwrap();
        let checked = analyze(
            SourceFile::new(&case.source, &text),
            builtin_registry().unwrap(),
        )
        .unwrap();
        let expected = std::fs::read(
            root.join("language/tests/fixtures/ir/golden")
                .join(&case.golden),
        )
        .unwrap();
        for _ in 0..10 {
            let ir = lower(&checked, BuildOptions::default()).unwrap();
            assert_eq!(ir.to_json().unwrap(), expected, "{}", case.name);
            assert_eq!(
                jocky_ir::verify(
                    jocky_ir::decode_ir(&expected).unwrap(),
                    checked.registry_snapshot()
                )
                .unwrap()
                .to_json()
                .unwrap(),
                expected
            );
        }
    }
}

#[test]
fn all_existing_examples_lower_without_changing_their_source() {
    for name in [
        "triage",
        "minimal",
        "comments",
        "control-flow",
        "types-and-literals",
        "cross-platform",
        "windows-events",
        "ubuntu-events",
    ] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("examples")
            .join(format!("{name}.jky"));
        let text = std::fs::read_to_string(path).unwrap();
        let label = format!("examples/{name}.jky");
        let checked = analyze(SourceFile::new(&label, &text), builtin_registry().unwrap()).unwrap();
        let program = lower(&checked, BuildOptions::default()).unwrap();
        let bytes = program.to_json().unwrap();
        for _ in 0..10 {
            assert_eq!(
                lower(&checked, BuildOptions::default())
                    .unwrap()
                    .to_json()
                    .unwrap(),
                bytes
            );
        }
        jocky_ir::verify(
            jocky_ir::decode_ir(&bytes).unwrap(),
            checked.registry_snapshot(),
        )
        .unwrap();
        assert!(program.document().functions.iter().all(|f| {
            f.span.is_some()
                && f.slots.iter().all(|s| s.span.is_some())
                && f.regions
                    .iter()
                    .flat_map(|r| &r.instructions)
                    .all(|i| i.span.is_some())
        }));
    }
}
