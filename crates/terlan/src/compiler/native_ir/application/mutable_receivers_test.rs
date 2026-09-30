use std::collections::HashMap;

use super::{receiver_types_match, resolve_expr, ReceiverTarget};
use crate::terlan_typeck::{CoreExpr, CoreType};

#[path = "receiver_defaults_test.rs"]
mod receiver_defaults_test;

#[test]
fn local_record_command_results_are_unit_and_preserve_snapshots() {
    crate::compiler::native_ir::source_constructor_test::check_sources(&[r#"
module local_record_command.
import std.core.Unit.
pub struct Counter { count: Int }.
pub (mut counter: Counter) add(value: Int): Unit -> Counter { count: counter.count + value }.
pub check(): Bool ->
    let counter = Counter { count: 3 };
    let snapshot = counter;
    let done = counter.add(4);
    done == Unit and counter.count == 7 and snapshot.count == 3.
"#]);
}

#[test]
fn trait_receivers_survive_pruning_and_execute_the_typed_implementation() {
    use crate::compiler::native_ir::{self, native_object_test_support::*};
    use crate::terlan_syntax::parse_module_as_syntax_output;
    use crate::terlan_typeck::{
        lower_syntax_module_output_to_core, type_check_syntax_module_output,
    };

    let syntax = parse_module_as_syntax_output(
        r#"
module app.TraitReceivers.
import std.core.String.
import std.core.String.{length as count}.
pub trait Score[T] { score(value: T): Int. }.
pub impl Score[Int] for Int { score(value: Int): Int -> value + String.length("abc") + count("four"). }.
pub impl Score[Bool] for Bool { score(_value: Bool): Int -> 11. }.
pub integer(value: Int): Int -> value.score().
pub boolean(value: Bool): Int -> value.score().
pub boolean_literal(): Int -> let value = true; value.score().
"#,
    )
    .expect("parse trait receiver program");
    let interfaces = crate::terlan_hir::checked_in_std_interfaces_for_module(&syntax);
    let resolved =
        crate::terlan_hir::resolve_syntax_module_output_with_interfaces(&syntax, &interfaces)
            .module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let serialized = serde_json::to_string(&core).expect("serialize trait receiver identities");
    let mut cores = vec![serde_json::from_str(&serialized).expect("restore trait receivers")];
    let roots = [("integer", 1), ("boolean", 1), ("boolean_literal", 0)]
        .map(|(name, arity)| ("app.TraitReceivers".into(), name.into(), arity));
    native_ir::prune_application_to_function_roots(&mut cores, &roots)
        .expect("retain receiver-only trait implementations");
    assert_eq!(
        cores[0]
            .functions
            .iter()
            .filter(|f| f.trait_method.is_some())
            .count(),
        2
    );
    let modules = native_ir::NativeModule::lower_application(&cores.iter().collect::<Vec<_>>())
        .unwrap_or_else(|error| panic!("{error}\n{}", cores[0].contract_text()));
    let object = native_ir::emit_native_application_object("trait-receivers", &modules)
        .expect("emit receiver-only trait program");
    let calls = [
        ("integer", vec![35], 42),
        ("boolean", vec![1], 11),
        ("boolean_literal", vec![], 11),
    ]
    .map(|(name, arguments, expected)| {
        let function = modules
            .iter()
            .flat_map(|module| &module.functions)
            .find(|function| function.name == name)
            .expect("receiver export");
        NativeObjectInvocation {
            export_id: function.export_id,
            arguments,
            expected_status: native_ir::status::OK,
            expected_result: Some(expected),
        }
    });
    assert_managed_native_object_invocations("trait-receivers", &modules, &object, &calls);
}

#[test]
fn declared_receiver_precedes_trait_fallback_without_ignoring_ambiguity() {
    let declared = ReceiverTarget {
        module: "app.Methods".into(),
        function: "score".into(),
        receiver: CoreType::Int,
        public: true,
        generic_params: vec![],
        trait_fallback: false,
        mutable: false,
        command: false,
    };
    let mut fallback = declared.clone();
    fallback.function = "trait_score".into();
    fallback.trait_fallback = true;
    let key = ("score".into(), 1);
    let mut targets = HashMap::from([(key.clone(), vec![fallback.clone(), declared.clone()])]);
    assert_eq!(
        super::receiver_target("score", 1, &CoreType::Int, "app.Caller", &targets)
            .unwrap()
            .function,
        "score"
    );
    targets.get_mut(&key).unwrap().push(declared);
    assert!(super::receiver_target("score", 1, &CoreType::Int, "app.Caller", &targets).is_none());
    targets.insert(key.clone(), vec![fallback.clone(), fallback.clone()]);
    assert!(super::receiver_target("score", 1, &CoreType::Int, "app.Caller", &targets).is_none());
    fallback.public = false;
    targets.insert(key, vec![fallback]);
    assert!(super::receiver_target("score", 1, &CoreType::Int, "app.Caller", &targets).is_none());
    assert!(super::receiver_target("score", 1, &CoreType::Int, "app.Methods", &targets).is_some());
}

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
        trait_fallback: false,
        mutable: false,
        command: false,
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
    assert_eq!(
        super::receiver_callable(&targets, "each", 2, &actual, "app.Caller"),
        Some("app.ListMethods.each".into()),
    );
    targets.get_mut(&identity).unwrap()[0].public = false;
    assert!(super::receiver_target("each", 2, &actual, "app.Caller", &targets).is_none());
    assert!(super::receiver_callable(&targets, "each", 2, &actual, "app.Caller").is_none());
    assert!(super::receiver_target("each", 2, &actual, "app.ListMethods", &targets).is_some());
    targets.insert(identity, vec![target.clone(), target]);
    assert!(super::receiver_target("each", 2, &actual, "app.Caller", &targets).is_none());
    assert!(super::receiver_callable(&targets, "each", 2, &actual, "app.Caller").is_none());
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
        trait_fallback: false,
        mutable: false,
        command: false,
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
                trait_fallback: false,
                mutable: false,
                command: false,
                generic_params: Vec::new(),
            },
            ReceiverTarget {
                module: "std.io.Path".to_string(),
                function: "join".to_string(),
                receiver: CoreType::Named("std.io.Path.Path".to_string()),
                public: true,
                trait_fallback: false,
                mutable: false,
                command: false,
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
