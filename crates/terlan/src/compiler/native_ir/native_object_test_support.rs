//! Shared linked-object execution support for NativeIR tests.

use std::ffi::c_void;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use libloading::{Library, Symbol};

use crate::runtime::native_image::dispatch_lookup::{tvm_dispatch_lookup_v1, TvmDispatchLookup};
use crate::runtime::native_image::managed::{
    decode_aggregate_layout, decode_collection_layout, ManagedExecutionRuntime,
    PendingManagedCaptures,
};
use crate::runtime::native_image::{
    TvmBoundaryType, TvmCallableDescriptor, TvmManagedCollectionDescriptor,
    TvmManagedLayoutDescriptor,
};

use super::{status, NativeModule, NativeType};

type NativeDispatch = unsafe extern "C" fn(
    *mut c_void,
    *const c_void,
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

struct LinkedCompletionFrame {
    continuation_id: u64,
    capture_types: Vec<TvmBoundaryType>,
    scalar_captures: Vec<i64>,
    managed: Option<PendingManagedCaptures>,
}

/// Links and executes one native object export, asserting its scalar result.
pub(super) fn assert_native_object_result(
    label: &str,
    object: &[u8],
    export_id: u64,
    arguments: &[i64],
    expected: i64,
) {
    assert_native_object_result_with_stack_policy(
        label, object, export_id, arguments, expected, false, 0,
    );
}

/// Links and executes one native export on a deliberately small native stack.
pub(super) fn assert_native_object_result_on_small_stack(
    label: &str,
    object: &[u8],
    export_id: u64,
    arguments: &[i64],
    expected: i64,
) {
    assert_native_object_result_with_stack_policy(
        label, object, export_id, arguments, expected, true, 1,
    );
}

fn assert_native_object_result_with_stack_policy(
    label: &str,
    object: &[u8],
    export_id: u64,
    arguments: &[i64],
    expected: i64,
    small_stack: bool,
    minimum_yields: usize,
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
    let harness = format!("{DISPATCH_LOOKUP_HARNESS}{NATIVE_OBJECT_HARNESS}")
        .replace("$EXPORT_ID", &export_id.to_string())
        .replace("$ARGUMENTS", &arguments)
        .replace("$EXPECTED", &expected.to_string())
        .replace("$MINIMUM_YIELDS", &minimum_yields.to_string());
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
    let run = if small_stack {
        Command::new("bash")
            .args(["-c", "ulimit -s 128; exec \"$1\"", "terlan-native-object"])
            .arg(&executable_path)
            .output()
            .expect("run native object harness on small stack")
    } else {
        Command::new(&executable_path)
            .output()
            .expect("run native object harness")
    };
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
    let harness =
        format!("{DISPATCH_LOOKUP_HARNESS}{NATIVE_OBJECT_BATCH_HARNESS}").replace("$CASES", &cases);
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
            .get(b"terlan_native_dispatch_v3")
            .expect("managed dispatch symbol")
    };
    for (index, invocation) in invocations.iter().enumerate() {
        let mut result = -1;
        let mut transitions = [0_i64; 128];
        let mut transition_len;
        let mut export_id = invocation.export_id;
        let mut arguments = invocation.arguments.clone();
        let mut reduction_yields = 0_usize;
        let mut completions = Vec::<LinkedCompletionFrame>::new();
        let status = loop {
            transition_len = 0;
            let current = runtime.with_dispatch(8_001, |context, allocator, resolver| unsafe {
                dispatch(
                    context,
                    allocator,
                    resolver,
                    tvm_dispatch_lookup_v1 as TvmDispatchLookup as *const c_void,
                    export_id,
                    if arguments.is_empty() {
                        std::ptr::null()
                    } else {
                        arguments.as_ptr()
                    },
                    arguments.len() as u64,
                    &mut result,
                    transitions.as_mut_ptr(),
                    transitions.len() as u64,
                    &mut transition_len,
                )
            });
            if current == status::OK && !completions.is_empty() {
                let completion = completions.remove(0);
                let mut restored = runtime
                    .restore_continuation_captures(
                        8_001,
                        completion.continuation_id,
                        &completion.capture_types,
                        &completion.scalar_captures,
                        completion.managed,
                    )
                    .expect("restore linked-object completion captures");
                restored.push(result);
                export_id = completion.continuation_id;
                arguments = restored;
                continue;
            }
            if current != status::YIELD || invocation.expected_status != status::OK {
                break current;
            }
            reduction_yields = reduction_yields.saturating_add(1);
            assert!(
                reduction_yields <= 100_000,
                "managed invocation {index} exceeded the reduction-yield bound"
            );
            let continuation_id = result as u64;
            let capture_types = linked_entry_parameters(modules, continuation_id)
                .unwrap_or_else(|| panic!("missing linked continuation {continuation_id}"));
            let values = &transitions[..transition_len as usize];
            assert!(
                values.len() >= capture_types.len(),
                "linked continuation {continuation_id} exposes {} values for {} captures",
                values.len(),
                capture_types.len()
            );
            let (capture_values, trailers) = values.split_at(capture_types.len());
            let mut appended = decode_linked_completion_frames(trailers)
                .expect("decode linked-object completion frames")
                .into_iter()
                .map(|(completion_id, raw_captures)| {
                    let parameters = linked_entry_parameters(modules, completion_id)
                        .unwrap_or_else(|| panic!("missing linked completion {completion_id}"));
                    let capture_types = parameters
                        .get(..parameters.len().saturating_sub(1))
                        .expect("linked completion has a result parameter")
                        .to_vec();
                    let (scalar_captures, managed) = runtime
                        .park_continuation_captures(
                            8_001,
                            completion_id,
                            &capture_types,
                            &raw_captures,
                        )
                        .expect("park linked-object completion captures");
                    LinkedCompletionFrame {
                        continuation_id: completion_id,
                        capture_types,
                        scalar_captures,
                        managed,
                    }
                })
                .collect::<Vec<_>>();
            appended.append(&mut completions);
            completions = appended;
            let (scalar_captures, mut parked) = runtime
                .park_continuation_captures(8_001, continuation_id, &capture_types, capture_values)
                .expect("park linked-object continuation captures");
            let mut roots = parked
                .iter_mut()
                .chain(
                    completions
                        .iter_mut()
                        .filter_map(|completion| completion.managed.as_mut()),
                )
                .collect::<Vec<_>>();
            runtime
                .collect_owner_with_continuation_stack(8_001, &mut roots)
                .expect("collect linked-object continuation roots");
            arguments = runtime
                .restore_continuation_captures(
                    8_001,
                    continuation_id,
                    &capture_types,
                    &scalar_captures,
                    parked,
                )
                .expect("restore linked-object continuation captures");
            export_id = continuation_id;
        };
        let target = modules
            .iter()
            .flat_map(|module| &module.functions)
            .find(|function| function.export_id == export_id)
            .map(|function| {
                format!(
                    "{}.{}/{} native-parameters={}",
                    function.source_module,
                    function.name,
                    function.arity,
                    function.params.len()
                )
            })
            .or_else(|| {
                modules
                    .iter()
                    .flat_map(|module| &module.continuations)
                    .find(|continuation| continuation.id == export_id)
                    .map(|continuation| {
                        format!(
                            "{}.{} generated-continuation parameters={}",
                            continuation.source_module,
                            continuation.source_function,
                            continuation.params.len()
                        )
                    })
            })
            .unwrap_or_else(|| "unknown generated entry".to_string());
        assert_eq!(
            status,
            invocation.expected_status,
            "managed status for invocation {index} after {reduction_yields} reduction yields at export {export_id} ({target}) with {} arguments; allocation error: {:?}",
            arguments.len(),
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

fn linked_entry_parameters(
    modules: &[NativeModule],
    entry_id: u64,
) -> Option<Vec<TvmBoundaryType>> {
    modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.export_id == entry_id)
        .map(|function| {
            function
                .params
                .iter()
                .copied()
                .map(NativeType::boundary_type)
                .collect()
        })
        .or_else(|| {
            modules
                .iter()
                .flat_map(|module| &module.continuations)
                .find(|continuation| continuation.id == entry_id)
                .map(|continuation| {
                    continuation
                        .params
                        .iter()
                        .copied()
                        .map(NativeType::boundary_type)
                        .collect()
                })
        })
}

fn decode_linked_completion_frames(mut words: &[i64]) -> Result<Vec<(u64, Vec<i64>)>, String> {
    let mut reversed = Vec::new();
    while !words.is_empty() {
        let capture_count = usize::try_from(*words.last().ok_or_else(|| {
            "error[native_test.completion_frame]: missing capture count".to_string()
        })?)
        .map_err(|_| "error[native_test.completion_frame]: negative capture count".to_string())?;
        words = &words[..words.len() - 1];
        let completion_word = *words.last().ok_or_else(|| {
            "error[native_test.completion_frame]: missing completion identity".to_string()
        })?;
        words = &words[..words.len() - 1];
        if words.len() < capture_count {
            return Err("error[native_test.completion_frame]: truncated captures".to_string());
        }
        let capture_start = words.len() - capture_count;
        reversed.push((
            u64::from_ne_bytes(completion_word.to_ne_bytes()),
            words[capture_start..].to_vec(),
        ));
        words = &words[..capture_start];
    }
    reversed.reverse();
    Ok(reversed)
}
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
const DISPATCH_LOOKUP_HARNESS: &str = r#"
unsafe extern "C" fn dispatch_lookup(
    index: *const u32,
    records: *const u8,
    mask: u64,
    export_id: u64,
) -> *const std::ffi::c_void {
    let mut slot = export_id & mask;
    for _ in 0..=mask {
        let tag = unsafe { index.add(slot as usize).read() };
        if tag == 0 {
            return std::ptr::null();
        }
        let record = unsafe { records.add((tag as usize - 1) * 24) };
        if unsafe { record.cast::<u64>().read() } == export_id {
            return record.cast();
        }
        slot = slot.wrapping_add(1) & mask;
    }
    std::ptr::null()
}

"#;

/// Adds the VM-owned lookup callback fixture to a standalone linked harness.
pub(super) fn with_dispatch_lookup_harness(source: &str) -> String {
    format!("{DISPATCH_LOOKUP_HARNESS}{source}")
}

/// Generic linked probe for one scalar native export.
const NATIVE_OBJECT_HARNESS: &str = r#"
use std::ffi::c_void;

unsafe extern "C" {
    fn terlan_native_dispatch_v3(
        context: *mut c_void,
        allocator: *const c_void,
        closure_resolver: *const c_void,
        dispatch_lookup: *const c_void,
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
    let mut arguments = vec![$ARGUMENTS];
    let mut entry = $EXPORT_ID;
    let mut result = -1_i64;
    let mut transitions = [0_i64; 128];
    let mut yields = 0_usize;
    loop {
        let argument_pointer = if arguments.is_empty() {
            std::ptr::null()
        } else {
            arguments.as_ptr()
        };
        let mut transition_len = 0_u64;
        let status = unsafe {
            terlan_native_dispatch_v3(
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                dispatch_lookup as *const c_void,
                entry,
                argument_pointer,
                arguments.len() as u64,
                &mut result,
                transitions.as_mut_ptr(),
                transitions.len() as u64,
                &mut transition_len,
            )
        };
        if status == 6 {
            yields += 1;
            entry = result as u64;
            arguments.clear();
            arguments.extend_from_slice(&transitions[..transition_len as usize]);
            continue;
        }
        assert_eq!(status, 0);
        assert_eq!(transition_len, 0);
        break;
    }
    assert_eq!(result, $EXPECTED);
    assert!(yields >= $MINIMUM_YIELDS, "observed {yields} reduction yields");
}
"#;

/// Generic linked probe for a batch of scalar native exports.
const NATIVE_OBJECT_BATCH_HARNESS: &str = r#"
use std::ffi::c_void;

unsafe extern "C" {
    fn terlan_native_dispatch_v3(
        context: *mut c_void,
        allocator: *const c_void,
        closure_resolver: *const c_void,
        dispatch_lookup: *const c_void,
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
        let mut transitions = [0_i64; 128];
        let mut transition_len = 99_u64;
        let status = unsafe {
            terlan_native_dispatch_v3(
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                dispatch_lookup as *const c_void,
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
