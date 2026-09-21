use jocky_language::{SourceFile, semantic::analyze};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    time::{Duration, Instant},
};

#[test]
fn ten_thousand_arbitrary_and_structured_checks_are_bounded_and_never_panic() {
    let registry = jocky_forensic::contracts::builtin_registry().unwrap();
    let start = Instant::now();
    let mut slowest = Duration::ZERO;
    let mut state = 0x4a4f_434b_595f_5632_u64;
    let alphabet = [
        "a", "0", "_", "\t", "\r\n", "{", "}", "(", ")", "<", ">", "\"", "\\", "/", "*", "|", ",",
        ":", "=", ".", "é", "中", "💾", "\0",
    ];
    let mut check = |text: &str| {
        let began = Instant::now();
        let result = catch_unwind(AssertUnwindSafe(|| {
            analyze(SourceFile::new("never-open-this-label", text), registry)
        }));
        let elapsed = began.elapsed();
        slowest = slowest.max(elapsed);
        assert!(result.is_ok(), "panicked for {text:?}");
        assert!(elapsed < Duration::from_secs(1), "case took {elapsed:?}");
    };
    for case in 0..10_000 {
        let mut text = String::new();
        for _ in 0..case % 97 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            text.push_str(alphabet[state as usize % alphabet.len()]);
        }
        check(&text);
    }
    for i in 0..256 {
        let body = match i % 4 {
            0 => "let unused = 1 return forensic.system.profile(target)".to_owned(),
            1 => format!(
                "if {} {{ return forensic.system.profile(target) }} return absent",
                i
            ),
            2 => "return main(target)".into(),
            _ => "return forensic.system.profile(target) let unused = false".into(),
        };
        let text = format!(
            "module m target ubuntu fn main(target: endpoint) -> forensic_result {{ {body} }} run main on selected_endpoints"
        );
        assert!(jocky_language::parse_source(SourceFile::new("memory", &text)).is_ok());
        check(&text);
    }
    for text in [
        "(".repeat(257),
        format!(
            "module {} target ubuntu fn main(target: endpoint) -> forensic_result {{ return main(target) }} run main on selected_endpoints",
            "a".repeat(500_000)
        ),
        format!(
            "module m target ubuntu fn f(x: {}int{}) -> int {{ return 1 }}",
            "list<".repeat(100),
            ">".repeat(100)
        ),
    ] {
        check(&text);
    }
    let elapsed = start.elapsed();
    eprintln!(
        "semantic campaign arbitrary=10000 structured=256 adversarial=3 total={elapsed:?} slowest={slowest:?}"
    );
    assert!(elapsed < Duration::from_secs(300));
}
