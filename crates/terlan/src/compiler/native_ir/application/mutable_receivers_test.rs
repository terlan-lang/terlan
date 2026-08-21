use std::collections::HashMap;

use super::{receiver_types_match, resolve_expr, ReceiverTarget};
use crate::terlan_typeck::{CoreExpr, CoreType};

#[test]
fn opaque_receiver_matches_its_expanded_struct_name() {
    let expected = CoreType::Named("Json".to_string());
    let actual = CoreType::Struct {
        name: "std.data.Json.Json".to_string(),
        fields: Vec::new(),
    };

    assert!(receiver_types_match(&expected, &actual));
}

#[test]
fn distinct_qualified_receiver_names_do_not_match_by_leaf() {
    let expected = CoreType::Named("std.alpha.Value".to_string());
    let actual = CoreType::Struct {
        name: "std.beta.Value".to_string(),
        fields: Vec::new(),
    };

    assert!(!receiver_types_match(&expected, &actual));
}

#[test]
fn unresolved_receiver_call_uses_checked_receiver_type() {
    let mut expression = CoreExpr::RemoteCall {
        module: "__receiver__".to_string(),
        function: "join".to_string(),
        args: vec![
            CoreExpr::Var("parent".to_string()),
            CoreExpr::Var("target".to_string()),
        ],
    };
    let variables = HashMap::from([
        (
            "parent".to_string(),
            CoreType::Named("std.io.Path.Path".to_string()),
        ),
        ("target".to_string(), CoreType::String),
    ]);
    let targets = HashMap::from([(
        ("join".to_string(), 2),
        vec![
            ReceiverTarget {
                module: "std.core.String".to_string(),
                function: "join".to_string(),
                receiver: CoreType::List(Box::new(CoreType::String)),
                public: true,
            },
            ReceiverTarget {
                module: "std.io.Path".to_string(),
                function: "join".to_string(),
                receiver: CoreType::Named("std.io.Path.Path".to_string()),
                public: true,
            },
        ],
    )]);

    resolve_expr(
        &mut expression,
        "app.Support",
        &variables,
        &HashMap::new(),
        &targets,
    )
    .expect("resolve typed receiver call");

    assert!(matches!(
        expression,
        CoreExpr::Call { function, .. } if function == "std.io.Path.join"
    ));
}
