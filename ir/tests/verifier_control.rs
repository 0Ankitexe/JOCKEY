mod support;
use jocky_ir::{DiagnosticCode as Code, IrDocument, model::*, value::Value};
use jocky_shared::compiler::{NamedType, Type};

fn ins(operation: Operation) -> Instruction {
    Instruction {
        id: String::new(),
        span: None,
        operation,
    }
}
fn slot(ty: Type, region: usize, kind: SlotKind) -> Slot {
    Slot {
        r#type: ty,
        region,
        kind,
        name: None,
        span: None,
    }
}
fn branch() -> IrDocument {
    let mut d = support::document();
    d.constants.push(Value::Bool { value: true });
    d.functions[0]
        .slots
        .push(slot(Type::named(NamedType::Bool), 0, SlotKind::Temporary));
    d.functions[0].regions[0].instructions = vec![
        ins(Operation::Const {
            destination: 1,
            constant: 0,
        }),
        ins(Operation::Const {
            destination: 2,
            constant: 1,
        }),
        ins(Operation::If {
            condition: 2,
            then_region: 1,
            else_region: Some(2),
        }),
    ];
    for _ in 0..2 {
        d.functions[0].regions.push(Region {
            instructions: vec![ins(Operation::Return { value: 1 })],
        });
    }
    support::reidentify(&mut d);
    d
}
fn looping() -> IrDocument {
    let mut d = support::document();
    d.constants.push(Value::List {
        element_type: Type::named(NamedType::Bool),
        values: vec![],
    });
    d.functions[0].slots.extend([
        slot(
            Type::List {
                element: Box::new(Type::named(NamedType::Bool)),
            },
            0,
            SlotKind::Temporary,
        ),
        slot(Type::named(NamedType::Bool), 1, SlotKind::LoopBinding),
        slot(Type::named(NamedType::Bool), 1, SlotKind::Local),
    ]);
    d.functions[0].regions[0].instructions = vec![
        ins(Operation::Const {
            destination: 1,
            constant: 0,
        }),
        ins(Operation::Const {
            destination: 2,
            constant: 1,
        }),
        ins(Operation::ForEach {
            collection: 2,
            item: 3,
            body_region: 1,
        }),
        ins(Operation::Return { value: 1 }),
    ];
    d.functions[0].regions.push(Region {
        instructions: vec![
            ins(Operation::Copy {
                destination: 4,
                source: 3,
            }),
            ins(Operation::Return { value: 1 }),
        ],
    });
    support::reidentify(&mut d);
    d
}
fn rejects(mut d: IrDocument, code: Code) {
    support::reidentify(&mut d);
    let e = support::verify(d).unwrap_err();
    assert!(e.diagnostics().iter().any(|d| d.code == code), "{e:?}");
}

#[test]
fn both_arms_return_but_missing_arm_or_empty_loop_can_fall_through() {
    support::verify(branch()).unwrap();
    let mut d = branch();
    d.functions[0].regions[2].instructions.clear();
    rejects(d, Code::Return);
    support::verify(looping()).unwrap();
    let mut d = looping();
    d.functions[0].regions[0].instructions.pop();
    rejects(d, Code::Return);
}

#[test]
fn region_ownership_visibility_boolean_and_iteration_types_are_exact() {
    let mut d = branch();
    d.functions[0].regions[0].instructions[2].operation = Operation::If {
        condition: 1,
        then_region: 1,
        else_region: Some(2),
    };
    rejects(d, Code::Type);
    let mut d = branch();
    d.functions[0].regions[0].instructions[2].operation = Operation::If {
        condition: 2,
        then_region: 1,
        else_region: Some(1),
    };
    rejects(d, Code::Control);
    let mut d = branch();
    d.functions[0].regions[1]
        .instructions
        .push(ins(Operation::If {
            condition: 2,
            then_region: 0,
            else_region: None,
        }));
    rejects(d, Code::Control);
    let mut d = looping();
    d.functions[0].slots[3].r#type = Type::named(NamedType::Int);
    rejects(d, Code::Type);
    let mut d = looping();
    d.functions[0].slots[3].region = 0;
    rejects(d, Code::Initialization);
    let mut d = looping();
    d.functions[0].regions[1].instructions[0].operation = Operation::Copy {
        destination: 4,
        source: 4,
    };
    rejects(d, Code::Initialization);
    let mut d = looping();
    d.functions[0].regions[0].instructions[3].operation = Operation::Return { value: 4 };
    rejects(d, Code::Reference);
    let mut d = looping();
    d.functions[0].regions[1].instructions[0].operation = Operation::Copy {
        destination: 2,
        source: 3,
    };
    rejects(d, Code::Reference);
}

#[test]
fn dead_reads_skip_initialization_not_static_types_or_references() {
    let mut d = support::document();
    d.functions[0].slots.push(slot(
        Type::named(NamedType::ForensicResult),
        0,
        SlotKind::Temporary,
    ));
    d.functions[0].regions[0]
        .instructions
        .push(ins(Operation::Copy {
            destination: 2,
            source: 2,
        }));
    support::reidentify(&mut d);
    support::verify(d.clone()).unwrap();
    d.functions[0].regions[0].instructions[2].operation = Operation::Copy {
        destination: 2,
        source: 0,
    };
    rejects(d, Code::Type);
    let mut d = branch();
    d.functions[0].regions[0].instructions.swap(0, 2);
    rejects(d, Code::Initialization);
}

#[test]
fn recursive_source_calls_and_composition_have_exact_signatures() {
    let mut d = support::document();
    d.functions[0].regions[0].instructions[0].operation = Operation::Call {
        destination: 1,
        callee: Callee::Source { function: 0 },
        arguments: vec![0],
        on_error: ErrorPolicy::Propagate,
    };
    support::verify(d.clone()).unwrap();
    d.functions[0].regions[0].instructions[0].operation = Operation::Call {
        destination: 1,
        callee: Callee::Source { function: 0 },
        arguments: vec![],
        on_error: ErrorPolicy::Propagate,
    };
    rejects(d, Code::Type);
    let mut d = support::document();
    d.functions[0].slots.push(slot(
        Type::named(NamedType::ForensicResult),
        0,
        SlotKind::Temporary,
    ));
    d.functions[0].regions[0].instructions.insert(
        1,
        ins(Operation::Call {
            destination: 2,
            callee: Callee::Composition,
            arguments: vec![1, 1],
            on_error: ErrorPolicy::Propagate,
        }),
    );
    d.functions[0].metadata.unavailable_dependencies = vec!["forensic_result".into()];
    d.entry_metadata = d.functions[0].metadata.clone();
    support::reidentify(&mut d);
    support::verify(d.clone()).unwrap();
    if let Operation::Call { arguments, .. } =
        &mut d.functions[0].regions[0].instructions[1].operation
    {
        arguments[1] = 0;
    }
    rejects(d, Code::Type);
}
