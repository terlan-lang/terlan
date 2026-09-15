//! Native executables used by tail-call code generation tests.

#[cfg(unix)]
pub(super) const DEEP_TAIL_LOOP_HARNESS: &str = r#"
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
    let arguments = [1_000_000_i64, 0_i64];
    let mut result = -1_i64;
    let mut transitions = [0_i64; 1];
    let mut transition_len = 99_u64;
    let status = unsafe {
        terlan_native_dispatch_v3(
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            dispatch_lookup as *const c_void,
            7,
            arguments.as_ptr(),
            arguments.len() as u64,
            &mut result,
            transitions.as_mut_ptr(),
            transitions.len() as u64,
            &mut transition_len,
        )
    };
    assert_eq!(status, 0);
    assert_eq!(result, 1_000_000);
    assert_eq!(transition_len, 0);
}
"#;

#[cfg(unix)]
pub(super) const SUSPENDING_TAIL_LOOP_HARNESS: &str = r#"
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
    let arguments = [1_000_000_i64, 0_i64];
    let mut result = -1_i64;
    let mut transitions = [0_i64; 1];
    let mut transition_len = 99_u64;
    let status = unsafe {
        terlan_native_dispatch_v3(
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            dispatch_lookup as *const c_void,
            7,
            arguments.as_ptr(),
            arguments.len() as u64,
            &mut result,
            transitions.as_mut_ptr(),
            transitions.len() as u64,
            &mut transition_len,
        )
    };
    assert_eq!(status, 6);
    assert_eq!(result, 91);
    assert_eq!(transition_len, 1);
    assert_eq!(transitions[0], 1_000_000);
}
"#;

#[cfg(unix)]
pub(super) const HETEROGENEOUS_TAIL_LOOP_HARNESS: &str = r#"
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
    let arguments = [1_000_000_i64];
    let mut result = -1_i64;
    let mut transitions = [0_i64; 1];
    let mut transition_len = 99_u64;
    let status = unsafe {
        terlan_native_dispatch_v3(
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            dispatch_lookup as *const c_void,
            7,
            arguments.as_ptr(),
            arguments.len() as u64,
            &mut result,
            transitions.as_mut_ptr(),
            transitions.len() as u64,
            &mut transition_len,
        )
    };
    assert_eq!(status, 0);
    assert_eq!(result, 42);
    assert_eq!(transition_len, 0);
}
"#;

#[cfg(unix)]
pub(super) const MANAGED_TAIL_LOOP_HARNESS: &str = r#"
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
    const MANAGED_TOKEN: i64 = 0x5a5a_1234;
    let arguments = [1_000_000_i64, MANAGED_TOKEN];
    let mut result = -1_i64;
    let mut transitions = [0_i64; 1];
    let mut transition_len = 99_u64;
    let status = unsafe {
        terlan_native_dispatch_v3(
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            dispatch_lookup as *const c_void,
            7,
            arguments.as_ptr(),
            arguments.len() as u64,
            &mut result,
            transitions.as_mut_ptr(),
            transitions.len() as u64,
            &mut transition_len,
        )
    };
    assert_eq!(status, 0);
    assert_eq!(result, MANAGED_TOKEN);
    assert_eq!(transition_len, 0);
}
"#;

#[cfg(unix)]
pub(super) const MANAGED_PARALLEL_TAIL_LOOP_HARNESS: &str = r#"
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
    const LEFT: i64 = 0x1111_2222;
    const RIGHT: i64 = 0x3333_4444;
    let arguments = [1_000_001_i64, LEFT, RIGHT];
    let mut result = -1_i64;
    let mut transitions = [0_i64; 1];
    let mut transition_len = 99_u64;
    let status = unsafe {
        terlan_native_dispatch_v3(
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            dispatch_lookup as *const c_void,
            7,
            arguments.as_ptr(),
            arguments.len() as u64,
            &mut result,
            transitions.as_mut_ptr(),
            transitions.len() as u64,
            &mut transition_len,
        )
    };
    assert_eq!(status, 0);
    assert_eq!(result, LEFT);
    assert_eq!(transition_len, 0);
}
"#;

#[cfg(unix)]
pub(super) const FAILING_TAIL_LOOP_HARNESS: &str = r#"
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
    let arguments = [1_000_000_i64, 0_i64];
    let mut result = -1_i64;
    let mut transitions = [0_i64; 1];
    let mut transition_len = 99_u64;
    let status = unsafe {
        terlan_native_dispatch_v3(
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            dispatch_lookup as *const c_void,
            7,
            arguments.as_ptr(),
            arguments.len() as u64,
            &mut result,
            transitions.as_mut_ptr(),
            transitions.len() as u64,
            &mut transition_len,
        )
    };
    assert_eq!(status, 4);
    assert_eq!(transition_len, 0);
}
"#;

#[cfg(unix)]
pub(super) const MANAGED_AGGREGATE_TAIL_LOOP_HARNESS: &str = r#"
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

#[derive(Default)]
struct Capture {
    calls: usize,
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
    capture.calls += 1;
    capture.fields =
        unsafe { std::slice::from_raw_parts(fields, field_count as usize).to_vec() };
    unsafe { *result = 0x5a5a_1234 };
    0
}

fn main() {
    let arguments = [1_000_000_i64, 42_i64];
    let mut capture = Capture::default();
    let mut result = -1_i64;
    let mut transitions = [0_i64; 1];
    let mut transition_len = 99_u64;
    let status = unsafe {
        terlan_native_dispatch_v3(
            (&mut capture as *mut Capture).cast(),
            allocate as *const () as *const c_void,
            std::ptr::null(),
            dispatch_lookup as *const c_void,
            7,
            arguments.as_ptr(),
            arguments.len() as u64,
            &mut result,
            transitions.as_mut_ptr(),
            transitions.len() as u64,
            &mut transition_len,
        )
    };
    assert_eq!(status, 0);
    assert_eq!(result, 0x5a5a_1234);
    assert_eq!(transition_len, 0);
    assert_eq!(capture.calls, 1);
    assert_eq!(capture.fields, [42]);
}
"#;

#[cfg(unix)]
pub(super) const MANAGED_COLLECTION_TAIL_LOOP_HARNESS: &str = r#"
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

#[derive(Default)]
struct Capture {
    calls: usize,
    fields: Vec<i64>,
}

unsafe extern "C" fn allocate(
    context: *mut c_void,
    operation: *const u8,
    operation_len: u64,
    fields: *const i64,
    field_count: u64,
    result: *mut u64,
) -> i32 {
    let capture = unsafe { &mut *context.cast::<Capture>() };
    let operation = unsafe { std::slice::from_raw_parts(operation, operation_len as usize) };
    assert_eq!(&operation[..4], b"TVMC");
    capture.calls += 1;
    capture.fields =
        unsafe { std::slice::from_raw_parts(fields, field_count as usize).to_vec() };
    unsafe { *result = 0x6b6b_2345 };
    0
}

fn main() {
    let arguments = [1_000_000_i64, 1_i64, 2_i64, 3_i64];
    let mut capture = Capture::default();
    let mut result = -1_i64;
    let mut transitions = [0_i64; 1];
    let mut transition_len = 99_u64;
    let status = unsafe {
        terlan_native_dispatch_v3(
            (&mut capture as *mut Capture).cast(),
            allocate as *const () as *const c_void,
            std::ptr::null(),
            dispatch_lookup as *const c_void,
            7,
            arguments.as_ptr(),
            arguments.len() as u64,
            &mut result,
            transitions.as_mut_ptr(),
            transitions.len() as u64,
            &mut transition_len,
        )
    };
    assert_eq!(status, 0);
    assert_eq!(result, 0x6b6b_2345);
    assert_eq!(transition_len, 0);
    assert_eq!(capture.calls, 1);
    assert_eq!(capture.fields, [1, 2, 3]);
}
"#;

#[cfg(unix)]
pub(super) const CANCELLING_TAIL_LOOP_HARNESS: &str = r#"
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

fn dispatch(export_id: u64, arguments: &[i64], transitions: &mut [i64]) -> (i32, i64, u64) {
    let mut result = -1_i64;
    let mut transition_len = 99_u64;
    let status = unsafe {
        terlan_native_dispatch_v3(
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            dispatch_lookup as *const c_void,
            export_id,
            arguments.as_ptr(),
            arguments.len() as u64,
            &mut result,
            transitions.as_mut_ptr(),
            transitions.len() as u64,
            &mut transition_len,
        )
    };
    (status, result, transition_len)
}

fn main() {
    let mut transitions = [0_i64; 2];
    let (status, continuation, transition_len) =
        dispatch(7, &[1_000_000, 0], &mut transitions);
    assert_eq!(status, 15);
    assert_eq!(continuation, 93);
    assert_eq!(transition_len, 2);
    assert_eq!(transitions, [123, 1_000_000]);

    let captured = transitions[1];
    let (status, result, transition_len) = dispatch(93, &[captured], &mut transitions);
    assert_eq!(status, 0);
    assert_eq!(result, 1_000_001);
    assert_eq!(transition_len, 0);
}
"#;
