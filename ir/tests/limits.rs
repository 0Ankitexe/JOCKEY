mod support;
use jocky_ir::{ExecutionLimits as Limits, FixtureFailure, decode_fixture, prepare};
use support::programs::{self, UBUNTU, run};

#[test]
fn public_limit_configuration_cannot_be_zero_over_cap_or_overflow() {
    assert!(Limits::new(1, 1, 1, 1).is_ok());
    assert!(Limits::new(1_000_000, 10_000, 10_000_000, 256).is_ok());
    for (i, c, w, d) in [
        (0, 1, 1, 1),
        (1_000_001, 1, 1, 1),
        (u64::MAX, 1, 1, 1),
        (1, 0, 1, 1),
        (1, 10_001, 1, 1),
        (1, usize::MAX, 1, 1),
        (1, 1, 0, 1),
        (1, 1, 10_000_001, 1),
        (1, 1, u64::MAX, 1),
        (1, 1, 1, 0),
        (1, 1, 1, 257),
        (1, 1, 1, usize::MAX),
    ] {
        assert!(Limits::new(i, c, w, d).is_err());
    }
}

#[test]
fn exact_instruction_and_work_boundaries_stop_before_the_next_charge() {
    let p = programs::triage();
    let exact = run(&p, UBUNTU, Limits::new(10, 10_000, 1629, 1).unwrap());
    assert_eq!(exact["outcome"]["status"], "success");
    assert_eq!(exact["accounting"]["work"], 1629);
    for (instructions, work, code) in [
        (9, 1629, "EXEC_INSTRUCTION_LIMIT"),
        (10, 1628, "EXEC_WORK_LIMIT"),
    ] {
        let r = run(
            &p,
            UBUNTU,
            Limits::new(instructions, 10_000, work, 1).unwrap(),
        );
        assert_eq!(r["outcome"]["error"]["code"], code);
        assert!(r["outcome"].get("value").is_none());
        assert!(r["accounting"]["instructions"].as_u64().unwrap() <= instructions);
        assert!(r["accounting"]["work"].as_u64().unwrap() <= work);
    }
}

#[test]
fn empty_nonempty_and_nested_loops_charge_n_plus_two_globally() {
    for (n, depth) in [(0, 1), (1, 1), (2, 1), (2, 2), (3, 3)] {
        let p = programs::loops(n, depth);
        let loop_cost = (0..depth).fold(0, |cost, _| n + 2 + n * cost);
        let expected = (3 + loop_cost) as u64; // list const, result const, return
        for budget in [expected - 1, expected] {
            let r = run(
                &p,
                UBUNTU,
                Limits::new(budget, 10_000, 1_000_000, 128).unwrap(),
            );
            assert_eq!(r["accounting"]["instructions"], budget);
            assert_eq!(
                r["outcome"]["status"],
                if budget == expected {
                    "success"
                } else {
                    "failure"
                }
            );
            if budget < expected {
                assert_eq!(r["outcome"]["error"]["code"], "EXEC_INSTRUCTION_LIMIT");
            }
        }
    }
}

#[test]
fn frame_and_control_depth_limits_are_explicit_stack_boundaries() {
    for depth in [1, 2, 128, 256] {
        let p = programs::chain(depth);
        let r = run(
            &p,
            UBUNTU,
            Limits::new(1000, 10_000, 1_000_000, depth).unwrap(),
        );
        assert_eq!(r["outcome"]["status"], "success");
        assert_eq!(r["accounting"]["peak_call_depth"], depth);
        assert_eq!(r["accounting"]["instructions"], 2 * depth);
        let r = run(
            &programs::recursive(),
            UBUNTU,
            Limits::new(1000, 10_000, 1_000_000, depth).unwrap(),
        );
        assert_eq!(r["outcome"]["error"]["code"], "EXEC_CALL_DEPTH");
        assert_eq!(r["accounting"]["instructions"], depth);
    }
    // Root region + two active cursors for each nested one-element loop.
    let r = run(&programs::loops(1, 127), UBUNTU, Limits::default());
    assert_eq!(r["outcome"]["status"], "success");
    let r = run(&programs::loops(1, 128), UBUNTU, Limits::default());
    assert_eq!(r["outcome"]["error"]["code"], "EXEC_MEMORY_LIMIT");
}

#[test]
fn stricter_preparation_checks_nested_constants_and_fixture_collections() {
    let limits = Limits::new(100_000, 2, 1_000_000, 128).unwrap();
    let f = decode_fixture(UBUNTU, &Limits::default()).unwrap();
    for count in [2, 3] {
        use jocky_ir::value::Value;
        use jocky_shared::compiler::{NamedType, Type};
        let mut d = support::document();
        let bool_type = Type::named(NamedType::Bool);
        let inner = Value::List {
            element_type: bool_type.clone(),
            values: vec![Value::Bool { value: true }; count],
        };
        d.constants.push(Value::List {
            element_type: Type::List {
                element: Box::new(bool_type),
            },
            values: vec![inner],
        });
        let p = support::verify(d).unwrap();
        assert_eq!(prepare(&p, &f, &limits).is_ok(), count == 2);
    }
    assert!(prepare(&programs::loops(2, 2), &f, &limits).is_ok());
    assert!(matches!(
        prepare(&programs::loops(3, 2), &f, &limits),
        Err(FixtureFailure::Resource)
    ));
    // The fixture contains a two-element record_ids collection nested inside an indicator.
    let limits = Limits::new(100_000, 1, 1_000_000, 128).unwrap();
    assert_eq!(
        decode_fixture(UBUNTU, &limits).unwrap_err(),
        FixtureFailure::Resource
    );
    assert!(matches!(
        prepare(&programs::triage(), &f, &limits),
        Err(FixtureFailure::Resource)
    ));
}

#[test]
fn exact_input_byte_caps_include_whitespace_and_reject_one_more_byte() {
    let mut ir = support::HAND_AUTHORED.to_vec();
    ir.resize(jocky_ir::limits::IR_BYTES, b' ');
    assert!(jocky_ir::decode_ir(&ir).is_ok());
    ir.push(b' ');
    assert_eq!(
        jocky_ir::decode_ir(&ir).unwrap_err().code(),
        jocky_ir::DiagnosticCode::Resource
    );
    let mut f = UBUNTU.to_vec();
    f.resize(jocky_ir::limits::FIXTURE_BYTES, b' ');
    assert!(decode_fixture(&f, &Limits::default()).is_ok());
    f.push(b' ');
    assert_eq!(
        decode_fixture(&f, &Limits::default()).unwrap_err(),
        FixtureFailure::Resource
    );
}
