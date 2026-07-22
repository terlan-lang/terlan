//! Closed-world executable conformance for the AOT-3 application surface.

#![allow(unsafe_code)]

use std::ffi::c_void;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use libloading::{Library, Symbol};

use crate::runtime::native_image::managed::{
    decode_aggregate_layout, decode_collection_layout, ManagedExecutionRuntime,
};
use crate::runtime::native_image::{
    TvmCallableDescriptor, TvmManagedCollectionDescriptor, TvmManagedLayoutDescriptor,
};
use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{
    lower_syntax_module_output_to_core, CoreExpr, CoreImport, CoreImportKind, CoreModule,
};

use super::{emit_native_application_object, NativeModule, NativeType};

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

fn core(source: &str) -> CoreModule {
    let syntax = parse_module_as_syntax_output(source).expect("parse AOT-3 source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    lower_syntax_module_output_to_core(&syntax, &resolved)
}

fn invoke(
    runtime: &mut ManagedExecutionRuntime,
    owner: u64,
    dispatch: NativeDispatch,
    export: u64,
    args: &[i64],
) -> (i32, i64, Vec<i64>) {
    let mut result = -1;
    let mut transition = [0_i64; 128];
    let mut transition_len = 0_u64;
    let status = runtime.with_dispatch(owner, |context, allocator, resolver| unsafe {
        dispatch(
            context,
            allocator,
            resolver,
            export,
            if args.is_empty() {
                std::ptr::null()
            } else {
                args.as_ptr()
            },
            args.len() as u64,
            &mut result,
            transition.as_mut_ptr(),
            transition.len() as u64,
            &mut transition_len,
        )
    });
    (
        status,
        result,
        transition[..transition_len as usize].to_vec(),
    )
}

#[test]
fn one_image_executes_closed_application_features_and_rejects_unbounded_peers() {
    let provider =
        core("module aot3.Provider.\n\npub imported_inc(value: Int): Int -> value + 1.\n");
    let mut application = core(
        "module aot3.Application.\n\n\
         import std.vm.Process.\n\n\
         identity[T](value: T): T -> value.\n\n\
         factorial(value: Int, acc: Int): Int ->\n\
             if { value == 0 -> acc; true -> factorial(value - 1, acc * value) }.\n\n\
         make(offset: Int): ((Int) -> Int) -> (value) -> value + offset.\n\n\
         pub recursion(value: Int): Int -> factorial(value, 1).\n\n\
         pub generic(value: Int): Int -> identity(value).\n\n\
         pub imported(value: Int): Int -> value.\n\n\
         pub remote(value: Int): Int -> value.\n\n\
         pub apply(value: Int, callback: (Int) -> Int): Int -> callback(value).\n\n\
         pub pair(value: Int): {Int, Int} -> {value, 2}.\n\n\
         pub structured(value: {Int, Int}): Int ->\n\
             case value { {left, right} -> left + right }.\n\n\
         pub recovered(divisor: Int): Int ->\n\
             try 84 div divisor { value -> value catch _reason -> 42 after 0 -> 7 }.\n\n\
         pub mixed_suspend(value: Int): Int ->\n\
             let _parked = Process.yield_now();\n\
             value + 1.\n",
    );
    application.imports.push(CoreImport {
        module: provider.module.clone(),
        kind: CoreImportKind::Module,
    });
    let imported = application
        .functions
        .iter_mut()
        .find(|function| function.name == "imported")
        .and_then(|function| function.clauses.first_mut())
        .and_then(|clause| clause.body.core_expr.as_mut())
        .expect("imported body");
    *imported = CoreExpr::Call {
        function: "imported_inc".to_string(),
        args: vec![CoreExpr::Var("value".to_string())],
    };
    let remote = application
        .functions
        .iter_mut()
        .find(|function| function.name == "remote")
        .and_then(|function| function.clauses.first_mut())
        .and_then(|clause| clause.body.core_expr.as_mut())
        .expect("remote body");
    *remote = CoreExpr::RemoteCall {
        module: provider.module.clone(),
        function: "imported_inc".to_string(),
        args: vec![CoreExpr::Var("value".to_string())],
    };

    let modules =
        NativeModule::lower_application(&[&application, &provider]).expect("closed AOT-3 image");
    let export = |name: &str| {
        modules
            .iter()
            .flat_map(|module| &module.functions)
            .find(|function| function.name == name)
            .unwrap_or_else(|| panic!("missing {name}"))
            .export_id
    };
    let callables = modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter(|function| !function.callable_captures.is_empty())
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
    let layouts = modules
        .iter()
        .flat_map(|module| module.managed_layouts.iter().cloned())
        .map(|encoded_layout| {
            let descriptor = decode_aggregate_layout(&encoded_layout).expect("aggregate layout");
            TvmManagedLayoutDescriptor {
                semantic_id: descriptor.managed().semantic_id().bytes(),
                encoded_layout: encoded_layout.to_vec(),
            }
        })
        .collect::<Vec<_>>();
    let collections = modules
        .iter()
        .flat_map(|module| module.managed_collections.iter().cloned())
        .map(|encoded_layout| {
            let descriptor = decode_collection_layout(&encoded_layout).expect("collection layout");
            TvmManagedCollectionDescriptor {
                semantic_id: descriptor.semantic_id().bytes(),
                encoded_layout: encoded_layout.to_vec(),
            }
        })
        .collect::<Vec<_>>();
    let atoms = modules
        .iter()
        .flat_map(|module| module.atoms.iter().cloned())
        .collect::<Vec<_>>();
    let mut runtime = ManagedExecutionRuntime::with_executable_image_metadata(
        &layouts,
        &collections,
        &atoms,
        [73; 32],
        &callables,
    )
    .expect("managed conformance runtime");
    let object = emit_native_application_object("aot3_conformance", &modules)
        .expect("AOT-3 conformance object");
    let (library, root) = link_library(&object);
    let dispatch: Symbol<'_, NativeDispatch> = unsafe {
        library
            .get(b"terlan_native_dispatch_v2")
            .expect("dispatch symbol")
    };
    let dispatch = *dispatch;
    let owner = 7001;

    assert_eq!(
        invoke(&mut runtime, owner, dispatch, export("recursion"), &[5]),
        (0, 120, vec![])
    );
    assert_eq!(
        invoke(&mut runtime, owner, dispatch, export("generic"), &[17]),
        (0, 17, vec![])
    );
    assert_eq!(
        invoke(&mut runtime, owner, dispatch, export("imported"), &[8]),
        (0, 9, vec![])
    );
    assert_eq!(
        invoke(&mut runtime, owner, dispatch, export("remote"), &[9]),
        (0, 10, vec![])
    );
    let made = invoke(&mut runtime, owner, dispatch, export("make"), &[4]);
    assert_eq!(made.0, 0);
    assert_eq!(
        invoke(&mut runtime, owner, dispatch, export("apply"), &[6, made.1]),
        (0, 10, vec![])
    );
    let pair = invoke(&mut runtime, owner, dispatch, export("pair"), &[5]);
    assert_eq!(pair.0, 0);
    assert_eq!(
        invoke(
            &mut runtime,
            owner,
            dispatch,
            export("structured"),
            &[pair.1]
        ),
        (0, 7, vec![])
    );
    assert_eq!(
        invoke(&mut runtime, owner, dispatch, export("recovered"), &[0]),
        (0, 42, vec![])
    );

    let (status, continuation, captures) = invoke(
        &mut runtime,
        owner,
        dispatch,
        export("mixed_suspend"),
        &[40],
    );
    assert_eq!(status, super::status::YIELD);
    let resumed = invoke(
        &mut runtime,
        owner,
        dispatch,
        continuation as u64,
        &captures,
    );
    assert_eq!(resumed, (0, 41, vec![]));

    drop(library);
    fs::remove_dir_all(root).expect("remove conformance library");

    let generic_export =
        core("module aot3.GenericExport.\n\npub identity[T](value: T): T -> value.\n");
    assert!(NativeModule::lower_application(&[&generic_export])
        .expect_err("generic export rejection")
        .starts_with("error[native_ir.generic_export]"));
}

fn link_library(object: &[u8]) -> (Library, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "terlan-aot3-conformance-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create conformance root");
    let object_path = root.join("image.o");
    let library_path = root.join("image.so");
    fs::write(&object_path, object).expect("write conformance object");
    let output = Command::new("cc")
        .arg("-shared")
        .arg("-o")
        .arg(&library_path)
        .arg(&object_path)
        .output()
        .expect("link conformance library");
    assert!(
        output.status.success(),
        "link failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let library = unsafe { Library::new(&library_path).expect("load conformance library") };
    (library, root)
}
