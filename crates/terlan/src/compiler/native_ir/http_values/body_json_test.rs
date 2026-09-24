//! Only the exact public result may enter the private immediate-match adapter.

use super::*;
use crate::terlan_typeck::CoreTupleTypeElem;

fn result_type(json_identity: &str) -> CoreType {
    CoreType::Union(
        [
            ("ok", "value", json_identity),
            ("error", "reason", "std.core.Error.Error"),
        ]
        .into_iter()
        .map(|(tag, field, identity)| {
            CoreType::Tuple(vec![
                CoreTupleTypeElem::Type(CoreType::AtomLiteral(tag.into())),
                CoreTupleTypeElem::Field {
                    name: field.into(),
                    ty: CoreType::Struct {
                        name: identity.into(),
                        fields: Vec::new(),
                    },
                },
            ])
        })
        .collect(),
    )
}

#[test]
fn transparent_result_cast_is_consumed_only_at_the_http_match() {
    let clauses = [CoreCaseClause {
        pattern: CorePattern::Wildcard,
        guard: None,
        body: CoreExpr::Int(1),
    }];
    for (call, target, accepted) in [
        ("body_json", result_type("std.data.Json.Json"), true),
        ("body_json", result_type("other.Json"), false),
        ("body_json", CoreType::Int, false),
        (
            "another_operation",
            result_type("std.data.Json.Json"),
            false,
        ),
    ] {
        let expression = CoreExpr::Cast {
            expr: Box::new(managed_call(call, vec![CoreExpr::Var("request".into())])),
            target_type: target,
        };
        let lowered = lower_body_json_case(&expression, &clauses).expect("match lowering");
        assert_eq!(lowered.is_some(), accepted);
        if let Some(CoreExpr::Let { bindings, .. }) = lowered {
            assert!(
                matches!(&bindings[0].value, CoreExpr::RemoteCall { function, .. } if function == "body_json")
            );
        }
    }
}

#[test]
fn json_result_adapter_rejects_changed_variant_order_or_fields() {
    let CoreType::Union(mut variants) = result_type("std.data.Json.Json") else {
        unreachable!()
    };
    variants.reverse();
    assert!(!is_public_json_result(&CoreType::Union(variants)));
    let CoreType::Union(mut variants) = result_type("std.data.Json.Json") else {
        unreachable!()
    };
    let CoreType::Tuple(fields) = &mut variants[0] else {
        unreachable!()
    };
    fields.pop();
    assert!(!is_public_json_result(&CoreType::Union(variants)));
}
