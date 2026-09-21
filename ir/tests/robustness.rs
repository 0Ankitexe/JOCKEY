mod support;
use jocky_ir::{ExecutionLimits, decode_fixture, decode_ir, execute, prepare};
use std::time::{Duration, Instant};
use support::programs::{self, UBUNTU};

fn next(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}
fn check_time(start: Instant, name: &str) {
    assert!(
        start.elapsed() < Duration::from_secs(300),
        "{name} exceeded five-minute harness budget"
    );
}

#[test]
fn ten_thousand_fixed_seed_strict_input_mutations_never_panic() {
    let start = Instant::now();
    let mut seed = 0x4a4f_434b_5903_u64;
    let limits = ExecutionLimits::default();
    for sample in 0..10_000 {
        let mut bytes = if sample % 4 == 0 {
            support::HAND_AUTHORED.to_vec()
        } else {
            vec![0; (next(&mut seed) % 256) as usize]
        };
        for byte in bytes
            .iter_mut()
            .step_by(if sample % 4 == 0 { 101 } else { 1 })
        {
            *byte = next(&mut seed) as u8;
        }
        let _ = decode_ir(&bytes);
        let _ = decode_fixture(&bytes, &limits);
        check_time(start, "decode");
    }
    for depth in [96, 97, 1024, 20_000] {
        let text = format!("{}0{}", "[".repeat(depth), "]".repeat(depth));
        assert!(decode_ir(text.as_bytes()).is_err());
        assert!(decode_fixture(text.as_bytes(), &limits).is_err());
    }
    eprintln!("10,000 strict input mutations: {:?}", start.elapsed());
}

#[test]
fn ten_thousand_direct_structured_verifier_mutations_never_panic() {
    let start = Instant::now();
    let original = support::document();
    let mut seed = 0x4942_0300_u64;
    let mut accepted = 0;
    let mut rejected = 0;
    for sample in 0..10_000 {
        let mut d = original.clone();
        let n = next(&mut seed) as usize;
        match sample % 10 {
            0 => d.entry = n,
            1 => d.functions[0].parameters[0] = n,
            2 => d.functions[0].slots[1].region = n,
            3 => {
                d.functions[0].regions[0].instructions[1].operation =
                    jocky_ir::model::Operation::Return { value: n }
            }
            4 => d.functions[0].regions[0].instructions[0].id = format!("bad-{n}"),
            5 => d.functions[0].regions[0].instructions.clear(),
            6 => d.constants.clear(),
            7 => {
                d.functions[0].result =
                    jocky_shared::compiler::Type::named(jocky_shared::compiler::NamedType::Bool)
            }
            8 => d.source.label = "inert://no-resource".into(),
            _ => {}
        }
        match support::verify(d) {
            Ok(_) => accepted += 1,
            Err(_) => rejected += 1,
        }
        check_time(start, "verify");
    }
    assert_eq!((accepted, rejected), (1000, 9000));
    eprintln!(
        "10,000 structured verifier mutations: {:?}",
        start.elapsed()
    );
}

#[test]
fn ten_thousand_prepared_executions_are_bounded_and_reusable() {
    let start = Instant::now();
    let f = decode_fixture(UBUNTU, &ExecutionLimits::default()).unwrap();
    let programs = [
        support::verify(support::document()).unwrap(),
        programs::recursive(),
        programs::loops(2, 2),
    ];
    let mut seed = 0x0045_5845_4303_u64;
    let mut successes = 0;
    let mut failures = 0;
    for sample in 0..10_000 {
        let limits = ExecutionLimits::new(
            1 + next(&mut seed) % 32,
            10_000,
            1 + next(&mut seed) % 2048,
            1 + (next(&mut seed) % 8) as usize,
        )
        .unwrap();
        let p = &programs[sample % programs.len()];
        let report = execute(prepare(p, &f, &limits).unwrap());
        let bytes = report.to_json().unwrap();
        let raw: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(report.accounting().instructions <= limits.instructions());
        assert!(report.accounting().work <= limits.work());
        assert!(report.accounting().peak_call_depth <= limits.call_depth());
        if report.failure().is_some() {
            failures += 1;
            assert!(raw["outcome"].get("value").is_none());
        } else {
            successes += 1;
        }
        check_time(start, "execution");
    }
    assert!(successes > 0 && failures > 0);
    eprintln!(
        "10,000 prepared executions: {:?}; successes={successes}, failures={failures}",
        start.elapsed()
    );
}
