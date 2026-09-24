use std::collections::HashMap;

use super::{receiver_types_match, resolve_expr, ReceiverTarget};
use crate::terlan_typeck::{CoreExpr, CoreType};

#[test]
fn only_declared_receivers_survive_serialized_dispatch() {
    use crate::terlan_hir::resolve_syntax_module_output;
    use crate::terlan_syntax::parse_module_as_syntax_output;
    use crate::terlan_typeck::{lower_syntax_module_output_to_core, CoreModule};

    let source = "module app.Methods. pub (values: List[T]) each(cb: (T) -> Unit): Unit -> Unit. \
                  pub ordinary(values: List[T], cb: (T) -> Unit): Unit -> Unit.";
    let syntax = parse_module_as_syntax_output(source).expect("parse receiver declarations");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let serialized = serde_json::to_string(&core).expect("serialize receiver declarations");
    let restored: CoreModule = serde_json::from_str(&serialized).expect("restore receivers");
    let targets = super::receiver_targets(&[restored]);
    assert!(targets.contains_key(&("each".into(), 2)));
    assert!(!targets.contains_key(&("ordinary".into(), 2)));
    let mut missing = serde_json::to_value(&core).expect("receiver JSON");
    missing["functions"][0]
        .as_object_mut()
        .unwrap()
        .remove("receiver_method");
    assert!(serde_json::from_value::<CoreModule>(missing).is_err());
}

#[test]
fn generic_receiver_dispatch_preserves_types_visibility_and_ambiguity() {
    let target = ReceiverTarget {
        module: "app.ListMethods".into(),
        function: "each".into(),
        receiver: CoreType::List(Box::new(CoreType::Named("T".into()))),
        public: true,
        generic_params: vec!["T".into()],
    };
    let actual = CoreType::List(Box::new(CoreType::Int));
    assert!(super::receiver_matches(&target, &actual));
    assert!(!super::receiver_matches(&target, &CoreType::Int));
    let mut concrete = target.clone();
    concrete.receiver = CoreType::List(Box::new(CoreType::String));
    assert!(!super::receiver_matches(&concrete, &actual));
    let identity = ("each".into(), 2);
    let mut targets = HashMap::from([(identity.clone(), vec![target.clone()])]);
    assert!(super::receiver_target("each", 2, &actual, "app.Caller", &targets).is_some());
    targets.get_mut(&identity).unwrap()[0].public = false;
    assert!(super::receiver_target("each", 2, &actual, "app.Caller", &targets).is_none());
    assert!(super::receiver_target("each", 2, &actual, "app.ListMethods", &targets).is_some());
    targets.insert(identity, vec![target.clone(), target]);
    assert!(super::receiver_target("each", 2, &actual, "app.Caller", &targets).is_none());
}

#[test]
fn generic_receivers_do_not_merge_distinct_qualified_constructors() {
    let target = ReceiverTarget {
        module: "app.Methods".into(),
        function: "each".into(),
        receiver: CoreType::Apply {
            constructor: "alpha.Box".into(),
            args: vec![CoreType::Named("T".into())],
        },
        public: true,
        generic_params: vec!["T".into()],
    };
    assert!(!super::receiver_matches(
        &target,
        &CoreType::Apply {
            constructor: "beta.Box".into(),
            args: vec![CoreType::Int],
        }
    ));
}

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
        type_args: Vec::new(),
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
                generic_params: Vec::new(),
            },
            ReceiverTarget {
                module: "std.io.Path".to_string(),
                function: "join".to_string(),
                receiver: CoreType::Named("std.io.Path.Path".to_string()),
                public: true,
                generic_params: Vec::new(),
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
