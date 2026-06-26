use super::*;

/// Verifies one helper process keeps vector resources alive across calls.
///
/// Inputs:
/// - A new worker and encoded term payloads.
///
/// Output:
/// - Test passes when handle-producing and handle-consuming operations share
///   Rust-owned state.
///
/// Transformation:
/// - Exercises the same line protocol used by the generated BEAM runtime.
#[test]
fn helper_line_protocol_preserves_vector_state() {
    let mut worker = SafeNativeWorker::new(32);
    let created = execute_line(&mut worker, 1, "from_list QQ==,Qg==");
    assert_eq!(created, "ok_handle 1 1");

    let len = execute_line(&mut worker, 2, "length 1 1");
    assert_eq!(len, "ok_int 2");

    let value = execute_line(&mut worker, 3, "get_at 1 1 1");
    assert_eq!(value, "ok_term Qg==");
}

/// Verifies mutation updates the Rust-owned vector behind the same handle.
///
/// Inputs:
/// - A new worker and three encoded payloads.
///
/// Output:
/// - Test passes when `push` and `to_list` observe shared resource state.
///
/// Transformation:
/// - Ensures helper calls mutate the SafeNative resource store rather than
///   returning detached values.
#[test]
fn helper_line_protocol_mutates_vector_state() {
    let mut worker = SafeNativeWorker::new(32);
    assert_eq!(execute_line(&mut worker, 1, "new"), "ok_handle 1 1");
    assert_eq!(
        execute_line(&mut worker, 2, "push 1 1 QQ=="),
        "ok_handle 1 1"
    );
    assert_eq!(
        execute_line(&mut worker, 3, "push 1 1 Qg=="),
        "ok_handle 1 1"
    );
    assert_eq!(
        execute_line(&mut worker, 4, "to_list 1 1"),
        "ok_terms QQ==,Qg=="
    );
}

/// Verifies separate vector handles do not share storage.
///
/// Inputs:
/// - Two created vectors and mutations applied only to the first handle.
///
/// Output:
/// - Test passes when the first vector changes and the second remains
///   unchanged.
///
/// Transformation:
/// - Exercises handle isolation inside one Rust-owned SafeNative worker.
#[test]
fn helper_line_protocol_isolates_multiple_vector_handles() {
    let mut worker = SafeNativeWorker::new(32);
    assert_eq!(
        execute_line(&mut worker, 1, "from_list QQ=="),
        "ok_handle 1 1"
    );
    assert_eq!(
        execute_line(&mut worker, 2, "from_list Wg=="),
        "ok_handle 2 1"
    );
    assert_eq!(
        execute_line(&mut worker, 3, "push 1 1 Qg=="),
        "ok_handle 1 1"
    );

    assert_eq!(
        execute_line(&mut worker, 4, "to_list 1 1"),
        "ok_terms QQ==,Qg=="
    );
    assert_eq!(execute_line(&mut worker, 5, "to_list 2 1"), "ok_terms Wg==");
}

/// Verifies indexed mutations preserve order after set and swap.
///
/// Inputs:
/// - One vector with three encoded values.
///
/// Output:
/// - Test passes when `set_at` and `swap` mutate the expected slots.
///
/// Transformation:
/// - Exercises the helper protocol for the Vector operations most likely to
///   expose off-by-one or stale-handle mistakes.
#[test]
fn helper_line_protocol_sets_and_swaps_indexes() {
    let mut worker = SafeNativeWorker::new(32);
    assert_eq!(
        execute_line(&mut worker, 1, "from_list QQ==,Qg==,Qw=="),
        "ok_handle 1 1"
    );
    assert_eq!(
        execute_line(&mut worker, 2, "set_at 1 1 1 WA=="),
        "ok_handle 1 1"
    );
    assert_eq!(
        execute_line(&mut worker, 3, "swap 1 1 0 2"),
        "ok_handle 1 1"
    );

    assert_eq!(
        execute_line(&mut worker, 4, "to_list 1 1"),
        "ok_terms Qw==,WA==,QQ=="
    );
}

/// Verifies malformed helper commands return stable protocol errors.
///
/// Inputs:
/// - Bad handle fields, missing arguments, and an unknown command.
///
/// Output:
/// - Test passes when each malformed line returns an `err` response with a
///   stable machine-readable code.
///
/// Transformation:
/// - Exercises helper-side protocol validation before requests reach the
///   SafeNative worker.
#[test]
fn helper_line_protocol_rejects_malformed_commands() {
    let mut worker = SafeNativeWorker::new(32);

    assert!(execute_line(&mut worker, 1, "").starts_with("err native_vector_empty_command "));
    assert!(
        execute_line(&mut worker, 2, "missing").starts_with("err native_vector_unknown_command ")
    );
    assert!(execute_line(&mut worker, 3, "length nope 1")
        .starts_with("err native_vector_invalid_integer "));
    assert!(
        execute_line(&mut worker, 4, "push 1 1").starts_with("err native_vector_missing_value ")
    );
    assert!(execute_line(&mut worker, 5, "new extra")
        .starts_with("err native_vector_unexpected_argument "));
    assert!(execute_line(&mut worker, 6, "from_list QQ== extra")
        .starts_with("err native_vector_unexpected_argument "));
    assert!(execute_line(&mut worker, 7, "from_list not-base64")
        .starts_with("err native_vector_invalid_encoded_term "));
    assert!(execute_line(&mut worker, 8, "push 1 1 not-base64")
        .starts_with("err native_vector_invalid_encoded_term "));
}

/// Verifies invalid resource access is rejected by the Rust store.
///
/// Inputs:
/// - One live vector handle, stale generations, and invalid indexes.
///
/// Output:
/// - Test passes when stale handles and index failures are reported by
///   SafeNative resource/vector diagnostics.
///
/// Transformation:
/// - Exercises the helper path through the worker so these failures cannot be
///   hidden by BEAM-side list semantics.
#[test]
fn helper_line_protocol_rejects_stale_handles_and_bad_indexes() {
    let mut worker = SafeNativeWorker::new(32);
    assert_eq!(
        execute_line(&mut worker, 1, "from_list QQ=="),
        "ok_handle 1 1"
    );

    assert!(execute_line(&mut worker, 2, "length 1 2").starts_with("err resource.stale_handle "));
    assert!(execute_line(&mut worker, 3, "get_at 1 1 -1").starts_with("err vector.negative_index "));
    assert!(
        execute_line(&mut worker, 4, "get_at 1 1 3").starts_with("err vector.index_out_of_bounds ")
    );
    assert!(execute_line(&mut worker, 5, "set_at 1 1 2 Qg==")
        .starts_with("err vector.index_out_of_bounds "));
}
