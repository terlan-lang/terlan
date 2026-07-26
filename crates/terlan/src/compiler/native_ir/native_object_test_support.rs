//! Shared linked-object execution support for NativeIR tests.

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

use super::{NativeModule, NativeType};

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

/// One linked native-object call and its exact scalar ABI outcome.
pub(super) struct NativeObjectInvocation {
    pub(super) export_id: u64,
    pub(super) arguments: Vec<i64>,
    pub(super) expected_status: i32,
    pub(super) expected_result: Option<i64>,
}

/// Links and executes one native object export, asserting its scalar result.
pub(super) fn assert_native_object_result(
    label: &str,
    object: &[u8],
    export_id: u64,
    arguments: &[i64],
    expected: i64,
) {
    let root = std::env::temp_dir().join(format!(
        "terlan-{label}-object-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create native object test root");
    let object_path = root.join("module.o");
    let harness_path = root.join("harness.rs");
    let executable_path = root.join("harness");
    fs::write(&object_path, object).expect("write native test object");
    let arguments = arguments
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    let harness = NATIVE_OBJECT_HARNESS
        .replace("$EXPORT_ID", &export_id.to_string())
        .replace("$ARGUMENTS", &arguments)
        .replace("$EXPECTED", &expected.to_string());
    fs::write(&harness_path, harness).expect("write native object harness");

    let compile = Command::new("rustc")
        .arg("--edition=2021")
        .arg(&harness_path)
        .arg("-C")
        .arg(format!("link-arg={}", object_path.display()))
        .arg("-o")
        .arg(&executable_path)
        .output()
        .expect("compile native object harness");
    assert!(
        compile.status.success(),
        "native object harness failed:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new(&executable_path)
        .output()
        .expect("run native object harness");
    assert!(
        run.status.success(),
        "native object execution failed:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    fs::remove_dir_all(root).expect("remove native object test root");
}

/// Links once and executes a batch of native exports and failure paths.
pub(super) fn assert_native_object_invocations(
    label: &str,
    object: &[u8],
    invocations: &[NativeObjectInvocation],
) {
    let root = std::env::temp_dir().join(format!(
        "terlan-{label}-object-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create native object test root");
    let object_path = root.join("module.o");
    let harness_path = root.join("harness.rs");
    let executable_path = root.join("harness");
    fs::write(&object_path, object).expect("write native test object");
    let cases = invocations
        .iter()
        .map(|invocation| {
            let arguments = invocation
                .arguments
                .iter()
                .map(i64::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            let expected = invocation
                .expected_result
                .map_or_else(|| "None".to_owned(), |value| format!("Some({value})"));
            format!(
                "({}, &[{}], {}, {})",
                invocation.export_id, arguments, invocation.expected_status, expected
            )
        })
        .collect::<Vec<_>>()
        .join(",\n        ");
    let harness = NATIVE_OBJECT_BATCH_HARNESS.replace("$CASES", &cases);
    fs::write(&harness_path, harness).expect("write native object harness");

    let compile = Command::new("rustc")
        .arg("--edition=2021")
        .arg(&harness_path)
        .arg("-C")
        .arg(format!("link-arg={}", object_path.display()))
        .arg("-o")
        .arg(&executable_path)
        .output()
        .expect("compile native object harness");
    assert!(
        compile.status.success(),
        "native object harness failed:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new(&executable_path)
        .output()
        .expect("run native object harness");
    assert!(
        run.status.success(),
        "native object execution failed:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    fs::remove_dir_all(root).expect("remove native object test root");
}

/// Links and executes native exports with the real actor-owned managed ABI.
#[allow(unsafe_code)]
pub(super) fn assert_managed_native_object_invocations(
    label: &str,
    modules: &[NativeModule],
    object: &[u8],
    invocations: &[NativeObjectInvocation],
) {
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
    let mut runtime = ManagedExecutionRuntime::with_executable_image_metadata(
        &layouts,
        &collections,
        &atoms,
        [91; 32],
        &callables,
    )
    .expect("managed native-object runtime");
    let (library, root) = link_managed_library(label, object);
    let dispatch: Symbol<'_, NativeDispatch> = unsafe {
        library
            .get(b"terlan_native_dispatch_v2")
            .expect("managed dispatch symbol")
    };
    for (index, invocation) in invocations.iter().enumerate() {
        let mut result = -1;
        let mut transitions = [0_i64; 128];
        let mut transition_len = 0_u64;
        let status = runtime.with_dispatch(8_001, |context, allocator, resolver| unsafe {
            dispatch(
                context,
                allocator,
                resolver,
                invocation.export_id,
                if invocation.arguments.is_empty() {
                    std::ptr::null()
                } else {
                    invocation.arguments.as_ptr()
                },
                invocation.arguments.len() as u64,
                &mut result,
                transitions.as_mut_ptr(),
                transitions.len() as u64,
                &mut transition_len,
            )
        });
        assert_eq!(
            status,
            invocation.expected_status,
            "managed status for invocation {index}; allocation error: {:?}",
            runtime.take_allocation_error()
        );
        assert_eq!(
            invocation.expected_result,
            (status == 0).then_some(result),
            "managed result for invocation {index}"
        );
        assert_eq!(
            transition_len, 0,
            "unexpected transition for invocation {index}"
        );
    }
    drop(library);
    fs::remove_dir_all(root).expect("remove managed object test root");
}

#[allow(unsafe_code)]
fn link_managed_library(label: &str, object: &[u8]) -> (Library, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "terlan-{label}-managed-object-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create managed object test root");
    let object_path = root.join("image.o");
    let library_path = root.join("image.so");
    fs::write(&object_path, object).expect("write managed object");
    let output = Command::new("cc")
        .arg("-shared")
        .arg("-o")
        .arg(&library_path)
        .arg(&object_path)
        .output()
        .expect("link managed object");
    assert!(
        output.status.success(),
        "managed object link failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let library = unsafe { Library::new(&library_path).expect("load managed object") };
    (library, root)
}

/// Generic linked probe for one scalar native export.
const NATIVE_OBJECT_HARNESS: &str = r#"
use std::ffi::c_void;

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

fn main() {
    let arguments = [$ARGUMENTS];
    let argument_pointer = if arguments.is_empty() {
        std::ptr::null()
    } else {
        arguments.as_ptr()
    };
    let mut result = -1_i64;
    let mut transitions = [0_i64; 1];
    let mut transition_len = 99_u64;
    let status = unsafe {
        terlan_native_dispatch_v2(
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            $EXPORT_ID,
            argument_pointer,
            arguments.len() as u64,
            &mut result,
            transitions.as_mut_ptr(),
            transitions.len() as u64,
            &mut transition_len,
        )
    };
    assert_eq!(status, 0);
    assert_eq!(result, $EXPECTED);
    assert_eq!(transition_len, 0);
}
"#;

/// Generic linked probe for a batch of scalar native exports.
const NATIVE_OBJECT_BATCH_HARNESS: &str = r#"
use std::ffi::c_void;

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

fn main() {
    let cases: &[(u64, &[i64], i32, Option<i64>)] = &[
        $CASES
    ];
    for (index, (export_id, arguments, expected_status, expected_result)) in
        cases.iter().enumerate()
    {
        let argument_pointer = if arguments.is_empty() {
            std::ptr::null()
        } else {
            arguments.as_ptr()
        };
        let mut result = i64::MIN;
        let mut transitions = [0_i64; 1];
        let mut transition_len = 99_u64;
        let status = unsafe {
            terlan_native_dispatch_v2(
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                *export_id,
                argument_pointer,
                arguments.len() as u64,
                &mut result,
                transitions.as_mut_ptr(),
                transitions.len() as u64,
                &mut transition_len,
            )
        };
        assert_eq!(status, *expected_status, "status for invocation {index}");
        if let Some(expected_result) = expected_result {
            assert_eq!(result, *expected_result, "result for invocation {index}");
        }
        assert_eq!(transition_len, 0, "transitions for invocation {index}");
    }
}
"#;
