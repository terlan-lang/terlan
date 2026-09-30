use super::*;

fn field(key: &str, value: CoreExpr) -> CoreRecordExprField {
    CoreRecordExprField {
        key: key.into(),
        required: true,
        value,
    }
}

#[test]
fn record_inference_matches_fields_by_name_and_rejects_inconsistent_evidence() {
    let core = crate::compiler::native_ir::source_constructor_test::checked_provider(
        "module app.Records. pub struct Pair[A, B] { first: A, second: B }. \
         pub struct Same[T] { first: T, second: T }.",
    );
    let mut templates = CallableTemplates::default();
    collect(&[core], &mut templates);
    let variables = HashMap::new();
    let infer = |name, fields: &[CoreRecordExprField]| {
        infer_record(name, fields, &variables, &templates, "app.Records")
    };
    let first = field("first", CoreExpr::Int(42));
    let second = field("second", CoreExpr::Atom("true".into()));
    assert_eq!(
        infer("app.Records.Pair", &[second.clone(), first.clone()]),
        Some(CoreType::Apply {
            constructor: "app.Records.Pair".into(),
            args: vec![CoreType::Int, CoreType::Bool],
        })
    );
    assert_eq!(
        infer("app.Records.Same", &[first.clone(), second.clone()]),
        None
    );
    assert_eq!(
        infer("app.Records.Pair", &[first.clone(), first.clone()]),
        None
    );
    assert_eq!(
        infer(
            "app.Records.Pair",
            &[field("unknown", CoreExpr::Int(1)), second.clone()]
        ),
        None
    );
    assert_eq!(
        infer(
            "app.Records.Pair",
            &[
                field("first", CoreExpr::Var("missing".into())),
                second.clone()
            ]
        ),
        None
    );
    assert_eq!(
        infer("elsewhere.Pair", &[first, second]),
        Some(CoreType::Named("elsewhere.Pair".into()))
    );
}
