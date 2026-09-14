use super::*;

#[test]
fn generated_reentry_runs_on_a_small_stack_with_periodic_budgeted_yields() {
    let mut modules = vec![module(deep_countdown_body())];
    super::super::tail_position::install_reduction_continuations(&mut modules)
        .expect("install source recursion reduction entry");
    let mut reentry = modules[0].continuations[0].clone();
    reentry.id ^= 1;
    let reentry_id = reentry.id;
    modules[0].continuations.push(reentry);
    let NativeExpr::If { clauses } = &mut modules[0].functions[0].body else {
        panic!("countdown conditional");
    };
    clauses[1].1 = NativeExpr::ContinuationTailCall {
        continuation_id: reentry_id,
        args: vec![decrement(), increment_accumulator()],
    };
    super::super::tail_position::attach_installed_reduction_yields(&mut modules);
    super::super::continuation_sharing::materialize_shared_continuations(&mut modules)
        .expect("share generated reentry bodies");
    lower_recursive_tail_calls(&mut modules);
    super::super::tail_position::attach_installed_reduction_yields(&mut modules);
    let object = super::super::emit_native_application_object("budgeted_reentry", &modules)
        .expect("emit generated reentry object");
    let root = crate::support::test_fs::TestDirectory::new("terlan-tail", "budgeted-reentry");
    let object_path = root.path().join("reentry.o");
    let harness_path = root.path().join("harness.rs");
    let executable_path = root.path().join("harness");
    fs::write(&object_path, object).expect("write generated reentry object");
    fs::write(&harness_path, with_dispatch_lookup_harness(HARNESS))
        .expect("write generated reentry harness");
    compile_and_run_with_small_stack(&object_path, &harness_path, &executable_path);
    root.close();
}

const HARNESS: &str = r#"
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
    let mut entry = 7;
    let mut arguments = [1_000_000_i64, 0_i64];
    let mut resumes = 0;
    loop {
        let mut result = -1;
        let mut transitions = [0_i64; 2];
        let mut transition_len = 99;
        let status = unsafe {
            terlan_native_dispatch_v3(
                std::ptr::null_mut(), std::ptr::null(), std::ptr::null(),
                dispatch_lookup as *const c_void, entry,
                arguments.as_ptr(), 2, &mut result, transitions.as_mut_ptr(),
                2, &mut transition_len,
            )
        };
        if status == 0 {
            assert_eq!(result, 1_000_000);
            assert_eq!(transition_len, 0);
            assert!(resumes > 0, "recursive work must remain preemptible");
            break;
        }
        assert_eq!(status, 6, "only reduction yields are expected");
        assert_eq!(transition_len, 2);
        assert!(transitions[0] < arguments[0], "each slice must progress");
        assert_eq!(transitions[0] + transitions[1], 1_000_000);
        resumes += 1;
        assert!(resumes <= 1000, "generated reentry must not yield on every edge");
        entry = u64::from_ne_bytes(result.to_ne_bytes());
        arguments = transitions;
    }
}
"#;
