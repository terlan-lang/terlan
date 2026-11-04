use super::*;
use crate::terlan_typeck::CoreTupleTypeElem;

fn tagged(tag: &str, payload: CoreType) -> CoreType {
    CoreType::Tuple(vec![
        CoreTupleTypeElem::Type(CoreType::AtomLiteral(tag.to_string())),
        CoreTupleTypeElem::Type(payload),
    ])
}

#[test]
fn concrete_context_replaces_descriptive_phantom_type_through_tuple_cast() {
    let core = crate::compiler::native_ir::source_constructor_test::checked_provider(
        r#"
module sample.Phantom.
pub make[Reason](code: Int): {Atom["specific"], Reason} | {Atom["general"], Int} ->
    {Atom["general"], code}.
"#,
    );
    let template = core
        .functions
        .iter()
        .find(|function| function.name == "make")
        .expect("generic phantom producer")
        .clone();
    let mut templates = CallableTemplates::new(std::slice::from_ref(&core));
    templates
        .functions
        .insert(("sample.Phantom.make".to_string(), 1), vec![template]);
    let symbolic_failure = CoreType::Union(vec![
        tagged("specific", CoreType::Named("Reason".to_string())),
        tagged("general", CoreType::Int),
    ]);
    let concrete_failure = CoreType::Union(vec![
        tagged("specific", CoreType::String),
        tagged("general", CoreType::Int),
    ]);
    let symbolic_result = CoreType::Union(vec![tagged("error", symbolic_failure)]);
    let concrete_result = CoreType::Union(vec![tagged("error", concrete_failure)]);
    let mut value = CoreExpr::Cast {
        expr: Box::new(CoreExpr::Tuple(vec![
            CoreExpr::Atom("error".to_string()),
            CoreExpr::Call {
                function: "sample.Phantom.make".to_string(),
                type_args: vec![CoreType::Named("Reason".to_string())],
                args: vec![CoreExpr::Int(42)],
            },
        ])),
        target_type: symbolic_result,
    };

    seed(
        &mut value,
        &concrete_result,
        &HashMap::new(),
        &templates,
        "sample.Consumer",
    );

    let CoreExpr::Cast { expr, target_type } = value else {
        panic!("expected retained result cast")
    };
    assert_eq!(target_type, concrete_result);
    let CoreExpr::Tuple(items) = *expr else {
        panic!("expected tagged result tuple")
    };
    let CoreExpr::Call { type_args, .. } = &items[1] else {
        panic!("expected nested generic producer")
    };
    assert_eq!(type_args, &vec![CoreType::String]);
}
