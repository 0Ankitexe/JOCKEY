use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::{Duration, Instant};

use jocky_language::{DiagnosticCategory, SourceFile, parse};

#[test]
fn fixed_seed_ten_thousand_input_campaign_never_panics_or_stalls() {
    let started = Instant::now();
    let mut state = 0x4a4f_434b_595f_5631_u64;
    let alphabet = [
        "a", "z", "0", "9", "_", " ", "\t", "\n", "\r\n", "{", "}", "(", ")", "<", ">", "\"", "\\",
        "/", "*", "|", ",", ":", "=", ".", "-", "é", "中", "💾", "\u{0001}",
    ];
    let mut slowest = Duration::ZERO;

    for case in 0..10_000 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let length = ((state >> 32) as usize % 48) + (case % 3);
        let mut source = String::new();
        for _ in 0..length {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            source.push_str(alphabet[(state as usize) % alphabet.len()]);
        }
        let case_started = Instant::now();
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            parse(SourceFile::new(
                "does-not-exist-and-must-not-be-opened.jky",
                &source,
            ))
        }));
        let elapsed = case_started.elapsed();
        slowest = slowest.max(elapsed);
        assert!(outcome.is_ok(), "case {case} panicked: {source:?}");
        assert!(
            elapsed < Duration::from_secs(1),
            "case {case} took {elapsed:?}"
        );
    }

    let total = started.elapsed();
    eprintln!("robustness cases=10000 total={total:?} slowest={slowest:?}");
    assert!(total < Duration::from_secs(300));
}

#[test]
fn long_tokens_and_delimiter_guard_are_bounded() {
    let huge_identifier = "a".repeat(1_000_000);
    let source = format!("module {huge_identifier}\ntarget windows\nfn f() -> r {{}}\n");
    let started = Instant::now();
    assert!(parse(SourceFile::new("long.jky", &source)).is_ok());
    assert!(started.elapsed() < Duration::from_secs(1));

    let exact = format!(
        "module m\ntarget windows\nfn f(value: {}item{}) -> r {{ return r() }}",
        "list<".repeat(255),
        ">".repeat(255),
    );
    let exact_result = parse(SourceFile::new("exact-depth.jky", &exact));
    assert!(exact_result.is_ok(), "{exact_result:?}");

    let over = "(".repeat(257);
    let diagnostics = parse(SourceFile::new("over-depth.jky", &over)).unwrap_err();
    assert_eq!(
        diagnostics.items()[0].category,
        DiagnosticCategory::ResourceLimit
    );
}
