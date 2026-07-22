//! Executable checks for generated managed-allocation callback calls.

use std::ffi::c_void;
use std::fs;
use std::process::Command;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use libloading::{Library, Symbol};

use crate::runtime::native_image::managed::{
    encode_aggregate_layout, encode_closure_allocation, ManagedAggregateDescriptor, ManagedClosure,
    ManagedExecutionRuntime, ManagedFieldType, SemanticTypeId,
};
use crate::runtime::native_image::{TvmBoundaryType, TvmCallableDescriptor};
use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::lower_syntax_module_output_to_core;

use super::super::{
    NativeBinaryOperator, NativeExpr, NativeFunction, NativeModule, NativeTransitionOperation,
    NativeType,
};
use super::emit_native_application_object;

type NativeDispatch = unsafe extern "C" fn(
    *mut c_void,
    *const c_void,
    *const c_void,
    u64,
    *const i64,
    u64,
    *mut i64,
    *mut i64,
    u64,
    *mut u64,
) -> i32;

#[test]
fn generated_constructor_reference_crosses_native_arguments_and_returns() {
    let canonical = "Result[Int, Bool]";
    let descriptor = Arc::new(
        ManagedAggregateDescriptor::constructor(
            canonical,
            "Ok",
            0,
            2,
            vec![
                (Some("value".to_owned()), ManagedFieldType::Int),
                (Some("ready".to_owned()), ManagedFieldType::Bool),
            ],
        )
        .expect("constructor descriptor"),
    );
    let layout = Arc::<[u8]>::from(
        encode_aggregate_layout(&descriptor).expect("encoded constructor descriptor"),
    );
    let result = NativeType::ManagedRef(
        SemanticTypeId::from_canonical(canonical).expect("constructor semantic identity"),
    );
    let module = NativeModule {
        name: "ManagedCallback".to_owned(),
        functions: vec![
            NativeFunction {
                export_id: 89,
                name: "construct".to_owned(),
                public: false,
                arity: 2,
                callable_captures: Vec::new(),
                params: vec![NativeType::Int, NativeType::Bool],
                return_type: result,
                body: NativeExpr::Construct {
                    descriptor,
                    encoded_layout: layout.clone(),
                    fields: vec![NativeExpr::Param(0), NativeExpr::Param(1)],
                },
            },
            NativeFunction {
                export_id: 90,
                name: "identity".to_owned(),
                public: false,
                arity: 1,
                callable_captures: Vec::new(),
                params: vec![result],
                return_type: result,
                body: NativeExpr::Param(0),
            },
            NativeFunction {
                export_id: 91,
                name: "ok".to_owned(),
                public: true,
                arity: 2,
                callable_captures: Vec::new(),
                params: vec![NativeType::Int, NativeType::Bool],
                return_type: result,
                body: NativeExpr::Call {
                    function: 1,
                    args: vec![NativeExpr::Call {
                        function: 0,
                        args: vec![NativeExpr::Param(0), NativeExpr::Param(1)],
                    }],
                },
            },
        ],
        continuations: vec![],
        managed_layouts: vec![layout],
        managed_collections: vec![],
        atoms: vec![],
    };
    let object = emit_native_application_object("managed_callback", &[module])
        .expect("managed callback object");
    let root = std::env::temp_dir().join(format!(
        "terlan-managed-callback-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("callback fixture directory");
    let object_path = root.join("managed.o");
    let harness_path = root.join("harness.rs");
    let executable_path = root.join("harness");
    fs::write(&object_path, object).expect("generated callback object");
    fs::write(&harness_path, HARNESS).expect("callback harness");

    let compile = Command::new("rustc")
        .arg("--edition=2021")
        .arg(&harness_path)
        .arg("-C")
        .arg(format!("link-arg={}", object_path.display()))
        .arg("-o")
        .arg(&executable_path)
        .output()
        .expect("compile callback harness");
    assert!(
        compile.status.success(),
        "callback harness failed:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new(&executable_path)
        .output()
        .expect("run callback harness");
    assert!(
        run.status.success(),
        "callback harness rejected generated ABI:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    fs::remove_dir_all(root).expect("remove callback fixture");
}

#[test]
#[allow(unsafe_code)]
fn generated_closure_owns_captures_and_dispatches_lifted_target() {
    let callable = TvmCallableDescriptor {
        id: 201,
        parameters: vec![TvmBoundaryType::Int],
        results: vec![TvmBoundaryType::Int],
        captures: vec![TvmBoundaryType::Int],
    };
    let digest = [17_u8; 32];
    let mut runtime = ManagedExecutionRuntime::with_executable_image_metadata(
        &[],
        &[],
        &[],
        digest,
        std::slice::from_ref(&callable),
    )
    .expect("managed executable metadata");
    let table = runtime
        .closure_dispatch()
        .expect("admitted closure table")
        .clone();
    let closure_semantic = table
        .closure_descriptor(callable.id)
        .expect("closure descriptor")
        .semantic_id();
    let encoded = Arc::<[u8]>::from(
        encode_closure_allocation(callable.id).expect("closure allocation descriptor"),
    );
    let module = NativeModule {
        name: "GeneratedClosure".to_owned(),
        functions: vec![
            NativeFunction {
                export_id: callable.id,
                name: "lifted_add".to_owned(),
                public: false,
                arity: 2,
                callable_captures: vec![NativeType::Int],
                params: vec![NativeType::Int, NativeType::Int],
                return_type: NativeType::Int,
                body: NativeExpr::Binary {
                    operator: NativeBinaryOperator::Add,
                    operand_type: NativeType::Int,
                    left: Box::new(NativeExpr::Param(0)),
                    right: Box::new(NativeExpr::Param(1)),
                },
            },
            NativeFunction {
                export_id: 202,
                name: "make_adder".to_owned(),
                public: true,
                arity: 0,
                callable_captures: Vec::new(),
                params: Vec::new(),
                return_type: NativeType::ManagedRef(closure_semantic),
                body: NativeExpr::MakeClosure {
                    encoded,
                    captures: vec![NativeExpr::Int(40)],
                },
            },
        ],
        continuations: vec![],
        managed_layouts: vec![],
        managed_collections: vec![],
        atoms: vec![],
    };
    let object = emit_native_application_object("generated_closure", &[module])
        .expect("generated closure object");
    let (library, root) = link_test_library("generated-closure", &object);
    // SAFETY: The freshly linked test image exports the frozen format-1 dispatch ABI.
    let dispatch: Symbol<'_, NativeDispatch> = unsafe {
        library
            .get(b"terlan_native_dispatch_v2")
            .expect("native dispatch symbol")
    };
    let dispatch = *dispatch;
    let owner = 91;
    let closure_word = runtime.with_dispatch(owner, |context, allocator, resolver| {
        invoke_dispatch(dispatch, context, allocator, resolver, 202, &[])
    });
    let closure_word = closure_word.expect("generated closure allocation");
    let invocation = runtime
        .with_public_materialization(owner, |heap, _| {
            let closure = heap
                .validate_abi_reference(
                    u64::from_ne_bytes(closure_word.to_ne_bytes()),
                    closure_semantic,
                )
                .map_err(|error| error.to_string())?
                .cast::<ManagedClosure>();
            let view = heap
                .closure_view(closure)
                .map_err(|error| error.to_string())?;
            assert_eq!(view.callable_id, callable.id);
            assert_eq!(view.capture_words, [40]);
            heap.prepare_closure_invocation(
                closure,
                &table,
                table.generation(),
                &[TvmBoundaryType::Int],
                &[2],
                &[TvmBoundaryType::Int],
            )
            .map_err(|error| error.to_string())
        })
        .expect("validated closure invocation");
    let answer = runtime.with_dispatch(owner, |context, allocator, resolver| {
        invoke_dispatch(
            dispatch,
            context,
            allocator,
            resolver,
            invocation.target().callable_id(),
            invocation.words(),
        )
    });
    assert_eq!(answer.expect("lifted closure target"), 42);
    drop(library);
    fs::remove_dir_all(root).expect("remove generated closure fixture");
}

#[test]
#[allow(unsafe_code)]
fn owned_closure_forwards_a_suspending_target_transition() {
    let callable = TvmCallableDescriptor {
        id: 301,
        parameters: vec![],
        results: vec![TvmBoundaryType::Unit],
        captures: vec![],
    };
    let mut runtime = ManagedExecutionRuntime::with_executable_image_metadata(
        &[],
        &[],
        &[],
        [31; 32],
        std::slice::from_ref(&callable),
    )
    .expect("suspending callable metadata");
    let closure_semantic = runtime
        .closure_dispatch()
        .expect("closure dispatch table")
        .closure_descriptor(callable.id)
        .expect("closure descriptor")
        .semantic_id();
    let encoded = Arc::<[u8]>::from(
        encode_closure_allocation(callable.id).expect("closure allocation descriptor"),
    );
    let module = NativeModule {
        name: "SuspendingClosure".to_owned(),
        functions: vec![
            NativeFunction {
                export_id: callable.id,
                name: "park".to_owned(),
                public: false,
                arity: 0,
                callable_captures: vec![],
                params: vec![],
                return_type: NativeType::Unit,
                body: NativeExpr::Suspend {
                    operation: NativeTransitionOperation::Yield,
                    arguments: vec![],
                    continuation_id: 333,
                    values: vec![],
                },
            },
            NativeFunction {
                export_id: 302,
                name: "make_park".to_owned(),
                public: true,
                arity: 0,
                callable_captures: vec![],
                params: vec![],
                return_type: NativeType::ManagedRef(closure_semantic),
                body: NativeExpr::MakeClosure {
                    encoded,
                    captures: vec![],
                },
            },
            NativeFunction {
                export_id: 303,
                name: "invoke_park".to_owned(),
                public: true,
                arity: 1,
                callable_captures: vec![],
                params: vec![NativeType::ManagedRef(closure_semantic)],
                return_type: NativeType::Unit,
                body: NativeExpr::InvokeClosure {
                    callee: Box::new(NativeExpr::Param(0)),
                    args: vec![],
                    parameter_types: vec![],
                    result_type: NativeType::Unit,
                },
            },
        ],
        continuations: vec![],
        managed_layouts: vec![],
        managed_collections: vec![],
        atoms: vec![],
    };
    let object = emit_native_application_object("suspending_closure", &[module])
        .expect("suspending closure object");
    let (library, root) = link_test_library("suspending-closure", &object);
    let dispatch: Symbol<'_, NativeDispatch> = unsafe {
        library
            .get(b"terlan_native_dispatch_v2")
            .expect("native dispatch symbol")
    };
    let dispatch = *dispatch;
    let owner = 93;
    let closure = runtime
        .with_dispatch(owner, |context, allocator, resolver| {
            invoke_dispatch(dispatch, context, allocator, resolver, 302, &[])
        })
        .expect("suspending closure allocation");
    let mut result = -1_i64;
    let mut transitions =
        [0_i64; crate::runtime::native_image::TVM_INDIRECT_TRANSITION_WORD_CAPACITY];
    let mut transition_len = u64::MAX;
    let status = runtime.with_dispatch(owner, |context, allocator, resolver| unsafe {
        dispatch(
            context,
            allocator,
            resolver,
            303,
            [closure].as_ptr(),
            1,
            &mut result,
            transitions.as_mut_ptr(),
            transitions.len() as u64,
            &mut transition_len,
        )
    });
    assert_eq!(status, super::super::status::YIELD);
    assert_eq!(result, 333);
    assert_eq!(transition_len, 0);
    drop(library);
    fs::remove_dir_all(root).expect("remove suspending closure fixture");
}

#[test]
#[allow(unsafe_code)]
fn source_named_suspending_closure_forwards_its_transition() {
    let syntax = parse_module_as_syntax_output(
        "module suspending_named.\n\n\
         import std.vm.Process.\n\n\
         park(): Unit -> Process.yield_now().\n\n\
         make(): (() -> Unit) -> park.\n\n\
         pub apply(callback: () -> Unit): Unit -> callback().\n",
    )
    .expect("parse suspending closure source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("suspending closure NativeIR");
    let functions = modules
        .iter()
        .flat_map(|module| &module.functions)
        .collect::<Vec<_>>();
    let maker = functions
        .iter()
        .find(|function| function.name == "make")
        .expect("closure maker");
    let apply = functions
        .iter()
        .find(|function| function.name == "apply")
        .expect("closure caller");
    let mut callables = functions
        .iter()
        .map(|function| TvmCallableDescriptor {
            id: function.export_id,
            parameters: function
                .params
                .iter()
                .skip(function.callable_captures.len())
                .copied()
                .map(NativeType::boundary_type)
                .collect(),
            results: vec![function.return_type.boundary_type()],
            captures: function
                .callable_captures
                .iter()
                .copied()
                .map(NativeType::boundary_type)
                .collect(),
        })
        .collect::<Vec<_>>();
    callables.sort_by_key(|callable| callable.id);
    let mut runtime = ManagedExecutionRuntime::with_executable_image_metadata(
        &[],
        &[],
        &[],
        [37; 32],
        &callables,
    )
    .expect("source callable table");
    let object = emit_native_application_object("source_suspending_closure", &modules)
        .expect("source suspending closure object");
    let (library, root) = link_test_library("source-suspending-closure", &object);
    let dispatch: Symbol<'_, NativeDispatch> = unsafe {
        library
            .get(b"terlan_native_dispatch_v2")
            .expect("native dispatch symbol")
    };
    let dispatch = *dispatch;
    let owner = 94;
    let closure = runtime
        .with_dispatch(owner, |context, allocator, resolver| {
            invoke_dispatch(dispatch, context, allocator, resolver, maker.export_id, &[])
        })
        .expect("source suspending closure allocation");
    let mut result = -1_i64;
    let mut transitions =
        [0_i64; crate::runtime::native_image::TVM_INDIRECT_TRANSITION_WORD_CAPACITY];
    let mut transition_len = u64::MAX;
    let status = runtime.with_dispatch(owner, |context, allocator, resolver| unsafe {
        dispatch(
            context,
            allocator,
            resolver,
            apply.export_id,
            [closure].as_ptr(),
            1,
            &mut result,
            transitions.as_mut_ptr(),
            transitions.len() as u64,
            &mut transition_len,
        )
    });
    assert_eq!(status, super::super::status::YIELD);
    assert_ne!(result, 0, "generated continuation identity");
    assert_eq!(transition_len, 0);
    drop(library);
    fs::remove_dir_all(root).expect("remove source suspending closure fixture");
}

#[test]
#[allow(unsafe_code)]
fn source_named_function_value_becomes_executable_owned_closure() {
    let syntax = parse_module_as_syntax_output(
        "module escaping_named.\n\n\
         double(value: Int): Int -> value * 2.\n\n\
         pub apply(value: Int, callback: (Int) -> Int): Int -> callback(value).\n\n\
         pub apply_later(value: Int, callback: (Int) -> Int): Int -> let saved = callback; let adjusted = value + 1; saved(adjusted).\n\n\
         make(): ((Int) -> Int) -> double.\n\n\
         identity(value: String): String -> value.\n\n\
         pub apply_string(value: String, callback: (String) -> String): String -> callback(value).\n\n\
         make_string(): ((String) -> String) -> identity.\n",
    )
    .expect("parse escaping named function source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("source closure NativeIR");
    let functions = modules
        .iter()
        .flat_map(|module| &module.functions)
        .collect::<Vec<_>>();
    let target = functions
        .iter()
        .find(|function| function.name == "double")
        .expect("named target");
    let maker = functions
        .iter()
        .find(|function| function.name == "make")
        .expect("closure maker");
    let apply = functions
        .iter()
        .find(|function| function.name == "apply")
        .expect("public closure caller");
    let apply_later = functions
        .iter()
        .find(|function| function.name == "apply_later")
        .expect("stored closure caller");
    let apply_string = functions
        .iter()
        .find(|function| function.name == "apply_string")
        .expect("public managed closure caller");
    let make_string = functions
        .iter()
        .find(|function| function.name == "make_string")
        .expect("managed closure maker");
    let NativeType::ManagedRef(closure_semantic) = maker.return_type else {
        panic!(
            "maker return is not a managed closure: {:?}",
            maker.return_type
        );
    };
    let NativeExpr::MakeClosure { encoded, captures } = &maker.body else {
        panic!(
            "maker did not lower to closure allocation: {:?}",
            maker.body
        );
    };
    assert!(captures.is_empty());
    assert_eq!(
        u64::from_le_bytes(encoded[16..24].try_into().expect("callable bytes")),
        target.export_id
    );

    let mut callables = functions
        .iter()
        .map(|function| TvmCallableDescriptor {
            id: function.export_id,
            parameters: function
                .params
                .iter()
                .skip(function.callable_captures.len())
                .copied()
                .map(NativeType::boundary_type)
                .collect(),
            results: vec![function.return_type.boundary_type()],
            captures: function
                .callable_captures
                .iter()
                .copied()
                .map(NativeType::boundary_type)
                .collect(),
        })
        .collect::<Vec<_>>();
    callables.sort_by_key(|callable| callable.id);
    let mut runtime = ManagedExecutionRuntime::with_executable_image_metadata(
        &[],
        &[],
        &[],
        [23; 32],
        &callables,
    )
    .expect("source image metadata");
    let table = runtime
        .closure_dispatch()
        .expect("source callable table")
        .clone();
    let object = emit_native_application_object("source_named_closure", &modules)
        .expect("source named closure object");
    let (library, root) = link_test_library("source-named-closure", &object);
    // SAFETY: The freshly linked test image exports the frozen format-1 dispatch ABI.
    let dispatch: Symbol<'_, NativeDispatch> = unsafe {
        library
            .get(b"terlan_native_dispatch_v2")
            .expect("source native dispatch symbol")
    };
    let dispatch = *dispatch;
    let owner = 92;
    let closure_word = runtime
        .with_dispatch(owner, |context, allocator, resolver| {
            invoke_dispatch(dispatch, context, allocator, resolver, maker.export_id, &[])
        })
        .expect("source closure allocation");
    let invocation = runtime
        .with_public_materialization(owner, |heap, _| {
            let closure = heap
                .validate_abi_reference(
                    u64::from_ne_bytes(closure_word.to_ne_bytes()),
                    closure_semantic,
                )
                .map_err(|error| error.to_string())?
                .cast::<ManagedClosure>();
            let view = heap
                .closure_view(closure)
                .map_err(|error| error.to_string())?;
            assert_eq!(view.callable_id, target.export_id);
            assert!(view.capture_words.is_empty());
            heap.prepare_closure_invocation(
                closure,
                &table,
                table.generation(),
                &[TvmBoundaryType::Int],
                &[21],
                &[TvmBoundaryType::Int],
            )
            .map_err(|error| error.to_string())
        })
        .expect("source closure invocation");
    let answer = runtime.with_dispatch(owner, |context, allocator, resolver| {
        invoke_dispatch(
            dispatch,
            context,
            allocator,
            resolver,
            invocation.target().callable_id(),
            invocation.words(),
        )
    });
    assert_eq!(answer.expect("source named target"), 42);
    let indirect_answer = runtime.with_dispatch(owner, |context, allocator, resolver| {
        invoke_dispatch(
            dispatch,
            context,
            allocator,
            resolver,
            apply.export_id,
            &[21, closure_word],
        )
    });
    assert_eq!(indirect_answer.expect("public owned-closure call"), 42);
    let stored_answer = runtime.with_dispatch(owner, |context, allocator, resolver| {
        invoke_dispatch(
            dispatch,
            context,
            allocator,
            resolver,
            apply_later.export_id,
            &[21, closure_word],
        )
    });
    assert_eq!(stored_answer.expect("stored owned-closure call"), 44);
    let string_closure = runtime
        .with_dispatch(owner, |context, allocator, resolver| {
            invoke_dispatch(
                dispatch,
                context,
                allocator,
                resolver,
                make_string.export_id,
                &[],
            )
        })
        .expect("managed closure allocation");
    let string_word = runtime
        .allocate_string_value(owner, "managed closure")
        .expect("managed closure argument");
    let string_result = runtime
        .with_dispatch(owner, |context, allocator, resolver| {
            invoke_dispatch(
                dispatch,
                context,
                allocator,
                resolver,
                apply_string.export_id,
                &[string_word, string_closure],
            )
        })
        .expect("managed owned-closure call");
    assert_eq!(
        runtime
            .materialize_string_value(owner, string_result)
            .expect("managed closure result"),
        "managed closure"
    );
    drop(library);
    fs::remove_dir_all(root).expect("remove source closure fixture");
}

#[test]
#[allow(unsafe_code)]
fn source_let_local_captured_lambda_becomes_executable_owned_closure() {
    let syntax = parse_module_as_syntax_output(
        "module escaping_lambda.\n\n\
         make(seed: Int): ((Int) -> Int) ->\n\
             let offset = seed + 1;\n\
             let callback = ((value: Int) -> value + offset);\n\
             callback.\n",
    )
    .expect("parse escaping captured lambda source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("captured lambda NativeIR");
    let functions = modules
        .iter()
        .flat_map(|module| &module.functions)
        .collect::<Vec<_>>();
    let maker = functions
        .iter()
        .find(|function| function.name == "make")
        .expect("closure maker");
    let target = functions
        .iter()
        .find(|function| function.name.starts_with("$closure_make_1_"))
        .expect("lifted closure target");
    assert_eq!(target.callable_captures, vec![NativeType::Int]);
    assert_eq!(target.params, vec![NativeType::Int, NativeType::Int]);
    assert_eq!(target.arity, 2);
    let NativeType::ManagedRef(closure_semantic) = maker.return_type else {
        panic!(
            "maker return is not a managed closure: {:?}",
            maker.return_type
        );
    };
    let NativeExpr::Let { bindings, body } = &maker.body else {
        panic!("maker did not lower its lexical prefix: {:?}", maker.body);
    };
    assert_eq!(bindings.len(), 1);
    let NativeExpr::MakeClosure { encoded, captures } = body.as_ref() else {
        panic!("maker did not lower to closure allocation: {body:?}");
    };
    assert_eq!(captures, &vec![NativeExpr::Param(1)]);
    assert_eq!(
        u64::from_le_bytes(encoded[16..24].try_into().expect("callable bytes")),
        target.export_id
    );

    let mut callables = functions
        .iter()
        .map(|function| TvmCallableDescriptor {
            id: function.export_id,
            parameters: function
                .params
                .iter()
                .skip(function.callable_captures.len())
                .copied()
                .map(NativeType::boundary_type)
                .collect(),
            results: vec![function.return_type.boundary_type()],
            captures: function
                .callable_captures
                .iter()
                .copied()
                .map(NativeType::boundary_type)
                .collect(),
        })
        .collect::<Vec<_>>();
    callables.sort_by_key(|callable| callable.id);
    let mut runtime = ManagedExecutionRuntime::with_executable_image_metadata(
        &[],
        &[],
        &[],
        [24; 32],
        &callables,
    )
    .expect("captured source image metadata");
    let table = runtime
        .closure_dispatch()
        .expect("captured source callable table")
        .clone();
    let object = emit_native_application_object("source_captured_closure", &modules)
        .expect("source captured closure object");
    let (library, root) = link_test_library("source-captured-closure", &object);
    // SAFETY: The freshly linked test image exports the frozen format-1 dispatch ABI.
    let dispatch: Symbol<'_, NativeDispatch> = unsafe {
        library
            .get(b"terlan_native_dispatch_v2")
            .expect("source native dispatch symbol")
    };
    let dispatch = *dispatch;
    let owner = 93;
    let closure_word = runtime
        .with_dispatch(owner, |context, allocator, resolver| {
            invoke_dispatch(
                dispatch,
                context,
                allocator,
                resolver,
                maker.export_id,
                &[39],
            )
        })
        .expect("source captured closure allocation");
    let invocation = runtime
        .with_public_materialization(owner, |heap, _| {
            let closure = heap
                .validate_abi_reference(
                    u64::from_ne_bytes(closure_word.to_ne_bytes()),
                    closure_semantic,
                )
                .map_err(|error| error.to_string())?
                .cast::<ManagedClosure>();
            let view = heap
                .closure_view(closure)
                .map_err(|error| error.to_string())?;
            assert_eq!(view.callable_id, target.export_id);
            assert_eq!(view.capture_words, &[40]);
            heap.prepare_closure_invocation(
                closure,
                &table,
                table.generation(),
                &[TvmBoundaryType::Int],
                &[2],
                &[TvmBoundaryType::Int],
            )
            .map_err(|error| error.to_string())
        })
        .expect("source captured closure invocation");
    let answer = runtime.with_dispatch(owner, |context, allocator, resolver| {
        invoke_dispatch(
            dispatch,
            context,
            allocator,
            resolver,
            invocation.target().callable_id(),
            invocation.words(),
        )
    });
    assert_eq!(answer.expect("source captured target"), 42);
    drop(library);
    fs::remove_dir_all(root).expect("remove source captured closure fixture");
}

#[test]
#[allow(unsafe_code)]
fn source_if_selects_distinct_executable_captured_closures() {
    let syntax = parse_module_as_syntax_output(
        "module branch_closure.\n\n\
         choose(forward: Bool, seed: Int): ((Int) -> Int) ->\n\
             if {\n\
                 forward -> ((value: Int) -> value + seed);\n\
                 true -> ((value: Int) -> seed - value)\n\
             }.\n",
    )
    .expect("parse branch closure source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("branch closure NativeIR");
    let functions = modules
        .iter()
        .flat_map(|module| &module.functions)
        .collect::<Vec<_>>();
    let maker = functions
        .iter()
        .find(|function| function.name == "choose")
        .expect("branch closure maker");
    let mut targets = functions
        .iter()
        .filter(|function| function.name.starts_with("$closure_choose_2_"))
        .copied()
        .collect::<Vec<_>>();
    targets.sort_by(|left, right| left.name.cmp(&right.name));
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[0].name, "$closure_choose_2_0");
    assert_eq!(targets[1].name, "$closure_choose_2_1");
    assert_ne!(targets[0].export_id, targets[1].export_id);
    assert!(targets
        .iter()
        .all(|target| target.callable_captures == vec![NativeType::Int]));
    let NativeType::ManagedRef(closure_semantic) = maker.return_type else {
        panic!("branch maker return is not a managed closure");
    };
    let NativeExpr::If { clauses } = &maker.body else {
        panic!("branch maker did not lower to native If: {:?}", maker.body);
    };
    assert_eq!(clauses.len(), 2);
    assert!(clauses.iter().all(|(_, body)| matches!(
        body,
        NativeExpr::MakeClosure { captures, .. }
            if captures == &vec![NativeExpr::Param(1)]
    )));

    let mut callables = functions
        .iter()
        .map(|function| TvmCallableDescriptor {
            id: function.export_id,
            parameters: function
                .params
                .iter()
                .skip(function.callable_captures.len())
                .copied()
                .map(NativeType::boundary_type)
                .collect(),
            results: vec![function.return_type.boundary_type()],
            captures: function
                .callable_captures
                .iter()
                .copied()
                .map(NativeType::boundary_type)
                .collect(),
        })
        .collect::<Vec<_>>();
    callables.sort_by_key(|callable| callable.id);
    let mut runtime = ManagedExecutionRuntime::with_executable_image_metadata(
        &[],
        &[],
        &[],
        [31; 32],
        &callables,
    )
    .expect("branch source image metadata");
    let table = runtime
        .closure_dispatch()
        .expect("branch source callable table")
        .clone();
    let object = emit_native_application_object("source_branch_closure", &modules)
        .expect("source branch closure object");
    let (library, root) = link_test_library("source-branch-closure", &object);
    // SAFETY: The freshly linked test image exports the frozen format-1 dispatch ABI.
    let dispatch: Symbol<'_, NativeDispatch> = unsafe {
        library
            .get(b"terlan_native_dispatch_v2")
            .expect("source branch dispatch symbol")
    };
    let dispatch = *dispatch;
    let owner = 101;
    for (forward, target, expected) in [(1, targets[0], 42), (0, targets[1], 38)] {
        let closure_word = runtime
            .with_dispatch(owner, |context, allocator, resolver| {
                invoke_dispatch(
                    dispatch,
                    context,
                    allocator,
                    resolver,
                    maker.export_id,
                    &[forward, 40],
                )
            })
            .expect("branch closure allocation");
        let invocation = runtime
            .with_public_materialization(owner, |heap, _| {
                let closure = heap
                    .validate_abi_reference(
                        u64::from_ne_bytes(closure_word.to_ne_bytes()),
                        closure_semantic,
                    )
                    .map_err(|error| error.to_string())?
                    .cast::<ManagedClosure>();
                let view = heap
                    .closure_view(closure)
                    .map_err(|error| error.to_string())?;
                assert_eq!(view.callable_id, target.export_id);
                assert_eq!(view.capture_words, &[40]);
                heap.prepare_closure_invocation(
                    closure,
                    &table,
                    table.generation(),
                    &[TvmBoundaryType::Int],
                    &[2],
                    &[TvmBoundaryType::Int],
                )
                .map_err(|error| error.to_string())
            })
            .expect("prepare selected branch closure");
        let answer = runtime.with_dispatch(owner, |context, allocator, resolver| {
            invoke_dispatch(
                dispatch,
                context,
                allocator,
                resolver,
                invocation.target().callable_id(),
                invocation.words(),
            )
        });
        assert_eq!(answer.expect("selected branch target"), expected);
    }
    drop(library);
    fs::remove_dir_all(root).expect("remove source branch closure fixture");
}

#[test]
fn source_named_function_value_rejects_signature_drift_before_codegen() {
    let syntax = parse_module_as_syntax_output(
        "module escaping_named_mismatch.\n\n\
         negate(value: Bool): Bool -> not value.\n\n\
         make(): ((Int) -> Int) -> negate.\n",
    )
    .expect("parse mismatched named function source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);

    assert_eq!(
        NativeModule::lower_application(&[&core]).unwrap_err(),
        "error[native_ir.function_value_abi]: `negate/1` does not match its declared closure signature"
    );
}

#[test]
fn public_function_value_result_uses_owned_closure_boundary() {
    let syntax = parse_module_as_syntax_output(
        "module escaping_named_public.\n\n\
         double(value: Int): Int -> value * 2.\n\n\
         pub make(): ((Int) -> Int) -> double.\n",
    )
    .expect("parse public named function source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);

    let modules = NativeModule::lower_application(&[&core]).expect("public closure result ABI");
    let maker = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "make")
        .expect("public closure maker");
    assert!(maker.public);
    assert!(matches!(maker.return_type, NativeType::ManagedRef(_)));
    assert!(matches!(maker.body, NativeExpr::MakeClosure { .. }));
}

#[allow(unsafe_code)]
fn invoke_dispatch(
    dispatch: NativeDispatch,
    context: *mut c_void,
    allocator: *const c_void,
    closure_resolver: *const c_void,
    export_id: u64,
    arguments: &[i64],
) -> Result<i64, i32> {
    let mut result = -1_i64;
    let mut transitions =
        [0_i64; crate::runtime::native_image::TVM_INDIRECT_TRANSITION_WORD_CAPACITY];
    let mut transition_len = 0_u64;
    let arguments_pointer = if arguments.is_empty() {
        std::ptr::null()
    } else {
        arguments.as_ptr()
    };
    // SAFETY: All pointers reference call-scoped storage and the admitted runtime callback.
    let status = unsafe {
        dispatch(
            context,
            allocator,
            closure_resolver,
            export_id,
            arguments_pointer,
            arguments.len() as u64,
            &mut result,
            transitions.as_mut_ptr(),
            transitions.len() as u64,
            &mut transition_len,
        )
    };
    if status == 0 && transition_len == 0 {
        Ok(result)
    } else {
        Err(status)
    }
}

#[allow(unsafe_code)]
fn link_test_library(label: &str, object: &[u8]) -> (Library, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "terlan-{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("test library directory");
    let object_path = root.join("closure.o");
    let library_path = root.join(if cfg!(target_os = "macos") {
        "closure.dylib"
    } else {
        "closure.so"
    });
    fs::write(&object_path, object).expect("generated closure object");
    let mut link = Command::new("cc");
    link.arg(if cfg!(target_os = "macos") {
        "-dynamiclib"
    } else {
        "-shared"
    });
    let output = link
        .arg(&object_path)
        .arg("-o")
        .arg(&library_path)
        .output()
        .expect("link generated closure library");
    assert!(
        output.status.success(),
        "generated closure library failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    // SAFETY: The path names the library just linked from compiler-owned object bytes.
    let library = unsafe { Library::new(&library_path) }.expect("load generated closure library");
    (library, root)
}

const HARNESS: &str = r#"
use std::ffi::c_void;

type Allocator = unsafe extern "C" fn(
    *mut c_void,
    *const u8,
    u64,
    *const i64,
    u64,
    *mut u64,
) -> i32;

unsafe extern "C" {
    fn terlan_native_dispatch_v2(
        context: *mut c_void,
        allocator: *const c_void,
        closure_resolver: *const c_void,
        export_id: u64,
        arguments: *const i64,
        arity: u64,
        result: *mut i64,
        transitions: *mut i64,
        transition_capacity: u64,
        transition_len: *mut u64,
    ) -> i32;
}

#[derive(Default)]
struct Capture {
    called: bool,
    fields: Vec<i64>,
}

unsafe extern "C" fn allocate(
    context: *mut c_void,
    layout: *const u8,
    layout_len: u64,
    fields: *const i64,
    field_count: u64,
    result: *mut u64,
) -> i32 {
    let capture = unsafe { &mut *context.cast::<Capture>() };
    let layout = unsafe { std::slice::from_raw_parts(layout, layout_len as usize) };
    assert_eq!(&layout[..4], b"TVMA");
    capture.fields = unsafe {
        std::slice::from_raw_parts(fields, field_count as usize).to_vec()
    };
    capture.called = true;
    unsafe { *result = 0x5a5a_1234 };
    0
}

fn main() {
    let arguments = [37_i64, 1_i64];
    let mut result = -1_i64;
    let mut transitions = [0_i64; 1];
    let mut transition_len = 99_u64;
    let missing = unsafe {
        terlan_native_dispatch_v2(
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            91,
            arguments.as_ptr(),
            arguments.len() as u64,
            &mut result,
            transitions.as_mut_ptr(),
            transitions.len() as u64,
            &mut transition_len,
        )
    };
    assert_eq!(missing, 20);
    assert_eq!(result, -1);

    let mut capture = Capture::default();
    let callback_failure = unsafe {
        terlan_native_dispatch_v2(
            (&mut capture as *mut Capture).cast(),
            fail as Allocator as *const c_void,
            std::ptr::null(),
            91,
            arguments.as_ptr(),
            arguments.len() as u64,
            &mut result,
            transitions.as_mut_ptr(),
            transitions.len() as u64,
            &mut transition_len,
        )
    };
    assert_eq!(callback_failure, 77);

    let invalid = unsafe {
        terlan_native_dispatch_v2(
            (&mut capture as *mut Capture).cast(),
            zero_reference as Allocator as *const c_void,
            std::ptr::null(),
            91,
            arguments.as_ptr(),
            arguments.len() as u64,
            &mut result,
            transitions.as_mut_ptr(),
            transitions.len() as u64,
            &mut transition_len,
        )
    };
    assert_eq!(invalid, 21);

    let status = unsafe {
        terlan_native_dispatch_v2(
            (&mut capture as *mut Capture).cast(),
            allocate as Allocator as *const c_void,
            std::ptr::null(),
            91,
            arguments.as_ptr(),
            arguments.len() as u64,
            &mut result,
            transitions.as_mut_ptr(),
            transitions.len() as u64,
            &mut transition_len,
        )
    };
    assert_eq!(status, 0);
    assert!(capture.called);
    assert_eq!(capture.fields, arguments);
    assert_eq!(result, 0x5a5a_1234);
    assert_eq!(transition_len, 0);
}

unsafe extern "C" fn zero_reference(
    _context: *mut c_void,
    _layout: *const u8,
    _layout_len: u64,
    _fields: *const i64,
    _field_count: u64,
    result: *mut u64,
) -> i32 {
    unsafe { *result = 0 };
    0
}

unsafe extern "C" fn fail(
    _context: *mut c_void,
    _layout: *const u8,
    _layout_len: u64,
    _fields: *const i64,
    _field_count: u64,
    _result: *mut u64,
) -> i32 {
    77
}
"#;
