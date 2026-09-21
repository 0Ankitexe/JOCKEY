#![allow(dead_code)]
use jocky_ir::{ExecutionLimits, VerifiedProgram, decode_fixture, decode_ir, execute, prepare};
use serde_json::{Value, json};

pub const UBUNTU: &[u8] = include_bytes!("../fixtures/valid/ubuntu/fixture.json");
pub const WINDOWS: &[u8] = include_bytes!("../fixtures/valid/windows/fixture.json");
pub fn triage() -> VerifiedProgram {
    super::verify(
        decode_ir(include_bytes!(
            "../../../language/tests/fixtures/ir/golden/triage.ir.json"
        ))
        .unwrap(),
    )
    .unwrap()
}
pub fn from_raw(raw: Value) -> VerifiedProgram {
    let mut d = serde_json::from_value(raw).unwrap();
    super::reidentify(&mut d);
    super::verify(d).unwrap()
}
pub fn run(p: &VerifiedProgram, bytes: &[u8], limits: ExecutionLimits) -> Value {
    let f = decode_fixture(bytes, &limits).unwrap();
    serde_json::from_slice(&execute(prepare(p, &f, &limits).unwrap()).to_json().unwrap()).unwrap()
}
pub fn recursive() -> VerifiedProgram {
    let mut d = super::document();
    d.functions[0].regions[0].instructions[0].operation = jocky_ir::model::Operation::Call {
        destination: 1,
        callee: jocky_ir::model::Callee::Source { function: 0 },
        arguments: vec![0],
        on_error: jocky_ir::model::ErrorPolicy::Propagate,
    };
    super::verify(d).unwrap()
}
pub fn chain(depth: usize) -> VerifiedProgram {
    let mut d = super::document();
    let original = d.functions[0].clone();
    d.functions = (0..depth)
        .map(|i| {
            let mut f = original.clone();
            f.name = format!("f{i}");
            if i + 1 < depth {
                f.regions[0].instructions[0].operation = jocky_ir::model::Operation::Call {
                    destination: 1,
                    callee: jocky_ir::model::Callee::Source { function: i + 1 },
                    arguments: vec![0],
                    on_error: jocky_ir::model::ErrorPolicy::Propagate,
                };
            }
            f
        })
        .collect();
    super::reidentify(&mut d);
    super::verify(d).unwrap()
}
pub fn loops(length: usize, depth: usize) -> VerifiedProgram {
    assert!(depth > 0);
    let mut d = super::raw();
    let ty = json!({"kind":"named","name":"bool"});
    d["constants"].as_array_mut().unwrap().push(json!({"kind":"list","element_type":ty,"values":vec![json!({"kind":"bool","value":true});length]}));
    let f = &mut d["functions"][0];
    f["slots"].as_array_mut().unwrap().push(json!({"type":{"kind":"list","element":ty},"region":0,"kind":"temporary","name":null,"span":null}));
    for level in 0..depth {
        f["slots"].as_array_mut().unwrap().push(json!({"type":ty,"region":level+1,"kind":"loop_binding","name":format!("item{level}"),"span":null}));
        f["regions"]
            .as_array_mut()
            .unwrap()
            .push(json!({"instructions":[]}));
        let op = json!({"id":"","span":null,"op":"for_each","collection":2,"item":3+level,"body_region":level+1});
        f["regions"][level]["instructions"]
            .as_array_mut()
            .unwrap()
            .insert(0, op);
    }
    f["regions"][0]["instructions"]
        .as_array_mut()
        .unwrap()
        .insert(
            0,
            json!({"id":"","span":null,"op":"const","destination":2,"constant":1}),
        );
    from_raw(d)
}
