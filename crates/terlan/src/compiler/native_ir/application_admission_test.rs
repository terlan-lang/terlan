//! Tests for closed-world direct-AOT application admission.

use crate::{
    terlan_hir::resolve_syntax_module_output,
    terlan_syntax::parse_module_as_syntax_output,
    terlan_typeck::{
        lower_syntax_module_output_to_core, CoreExpr, CoreImport, CoreImportKind, CoreModule,
    },
};

use super::{
    application_admission::validate_continuation_graph, NativeContinuation, NativeExpr,
    NativeFunction, NativeModule, NativeTransitionOperation, NativeType,
};

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
        "error[native_ir.continuation_graph]: module `app.Dangling` references missing continuation 9"
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

/// Verifies an unsupported reachable function keeps its stable prelink error.
#[test]
fn unsupported_reachable_function_is_rejected_before_linking() {
    let mut module = core("module app.Unsupported.\n\npub value(): Int -> 1.\n");
    *body_mut(&mut module, "value") = CoreExpr::Atom("unbounded_runtime_atom".to_string());

    assert_eq!(
        NativeModule::lower_application(&[&module]).unwrap_err(),
        "error[native_ir.unsupported_application_function]: `app.Unsupported.value/0` cannot be lowered into the native application image; runtime CoreIR interpretation has been removed"
    );
}
