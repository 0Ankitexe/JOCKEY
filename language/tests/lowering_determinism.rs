#[path = "support/artifacts.rs"]
mod artifacts;
use artifacts::OwnedDir;
use jocky_language::{
    SourceFile,
    lowering::{BuildOptions, lower},
    semantic::analyze,
};
use std::{
    process::Command,
    time::{Duration, Instant},
};

#[test]
fn ten_builds_per_seed_preserve_values_and_original_newline_spans() {
    let source = include_str!("../../examples/triage.jky");
    let registry = jocky_forensic::contracts::builtin_registry().unwrap();
    let limits = jocky_ir::ExecutionLimits::default();
    let fixtures = [
        include_bytes!("../../examples/fixtures/triage/ubuntu/fixture.json").as_slice(),
        include_bytes!("../../examples/fixtures/triage/windows/fixture.json").as_slice(),
    ];
    let mut hashes = Vec::new();
    for text in [source.to_owned(), source.replace('\n', "\r\n")] {
        let checked = analyze(SourceFile::new("logical/triage.jky", &text), registry).unwrap();
        let mut first_ids = Vec::new();
        for seed in [0, 42, u64::MAX] {
            let p = lower(&checked, BuildOptions { seed }).unwrap();
            let bytes = p.to_json().unwrap();
            first_ids.push(
                p.document().functions[0].regions[0].instructions[0]
                    .id
                    .clone(),
            );
            for _ in 0..10 {
                assert_eq!(
                    lower(&checked, BuildOptions { seed })
                        .unwrap()
                        .to_json()
                        .unwrap(),
                    bytes
                );
            }
            for (f, expected) in fixtures.iter().zip([
                include_str!("../../examples/fixtures/triage/ubuntu/expected-result.json"),
                include_str!("../../examples/fixtures/triage/windows/expected-result.json"),
            ]) {
                let f = jocky_ir::decode_fixture(f, &limits).unwrap();
                let r = jocky_ir::execute(jocky_ir::prepare(&p, &f, &limits).unwrap());
                let raw: serde_json::Value = serde_json::from_slice(&r.to_json().unwrap()).unwrap();
                assert_eq!(
                    raw["outcome"]["value"],
                    serde_json::from_str::<serde_json::Value>(expected).unwrap()
                );
            }
            for i in &p.document().functions[0].regions[0].instructions {
                let span = i.span.unwrap();
                assert!(!text[span.start..span.end].is_empty());
                if matches!(i.operation, jocky_ir::model::Operation::Call { .. }) {
                    assert!(text[span.start..span.end].contains('('));
                }
            }
        }
        assert!(first_ids.windows(2).all(|pair| pair[0] != pair[1]));
        hashes.push(
            lower(&checked, BuildOptions::default())
                .unwrap()
                .document()
                .source
                .sha256
                .clone(),
        );
    }
    assert_ne!(hashes[0], hashes[1]);
}

#[test]
fn ten_binary_builds_and_each_platform_run_ignore_cwd_environment_and_output_name() {
    let dir = OwnedDir::new("deterministic");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let source = root.join("examples/triage.jky");
    let mut expected_ir = None;
    let mut reports = [None, None];
    for n in 0..10 {
        let output = dir.path(&format!("artifact-{n}.json"));
        let start = Instant::now();
        let r = Command::new(env!("CARGO_BIN_EXE_jockyc"))
            .current_dir(if n % 2 == 0 { root } else { &dir.0 })
            .env("JOCKY_FIXTURE_PROVIDER", format!("ignored-{n}"))
            .env("JOCKY_INSTRUCTION_LIMIT", "0")
            .env("TZ", if n % 2 == 0 { "UTC" } else { "Asia/Kolkata" })
            .arg("build-ir")
            .arg(&source)
            .arg("--output")
            .arg(&output)
            .output()
            .unwrap();
        assert!(start.elapsed() < Duration::from_secs(5));
        assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
        assert!(r.stdout.is_empty() && r.stderr.is_empty());
        let bytes = std::fs::read(output).unwrap();
        assert_eq!(expected_ir.get_or_insert_with(|| bytes.clone()), &bytes);
        for (index, platform) in ["ubuntu", "windows"].into_iter().enumerate() {
            let start = Instant::now();
            let r = Command::new(env!("CARGO_BIN_EXE_jockyc"))
                .current_dir(&dir.0)
                .env("JOCKY_FIXTURE_PROVIDER", "unavailable")
                .arg("run-fixture")
                .arg(&source)
                .arg("--fixture")
                .arg(root.join("examples/fixtures/triage").join(platform))
                .output()
                .unwrap();
            assert!(start.elapsed() < Duration::from_secs(5));
            assert!(r.status.success());
            assert!(r.stderr.is_empty());
            assert_eq!(
                reports[index].get_or_insert_with(|| r.stdout.clone()),
                &r.stdout
            );
        }
    }
}
