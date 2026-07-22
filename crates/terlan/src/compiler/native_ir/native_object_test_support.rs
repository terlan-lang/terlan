//! Shared linked-object execution support for NativeIR tests.

use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

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
