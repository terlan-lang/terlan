use super::*;
use crate::{
    terlan_hir::resolve_syntax_module_output, terlan_syntax::parse_module_as_syntax_output,
    terlan_typeck::lower_syntax_module_output_to_core,
};

/// Imported free-function receiver syntax keeps its provider through generic lowering.
#[test]
fn imported_builder_receiver_calls_keep_their_provider() {
    super::super::super::source_constructor_test::check_sources(&[
        r#"
module builders.Caller.
import builders.Library.
pub check(): Bool ->
    let command = if {
        true -> Library.command(20).with_arguments(22);
        false -> Library.command(1).with_arguments(2)
    };
    Library.value(command) == 42.
"#,
        r#"
module builders.Library.
pub struct Command { value: Int }.
pub command(value: Int): Command -> Command { value: value }.
pub with_arguments(command: Command, value: Int): Command ->
    Command { value: command.value + value }.
pub value(command: Command): Int -> command.value.
"#,
    ]);
}

/// Receiver qualification precedes renaming same-arity free-function overloads.
#[test]
fn method_and_free_function_overloads_keep_their_distinct_receiver_targets() {
    super::super::super::source_constructor_test::check_sources(&[
        r#"
module methods.Caller.
import methods.Library.
pub check(): Bool ->
    let adapter = Library.adapter();
    let policy = Library.policy();
    adapter.available()
        and not Library.available(policy)
        and not methods.Library.available(policy).
"#,
        r#"
module methods.Library.
pub struct Policy { enabled: Bool }.
pub struct Adapter { enabled: Bool }.
pub adapter(): Adapter -> Adapter { enabled: true }.
pub policy(): Policy -> Policy { enabled: false }.
pub available(policy: Policy): Bool -> policy.enabled.
pub (adapter: Adapter) available(): Bool -> adapter.enabled.
"#,
    ]);
}

/// Explicit import families keep the ABI selected by argument types, including
/// lexical bindings and typed parameters, rather than failing as module imports.
#[test]
fn selected_import_overloads_execute_with_their_concrete_provider() {
    super::super::super::source_constructor_test::check_sources(&[
        r#"
module imports.Caller.
import imports.Integer.{choose}.
import imports.Floating.{choose}.
integer(value: Int): Int -> let bound = value; choose(bound).
floating(value: Float): Float -> choose(value).
pub check(): Bool -> integer(41) == 42 and floating(40.0) == 42.0.
"#,
        "module imports.Integer. pub choose(value: Int): Int -> value + 1.",
        "module imports.Floating. pub choose(value: Float): Float -> value + 2.0.",
    ]);
}

/// Import aliases retain the different public names of their checked providers.
#[test]
fn selected_import_aliases_execute_without_losing_provider_names() {
    super::super::super::source_constructor_test::check_sources(&[
        r#"
module imports.Aliases.
import imports.Integer.{integer as choose}.
import imports.Floating.{floating as choose}.
pub check(): Bool -> choose(41) == 42 and choose(40.0) == 42.0.
"#,
        "module imports.Integer. pub integer(value: Int): Int -> value + 1.",
        "module imports.Floating. pub floating(value: Float): Float -> value + 2.0.",
    ]);
}

/// Compiler-cache serialization preserves selected names and aliases exactly.
#[test]
fn selected_import_provenance_survives_core_serialization() {
    let caller = core(
        "module imports.Caller. import imports.Integer.{integer as choose}. \
         import imports.Floating.{floating as choose}. pub check(): Int -> choose(1).",
    );
    assert_eq!(caller.selected_function_imports.len(), 2);
    let encoded = serde_json::to_vec(&caller).expect("serialize selected import provenance");
    let restored: CoreModule = serde_json::from_slice(&encoded).expect("restore selected imports");
    assert_eq!(
        restored.selected_function_imports,
        caller.selected_function_imports
    );
    assert_eq!(restored.contract_text(), caller.contract_text());
}

/// Equivalent selected signatures remain an error, independent of source order.
#[test]
fn selected_import_overloads_reject_indistinguishable_providers() {
    let left = core("module imports.Left. pub choose(value: Int): Int -> value.");
    let right = core("module imports.Right. pub choose(value: Int): Int -> value.");
    let caller = core(
        "module imports.Caller. import imports.Left.{choose}. \
         import imports.Right.{choose}. pub check(): Int -> choose(1).",
    );
    for mut modules in [
        vec![caller.clone(), left.clone(), right.clone()],
        vec![right, left, caller],
    ] {
        let error = resolve_selected_imports(&mut modules).expect_err("ambiguous import");
        assert!(error.contains("native_ir.overload_ambiguous"), "{error}");
    }
}

/// Intrinsic-only providers retain checked signatures after body pruning.
#[test]
fn selected_imports_admit_primitive_and_source_backed_candidates_together() {
    let mut boolean =
        core("module std.core.Bool. pub equal(left: Bool, right: Bool): Bool -> left == right.");
    boolean.functions.clear();
    let integer =
        core("module std.core.Int. pub equal(left: Int, right: Int): Bool -> left == right.");
    let caller = core(
        "module imports.Caller. import std.core.Bool.{equal}. \
         import std.core.Int.{equal}. pub check(): Bool -> equal(true, true).",
    );
    let mut modules = vec![caller, boolean, integer];
    resolve_selected_imports(&mut modules).expect("resolve selected intrinsic signature");
    assert!(matches!(
        modules[0].functions[0].clauses[0].body.core_expr.as_ref(),
        Some(CoreExpr::Intrinsic(call))
            if call.id == crate::terlan_typeck::CoreIntrinsicId::Primitive(
                crate::terlan_typeck::CorePrimitiveIntrinsic::BoolEqual
            )
    ));
}

/// Lowers one source fixture into CoreIR without entering NativeIR.
fn core(source: &str) -> CoreModule {
    let syntax = parse_module_as_syntax_output(source).expect("parse overload fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    lower_syntax_module_output_to_core(&syntax, &resolved)
}

/// Returns the direct call target retained by one zero-arity function.
fn direct_call_target<'a>(core: &'a CoreModule, function_name: &str) -> &'a str {
    let expression = core
        .functions
        .iter()
        .find(|function| function.name == function_name)
        .and_then(|function| function.clauses.first())
        .and_then(|clause| clause.body.core_expr.as_ref())
        .expect("typed direct-call body");
    match expression {
        CoreExpr::Call { function, .. } => function,
        other => panic!("expected direct call, found {other:?}"),
    }
}

/// Type-distinct same-arity native overloads retain their selected ABI.
#[test]
fn typed_native_overloads_receive_distinct_internal_call_identities() {
    let mut modules = vec![core(
        "module app.Overload.\n\n\
         @compiler.native {fixture.int}\n\
         choose(_value: Int): Int -> native.\n\n\
         @compiler.native {fixture.list}\n\
         choose(_value: List[Int]): Int -> native.\n\n\
         pub scalar(): Int -> choose(1).\n\n\
         pub collection(): Int -> choose([1, 2]).\n",
    )];

    assert_eq!(
        modules[0]
            .functions
            .iter()
            .filter(|function| function.name == "choose")
            .count(),
        2
    );
    resolve_typed_overloads(&mut modules).expect("resolve typed overloads");

    let integer = modules[0]
        .functions
        .iter()
        .find(|function| function.native_operation.as_deref() == Some("fixture.int"))
        .expect("integer native overload");
    let list = modules[0]
        .functions
        .iter()
        .find(|function| function.native_operation.as_deref() == Some("fixture.list"))
        .expect("list native overload");
    assert_ne!(integer.name, list.name);
    assert_eq!(direct_call_target(&modules[0], "scalar"), integer.name);
    assert_eq!(direct_call_target(&modules[0], "collection"), list.name);
}

/// Alias, list, and Bool literals select a complete same-arity overload.
#[test]
fn typed_overloads_infer_alias_list_and_bool_literal_arguments() {
    let mut modules = vec![core(
        "module app.StructuralOverload.\n\n\
         /** Structural alias used to select the integer overload. */\n\
         pub type Mode: Int = DEFAULT = 0 | OTHER = 1.\n\n\
         choose(_value: Int, _mode: Mode, _axes: List[Int], _keep: Bool): Int -> 1.\n\n\
         choose(_value: Int, _order: Float, _axes: List[Int], _keep: Bool): Int -> 2.\n\n\
         pub selected(): Int -> choose(1, Mode.DEFAULT, [0, 1], true).\n",
    )];

    resolve_typed_overloads(&mut modules).expect("resolve structural overload literals");
    let selected = direct_call_target(&modules[0], "selected");
    let alias = modules[0]
        .functions
        .iter()
        .find(|function| {
            function
                .params
                .get(1)
                .and_then(|parameter| parameter.core_ty.as_ref())
                == Some(&CoreType::Named("Mode".to_string()))
        })
        .expect("alias overload");
    assert_eq!(selected, alias.name);
}

/// Duplicate typed declarations remain for application admission.
#[test]
fn duplicate_core_signature_is_left_for_application_admission() {
    let mut module = core(
        "module app.DuplicateOverload.\n\n\
         @compiler.native {fixture.int}\n\
         choose(_value: Int): Int -> native.\n",
    );
    module.functions.push(module.functions[0].clone());

    resolve_typed_overloads(std::slice::from_mut(&mut module))
        .expect("duplicate declarations are not typed overloads");
    assert_eq!(module.functions[0].name, "choose");
    assert_eq!(module.functions[1].name, "choose");
}

/// Concrete implementations select by checked parameters, not declaration order.
#[test]
fn concrete_trait_calls_select_distinct_typed_bodies() {
    let mut modules = vec![core(
        r#"
module app.TraitChoice.
pub trait Score[T] { score(value: T): Int. }.
pub impl Score[Bool] for Bool { score(_value: Bool): Int -> 11. }.
pub impl Score[Int] for Int { score(value: Int): Int -> value + 7. }.
pub integer(value: Int): Int -> Score.score(value).
pub boolean(value: Bool): Int -> Score.score(value).
"#,
    )];
    let serialized = serde_json::to_string(&modules[0]).expect("serialize trait metadata");
    modules[0] = serde_json::from_str(&serialized).expect("restore trait metadata");
    let original = modules.clone();
    assert!(modules[0]
        .contract_text()
        .contains("trait_method=app.TraitChoice.Score.score"));
    resolve_typed_overloads(&mut modules).expect("select concrete trait bodies");
    for (name, ty) in [("integer", CoreType::Int), ("boolean", CoreType::Bool)] {
        let target = direct_call_target(&modules[0], name);
        let implementation = modules[0]
            .functions
            .iter()
            .find(|function| function.name == target)
            .expect("selected implementation exists");
        assert_eq!(implementation.params[0].core_ty.as_ref(), Some(&ty));
    }
    let native = NativeModule::lower_application(&original.iter().collect::<Vec<_>>())
        .expect("lower typed trait dispatch through the complete application pipeline");
    let object =
        crate::compiler::native_ir::emit_native_application_object("trait_choice", &native)
            .expect("emit trait dispatch object");
    use crate::compiler::native_ir::native_object_test_support::{
        assert_managed_native_object_invocations, NativeObjectInvocation,
    };
    let invocations =
        [("integer", 35, 42), ("boolean", 1, 11)].map(|(name, argument, expected)| {
            let export = native
                .iter()
                .flat_map(|module| &module.functions)
                .find(|function| function.name == name)
                .expect("native trait caller");
            NativeObjectInvocation {
                export_id: export.export_id,
                arguments: vec![argument],
                expected_status: crate::compiler::native_ir::status::OK,
                expected_result: Some(expected),
            }
        });
    assert_managed_native_object_invocations("trait-choice", &native, &object, &invocations);
}

/// Explicit targets distinguish implementations even without value arguments.
#[test]
fn explicit_trait_targets_select_return_only_implementations_after_pruning() {
    let syntax = parse_module_as_syntax_output(
        r#"
module app.ExplicitTrait.
pub trait Default[T] { value(): T. }.
pub impl Default[Int] for Int { value(): Int -> 42. }.
pub impl Default[Bool] for Bool { value(): Bool -> true. }.
pub impl Default[Float] for Float { value(): Float -> 3.5. }.
pub integer(): Int -> Default[Int].value().
pub boolean(): Bool -> Default[Bool].value().
"#,
    )
    .expect("parse explicit trait targets");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = crate::terlan_typeck::type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let module = lower_syntax_module_output_to_core(&syntax, &resolved);
    let serialized = serde_json::to_string(&module).expect("serialize explicit trait arguments");
    let mut cores =
        vec![serde_json::from_str(&serialized).expect("restore explicit trait arguments")];
    crate::compiler::native_ir::prune_application_to_function_roots(
        &mut cores,
        &[
            ("app.ExplicitTrait".into(), "integer".into(), 0),
            ("app.ExplicitTrait".into(), "boolean".into(), 0),
        ],
    )
    .expect("retain both explicit implementation closures");
    assert_eq!(
        cores[0]
            .functions
            .iter()
            .filter(|function| function.trait_method.is_some())
            .count(),
        2,
        "an unreachable third implementation must not become an image root"
    );
    let native = NativeModule::lower_application(&cores.iter().collect::<Vec<_>>())
        .expect("lower explicit targets without value arguments");
    let object =
        crate::compiler::native_ir::emit_native_application_object("explicit-trait", &native)
            .expect("emit explicit trait object");
    use crate::compiler::native_ir::native_object_test_support::{
        assert_managed_native_object_invocations, NativeObjectInvocation,
    };
    let invocations = [("integer", 42), ("boolean", 1)].map(|(name, expected)| {
        let export = native
            .iter()
            .flat_map(|module| &module.functions)
            .find(|function| function.name == name)
            .expect("explicit caller");
        NativeObjectInvocation {
            export_id: export.export_id,
            arguments: vec![],
            expected_status: crate::compiler::native_ir::status::OK,
            expected_result: Some(expected),
        }
    });
    assert_managed_native_object_invocations("explicit-trait", &native, &object, &invocations);
}

/// Imported trait aliases preserve provider identity and public implementation visibility.
#[test]
fn imported_concrete_trait_alias_selects_its_owning_module() {
    use crate::terlan_hir::resolve_syntax_module_output_with_interfaces;
    let provider = core(
        r#"
module app.TraitProvider.
pub trait Score[T] { score(value: T): Int. }.
pub impl Score[Int] for Int { score(value: Int): Int -> value + 7. }.
"#,
    );
    let syntax = parse_module_as_syntax_output(
        r#"
module app.TraitConsumer.
import app.TraitProvider.{Score as Rating}.
pub run(value: Int): Int -> Rating.score(value).
pub explicit(value: Int): Int -> Rating[Int].score(value).
"#,
    )
    .expect("parse imported trait alias");
    let interfaces = HashMap::from([(provider.module.clone(), provider.interface.clone())]);
    let resolved = resolve_syntax_module_output_with_interfaces(&syntax, &interfaces).module;
    assert!(
        resolved.diagnostics.is_empty(),
        "{:?}",
        resolved.diagnostics
    );
    let diagnostics = crate::terlan_typeck::type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let consumer = lower_syntax_module_output_to_core(&syntax, &resolved);
    let mut private_provider = provider.clone();
    for function in &mut private_provider.functions {
        if function.trait_method.is_some() {
            function.public = false;
        }
    }
    let error = resolve_typed_overloads(&mut [private_provider, consumer.clone()])
        .expect_err("private implementation must not cross module boundary");
    assert!(error.contains("native_ir.overload_no_match"), "{error}");
    let mut modules = vec![provider, consumer];
    resolve_typed_overloads(&mut modules).expect("select imported implementation");
    let implementation = modules[0]
        .functions
        .iter()
        .find(|function| function.trait_method.is_some())
        .expect("provider implementation");
    assert_eq!(
        direct_call_target(&modules[1], "run"),
        format!("app.TraitProvider.{}", implementation.name)
    );
    assert_eq!(
        direct_call_target(&modules[1], "explicit"),
        direct_call_target(&modules[1], "run")
    );
}

/// Selecting a trait template must not bypass its generic argument constraints.
#[test]
fn explicit_generic_trait_method_checks_arguments_during_specialization() {
    let module = core(
        r#"
module app.GenericTrait.
pub trait Pair[T] { first[A](left: A, right: A): A. }.
pub impl Pair[Int] for Int { first(left: A, _right: A): A -> left. }.
pub run(): Int -> Pair[Int].first[Int](42, 43).
"#,
    );
    let native = NativeModule::lower_application(&[&module])
        .expect("specialize a uniquely selected trait method");
    let object =
        crate::compiler::native_ir::emit_native_application_object("generic-trait", &native)
            .expect("emit generic trait method");
    use crate::compiler::native_ir::native_object_test_support::{
        assert_managed_native_object_invocations, NativeObjectInvocation,
    };
    let export = native
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "run")
        .expect("generic caller");
    assert_managed_native_object_invocations(
        "generic-trait",
        &native,
        &object,
        &[NativeObjectInvocation {
            export_id: export.export_id,
            arguments: vec![],
            expected_status: crate::compiler::native_ir::status::OK,
            expected_result: Some(42),
        }],
    );
    let mut invalid = module;
    let body = invalid
        .functions
        .iter_mut()
        .find(|function| function.name == "run")
        .and_then(|function| function.clauses.first_mut())
        .and_then(|clause| clause.body.core_expr.as_mut())
        .expect("generic call");
    let CoreExpr::Call { args, .. } = body else {
        panic!("expected direct trait call")
    };
    args[1] = CoreExpr::Atom("true".into());
    let error = NativeModule::lower_application(&[&invalid])
        .expect_err("inconsistent generic method arguments must fail closed");
    assert!(error.contains("native_ir.generic_"), "{error}");
}

/// Incomplete or contradictory concrete implementation inventories are not guessed.
#[test]
fn concrete_trait_calls_reject_wrong_types_and_duplicate_candidates() {
    let module = core(
        r#"
module app.TraitRejection.
pub trait Score[T] { score(value: T): Int. }.
pub impl Score[Int] for Int { score(value: Int): Int -> value. }.
pub run(value: Bool): Int -> Score.score(value).
"#,
    );
    let error = resolve_typed_overloads(&mut [module.clone()]).expect_err("wrong argument type");
    assert!(error.contains("native_ir.overload_no_match"), "{error}");
    let mut duplicate = module;
    let mut implementation = duplicate
        .functions
        .iter()
        .find(|function| function.trait_method.is_some())
        .expect("implementation")
        .clone();
    implementation.name.push_str("_duplicate");
    duplicate.functions.push(implementation);
    let run = duplicate
        .functions
        .iter_mut()
        .find(|function| function.name == "run")
        .expect("caller");
    run.params[0].core_ty = Some(CoreType::Int);
    let error = resolve_typed_overloads(&mut [duplicate]).expect_err("ambiguous implementation");
    assert!(error.contains("native_ir.overload_ambiguous"), "{error}");
}
