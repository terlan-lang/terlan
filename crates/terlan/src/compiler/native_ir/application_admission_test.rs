//! Tests for closed-world direct-AOT application admission.

use crate::{
    terlan_hir::resolve_syntax_module_output,
    terlan_syntax::parse_module_as_syntax_output,
    terlan_typeck::{
        lower_syntax_module_output_to_core, CoreExpr, CoreImport, CoreImportKind, CoreModule,
    },
};

use super::{
    application::normalize_application_remote_calls,
    application_admission::validate_continuation_graph, NativeContinuation, NativeExpr,
    NativeFunction, NativeModule, NativeTransitionOperation, NativeType,
};
use crate::runtime::native_image::managed::SemanticTypeId;

/// Lowers one canonical source module into checked CoreIR.
fn core(source: &str) -> CoreModule {
    let syntax = parse_module_as_syntax_output(source).expect("parse admission source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    lower_syntax_module_output_to_core(&syntax, &resolved)
}

/// Returns the mutable body of one named CoreIR function.
fn body_mut<'a>(core: &'a mut CoreModule, name: &str) -> &'a mut CoreExpr {
    core.functions
        .iter_mut()
        .find(|function| function.name == name)
        .and_then(|function| function.clauses.first_mut())
        .and_then(|clause| clause.body.core_expr.as_mut())
        .expect("typed function body")
}

/// Creates one empty native module for continuation-graph fixtures.
fn native_module(name: &str) -> NativeModule {
    NativeModule {
        name: name.to_string(),
        functions: Vec::new(),
        continuations: Vec::new(),
        managed_layouts: Vec::new(),
        managed_collections: Vec::new(),
        atoms: Vec::new(),
    }
}

/// Creates one scalar continuation fixture.
fn continuation(id: u64) -> NativeContinuation {
    NativeContinuation {
        id,
        source_module: "app.Test".to_string(),
        source_function: "main".to_string(),
        source_arity: 0,
        source_span: None,
        capture_names: Vec::new(),
        params: Vec::new(),
        return_type: NativeType::Unit,
        body: NativeExpr::Unit,
    }
}

/// Verifies unresolved direct calls fail before candidate lowering.
#[test]
fn unresolved_application_call_has_stable_prelink_diagnostic() {
    let mut caller = core("module app.Caller.\n\npub main(): Int -> 1.\n");
    *body_mut(&mut caller, "main") = CoreExpr::Call {
        function: "missing".to_string(),
        args: Vec::new(),
    };

    assert_eq!(
        NativeModule::lower_application(&[&caller]).unwrap_err(),
        "error[native_ir.unresolved_call]: `app.Caller.missing/0` has no function in the native application closure"
    );
}

/// Verifies concrete trait dispatch cannot fall back to runtime interpretation.
#[test]
fn unresolved_trait_impl_dispatch_has_stable_prelink_diagnostic() {
    let module = core(
        "module app.TraitDispatch.\n\n\
         pub struct Profile { title: String }.\n\n\
         pub trait Render[T] { render(value: T): String. }.\n\n\
         pub impl Render[Profile] for Profile {\n\
             render(value: Profile): String -> value.title.\n\
         }.\n\n\
         pub run(): String -> Render.render(Profile {title: \"Engineer\"}).\n",
    );

    assert_eq!(
        NativeModule::lower_application(&[&module]).unwrap_err(),
        "error[native_ir.unresolved_call]: `app.TraitDispatch.Render.render/1` has no function in the native application closure"
    );
}

/// Verifies indexed assignment cannot fall back to runtime interpretation.
#[test]
fn unresolved_index_assignment_has_stable_prelink_diagnostic() {
    let mut module = core("module app.IndexAssignment.\n\npub run(): Unit -> Unit.\n");
    *body_mut(&mut module, "run") = CoreExpr::Call {
        function: "IndexSet.set_at".to_string(),
        args: vec![
            CoreExpr::List(vec![CoreExpr::Int(1)]),
            CoreExpr::Int(0),
            CoreExpr::Int(2),
        ],
    };

    assert_eq!(
        NativeModule::lower_application(&[&module]).unwrap_err(),
        "error[native_ir.unresolved_call]: `app.IndexAssignment.IndexSet.set_at/3` has no function in the native application closure"
    );
}

/// Verifies conflicting imported symbols cannot acquire an accidental ABI.
#[test]
fn incompatible_imported_function_abis_are_rejected() {
    let integer = core("module app.Integer.\n\npub convert(value: Int): Int -> value.\n");
    let floating = core("module app.Floating.\n\npub convert(value: Float): Float -> value.\n");
    let mut caller = core("module app.Caller.\n\npub main(): Int -> 1.\n");
    caller.imports.extend([
        CoreImport {
            module: "app.Integer".to_string(),
            kind: CoreImportKind::Module,
        },
        CoreImport {
            module: "app.Floating".to_string(),
            kind: CoreImportKind::Module,
        },
    ]);
    *body_mut(&mut caller, "main") = CoreExpr::Call {
        function: "convert".to_string(),
        args: vec![CoreExpr::Int(1)],
    };

    assert_eq!(
        NativeModule::lower_application(&[&caller, &integer, &floating]).unwrap_err(),
        "error[native_ir.import_abi]: call `convert/1` in module `app.Caller` resolves to app.Floating.convert/1, app.Integer.convert/1"
    );
}

/// Verifies same-shaped imported symbols remain ambiguous rather than being
/// selected according to input ordering.
#[test]
fn duplicate_compatible_imports_are_rejected_as_ambiguous() {
    let left = core("module app.Left.\n\npub convert(value: Int): Int -> value.\n");
    let right = core("module app.Right.\n\npub convert(value: Int): Int -> value.\n");
    let mut caller = core("module app.Caller.\n\npub main(): Int -> 1.\n");
    caller.imports.extend([
        CoreImport {
            module: "app.Left".to_string(),
            kind: CoreImportKind::Module,
        },
        CoreImport {
            module: "app.Right".to_string(),
            kind: CoreImportKind::Module,
        },
    ]);
    *body_mut(&mut caller, "main") = CoreExpr::Call {
        function: "convert".to_string(),
        args: vec![CoreExpr::Int(1)],
    };

    assert_eq!(
        NativeModule::lower_application(&[&caller, &left, &right]).unwrap_err(),
        "error[native_ir.ambiguous_import]: call `convert/1` in module `app.Caller` resolves to app.Left.convert/1, app.Right.convert/1"
    );
}

/// Verifies malformed CoreIR arity metadata cannot cross the native ABI.
#[test]
fn malformed_function_abi_is_rejected() {
    let mut module = core("module app.Malformed.\n\npub main(): Int -> 1.\n");
    module.functions[0].arity = 1;

    assert_eq!(
        NativeModule::lower_application(&[&module]).unwrap_err(),
        "error[native_ir.function_abi]: `app.Malformed.main/1` declares 0 parameters"
    );
}

/// Verifies repeated module identities cannot be collapsed according to input
/// ordering.
#[test]
fn duplicate_module_identity_is_rejected() {
    let first = core("module app.Duplicate.\n\npub first(): Int -> 1.\n");
    let second = core("module app.Duplicate.\n\npub second(): Int -> 2.\n");

    assert_eq!(
        NativeModule::lower_application(&[&first, &second]).unwrap_err(),
        "error[native_ir.duplicate_module]: application contains duplicate module `app.Duplicate`"
    );
}

/// Verifies repeated local function identities cannot overwrite each other in
/// the application resolver.
#[test]
fn duplicate_function_identity_is_rejected() {
    let mut module = core("module app.Duplicate.\n\npub main(): Int -> 1.\n");
    module.functions.push(module.functions[0].clone());

    assert_eq!(
        NativeModule::lower_application(&[&module]).unwrap_err(),
        "error[native_ir.function_identity]: duplicate function `app.Duplicate.main/0`"
    );
}

/// Verifies duplicate continuation identities fail before object emission.
#[test]
fn duplicate_continuation_identity_is_rejected() {
    let mut left = native_module("app.Left");
    left.continuations.push(continuation(7));
    let mut right = native_module("app.Right");
    right.continuations.push(continuation(7));

    assert_eq!(
        validate_continuation_graph(&[left, right]).unwrap_err(),
        "error[native_ir.continuation_graph]: continuation identity 7 is ambiguous"
    );
}

/// Verifies the reserved zero identity cannot represent a resume entry.
#[test]
fn zero_continuation_identity_is_rejected() {
    let mut module = native_module("app.Zero");
    module.continuations.push(continuation(0));

    assert_eq!(
        validate_continuation_graph(&[module]).unwrap_err(),
        "error[native_ir.continuation_graph]: continuation identity 0 is ambiguous"
    );
}

/// Verifies a suspension cannot reference an absent resume entry.
#[test]
fn dangling_continuation_reference_is_rejected() {
    let mut module = native_module("app.Dangling");
    module.functions.push(NativeFunction {
        export_id: 1,
        name: "main".to_string(),
        public: true,
        arity: 0,
        source_module: "app.Dangling".to_string(),
        source_function: "main".to_string(),
        source_arity: 0,
        callable_captures: Vec::new(),
        params: Vec::new(),
        return_type: NativeType::Unit,
        body: NativeExpr::Suspend {
            operation: NativeTransitionOperation::Yield,
            arguments: Vec::new(),
            continuation_id: 9,
            values: Vec::new(),
        },
    });

    assert_eq!(
        validate_continuation_graph(&[module]).unwrap_err(),
        "error[native_ir.continuation_graph]: module `app.Dangling` references missing continuation 9; referenced by `app.Dangling.main/0`"
    );
}

/// Verifies an ordinary closed application still reaches NativeIR.
#[test]
fn closed_application_passes_admission_and_graph_validation() {
    let module = core(
        "module app.Closed.\n\nprivate(value: Int): Int -> value + 1.\n\npub main(): Int -> private(41).\n",
    );
    let native = NativeModule::lower_application(&[&module]).expect("closed native application");

    assert_eq!(native.len(), 1);
    assert!(native[0]
        .functions
        .iter()
        .any(|function| function.name == "main"));
}

/// A short-circuited suspending binding keeps its Boolean continuation type
/// instead of inheriting the enclosing Unit-returning function type.
#[test]
fn suspending_boolean_binding_does_not_inherit_enclosing_unit_type() {
    let module = core(
        "module app.NativeBoolean.\n\n\
         @compiler.native {fixture.scalar}\n\
         scalar(): Float -> native.\n\n\
         sink(_valid: Bool): Unit -> Unit.\n\n\
         pub main(): Unit ->\n\
             let valid = scalar() == 0.0 and scalar() == 0.0;\n\
             sink(valid).\n",
    );

    let native = NativeModule::lower_application(&[&module])
        .expect("lower short-circuited native Boolean binding");

    assert!(native[0]
        .functions
        .iter()
        .any(|function| function.name == "main" && function.return_type == NativeType::Unit));
}

#[test]
fn suspending_left_operand_short_circuits_a_later_suspending_call() {
    let module = core(
        "module app.SuspendingAnd.\n\n\
         @compiler.native {fixture.left}\n\
         left(): Bool -> native.\n\n\
         @compiler.native {fixture.right}\n\
         right(): Bool -> native.\n\n\
         wrapped_left(): Bool -> left().\n\n\
         pub validate(require_right: Bool): Bool ->\n\
             wrapped_left() and if {\n\
                 require_right -> right();\n\
                 true -> true\n\
             }.\n",
    );

    let native = NativeModule::lower_application(&[&module])
        .expect("lower suspending left-hand short circuit");
    let continuation = native
        .iter()
        .find(|module| module.name == "$terlan.continuations")
        .and_then(|module| {
            module.functions.iter().find(|function| {
                function.source_function == "validate" && function.params.len() == 2
            })
        })
        .expect("validate completion continuation");
    let NativeExpr::If { clauses } = &continuation.body else {
        panic!("expected outer short-circuit gate: {continuation:#?}");
    };
    assert_eq!(clauses.len(), 2);
    assert_eq!(clauses[0].0, NativeExpr::Param(1));
    assert_eq!(
        clauses[1],
        (NativeExpr::Bool(true), NativeExpr::Bool(false))
    );
}

/// Verifies receiver calls resolve through the caller's explicit module import
/// when multiple application modules export the same name and arity.
#[test]
fn receiver_call_prefers_explicitly_imported_provider() {
    let json =
        core("module app.Json.\n\npub put(value: Int, key: String, item: Int): Int -> item.\n");
    let map =
        core("module app.Map.\n\npub put(value: Int, key: String, item: Int): Int -> item.\n");
    let mut caller = core("module app.Consumer.\n\npub run(value: Int): Int -> value.\n");
    caller.imports.push(CoreImport {
        module: "app.Json".to_string(),
        kind: CoreImportKind::Module,
    });
    *body_mut(&mut caller, "run") = CoreExpr::RemoteCall {
        module: "__receiver__".to_string(),
        function: "put".to_string(),
        args: vec![
            CoreExpr::Var("value".to_string()),
            CoreExpr::Binary("\"key\"".to_string()),
            CoreExpr::Int(1),
        ],
    };
    let mut modules = vec![caller, json, map];

    normalize_application_remote_calls(&mut modules, false);

    assert!(matches!(
        body_mut(&mut modules[0], "run"),
        CoreExpr::Call { function, .. } if function == "app.Json.put"
    ));
}

/// Verifies mutable opaque receiver calls use the same import-directed
/// resolution while retaining the receiver as the first call argument.
#[test]
fn mutable_receiver_call_prefers_explicitly_imported_provider() {
    let json =
        core("module app.Json.\n\npub put(value: Int, key: String, item: Int): Int -> item.\n");
    let map =
        core("module app.Map.\n\npub put(value: Int, key: String, item: Int): Int -> item.\n");
    let mut caller = core("module app.Consumer.\n\npub run(value: Int): Int -> value.\n");
    caller.imports.push(CoreImport {
        module: "app.Json".to_string(),
        kind: CoreImportKind::Module,
    });
    *body_mut(&mut caller, "run") = CoreExpr::MutableReceiverCall {
        receiver: Box::new(CoreExpr::Var("value".to_string())),
        method: "put".to_string(),
        args: vec![CoreExpr::Binary("\"key\"".to_string()), CoreExpr::Int(1)],
        effects: crate::terlan_typeck::CoreEffectSet {
            effects: vec!["receiver_mutation".to_string()],
        },
    };
    let mut modules = vec![caller, json, map];

    normalize_application_remote_calls(&mut modules, false);

    assert!(matches!(
        body_mut(&mut modules[0], "run"),
        CoreExpr::Call { function, args }
            if function == "app.Json.put"
                && matches!(args.first(), Some(CoreExpr::Var(name)) if name == "value")
    ));
}

/// Verifies an imported opaque package type has one application-wide managed
/// identity before any suspending continuation can capture it.
#[test]
fn imported_opaque_package_type_uses_qualified_capture_identity() {
    let resource = core("module app.Resource.\n\npub opaque type Resource.\n");
    let consumer = core(
        "module app.Consumer.\n\n\
         import app.Resource.{Resource}.\n\n\
         pub keep(value: Resource): Resource -> value.\n",
    );

    let modules =
        NativeModule::lower_application(&[&consumer, &resource]).expect("lower opaque package");
    let keep = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "keep")
        .expect("consumer keep function");
    let expected = NativeType::ManagedRef(
        SemanticTypeId::from_canonical("app.Resource.Resource")
            .expect("canonical opaque package identity"),
    );

    assert_eq!(keep.params, vec![expected]);
    assert_eq!(keep.return_type, expected);
}

/// Verifies an unsupported reachable function keeps its stable prelink error.
#[test]
fn unsupported_reachable_function_is_rejected_before_linking() {
    let mut module = core("module app.Unsupported.\n\npub value(): Int -> 1.\n");
    *body_mut(&mut module, "value") = CoreExpr::FixedArray(vec![CoreExpr::Int(1)]);

    assert_eq!(
        NativeModule::lower_application(&[&module]).unwrap_err(),
        "error[native_ir.unsupported_application_function]: `app.Unsupported.value/0` cannot be lowered into the native application image (native-operation=true, parameters=true, result=true, clause=true, body=false, body-gap=FixedArray(Int(1)), missing-core=none); runtime CoreIR interpretation has been removed"
    );
}
