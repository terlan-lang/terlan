//! Full-cycle assertions against one compiled generated C++ helper.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

/// Exercises exact resource, copied-value, enum, and exception replies.
pub(super) fn assert_generated_helper_replies(helper: &Path) {
    let mut child = Command::new(helper)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("start generated helper");
    let mut input = child.stdin.take().expect("helper input");
    let mut output = BufReader::new(child.stdout.take().expect("helper output"));
    let type_name = STANDARD.encode("cpp_fixture.NativeBoundary.NativeBoundary");
    let mut call = |request_id: u64, operation: &str, args: &str| {
        writeln!(
            input,
            "call {request_id} {}{args}",
            STANDARD.encode(operation)
        )
        .expect("write helper call");
        input.flush().expect("flush helper call");
        let mut reply = String::new();
        output.read_line(&mut reply).expect("read helper reply");
        reply.trim_end().to_string()
    };
    let first = call(1, "cpp_fixture.native_boundary.new", " i:42");
    let fields = first.split_whitespace().collect::<Vec<_>>();
    let ["reply", "1", "1", "ok_handle", owner, "1", "1", returned_type] = fields.as_slice() else {
        panic!("unexpected owner-bound handle reply: {first}");
    };
    assert_eq!(*returned_type, type_name);
    let owner = (*owner).to_string();
    let handle = format!(" h:{owner}:1:1:{type_name}");
    assert_eq!(
        call(2, "cpp_fixture.native_boundary.label", &handle),
        format!("reply 2 1 ok_string {}", STANDARD.encode("42"))
    );
    assert_eq!(
        call(3, "cpp_fixture.native_boundary.bytes", &handle),
        format!("reply 3 1 ok_bytes {}", STANDARD.encode([42_u8, 43]))
    );
    assert_eq!(
        call(4, "cpp_fixture.native_boundary.samples", &handle),
        "reply 4 1 ok_ints 42,84"
    );
    assert_eq!(
        call(5, "cpp_fixture.native_boundary.mode", &handle),
        format!("reply 5 1 ok_atom {}", STANDARD.encode("doubled"))
    );
    assert_eq!(
        call(6, "cpp_fixture.native_boundary.new", " i:1"),
        format!("reply 6 1 ok_handle {owner} 2 1 {type_name}")
    );
    let hidden_handle = format!(" h:{owner}:2:1:{type_name}");
    assert_eq!(
        call(7, "cpp_fixture.native_boundary.mode", &hidden_handle),
        format!(
            "reply 7 1 err {} {}",
            STANDARD.encode("native_unknown_enum"),
            STANDARD.encode("mode returned an unselected enum value")
        )
    );
    assert_eq!(
        call(8, "cpp_fixture.native_boundary.new", " i:-1"),
        format!("reply 8 1 ok_handle {owner} 3 1 {type_name}")
    );
    let failing_handle = format!(" h:{owner}:3:1:{type_name}");
    let failure = call(
        9,
        "cpp_fixture.native_boundary.tripled_or_error",
        &failing_handle,
    );
    assert_eq!(
        failure,
        format!(
            "reply 9 1 result_err {} {}",
            STANDARD.encode("boundary_operation_failed"),
            STANDARD.encode("Native boundary operation failed.")
        )
    );
    assert!(!failure.contains("sensitive"));
    assert_eq!(
        call(10, "cpp_fixture.native_boundary.tripled_or_error", &handle,),
        "reply 10 1 result_ok_int 126"
    );
    let foreign_handle = format!(" h:{}:1:1:{type_name}", STANDARD.encode("foreign-worker"));
    assert_eq!(
        call(11, "cpp_fixture.native_boundary.label", &foreign_handle),
        format!(
            "reply 11 1 err {} {}",
            STANDARD.encode("cross_owner_handle"),
            STANDARD.encode("native resource belongs to another worker")
        )
    );
    let snapshot = format!(
        " r:{}:{}:i:42,{}:i:84",
        STANDARD.encode("NativeSnapshot"),
        STANDARD.encode("value"),
        STANDARD.encode("doubled")
    );
    assert_eq!(
        call(12, "cpp_fixture.native_boundary.sum_snapshot", &snapshot),
        "reply 12 1 ok_int 126"
    );
    let incomplete = format!(
        " r:{}:{}:i:42",
        STANDARD.encode("NativeSnapshot"),
        STANDARD.encode("value")
    );
    assert_eq!(
        call(13, "cpp_fixture.native_boundary.sum_snapshot", &incomplete),
        format!(
            "reply 13 1 err {} {}",
            STANDARD.encode("missing_record_field"),
            STANDARD.encode("doubled")
        )
    );
    assert_eq!(
        call(14, "cpp_fixture.native_boundary.sum_integers", " li:1,-2,4"),
        "reply 14 1 ok_int 3"
    );
    assert_eq!(
        call(15, "cpp_fixture.native_boundary.sum_integers", " ls:"),
        "reply 15 1 ok_int 0"
    );
    assert_eq!(
        call(
            16,
            "cpp_fixture.native_boundary.sum_floats",
            " lf:1.5,-2.25,3"
        ),
        "reply 16 1 ok_float 2.25"
    );
    assert_eq!(
        call(17, "cpp_fixture.native_boundary.sum_floats", " ls:"),
        "reply 17 1 ok_float 0"
    );
    assert_eq!(
        call(18, "cpp_fixture.native_boundary.sum_floats", " li:1,2"),
        format!(
            "reply 18 1 err {} {}",
            STANDARD.encode("invalid_arguments"),
            STANDARD.encode("sum_floats received invalid arguments")
        )
    );
    assert_eq!(
        call(19, "cpp_fixture.native_boundary.owned_snapshot", " i:7"),
        format!(
            "reply 19 1 ok_record {} {}:i:7,{}:i:14",
            STANDARD.encode("NativeSnapshot"),
            STANDARD.encode("value"),
            STANDARD.encode("doubled")
        )
    );
    assert_eq!(
        call(20, "cpp_fixture.native_boundary.sum_floats", " lf:1,,2"),
        format!(
            "reply 20 1 err {} {}",
            STANDARD.encode("invalid_argument"),
            STANDARD.encode("cannot parse float from empty string")
        )
    );
    let duplicate = call(20, "cpp_fixture.native_boundary.sum_floats", " lf:1");
    assert!(
        duplicate.contains(&STANDARD.encode("request_not_monotonic")),
        "duplicate request id was not rejected: {duplicate}"
    );
    drop(call);
    writeln!(
        input,
        "{}",
        "x".repeat(
            crate::runtime::native_boundary::adapter_abi::PUBLIC_ADAPTER_MAX_FRAME_BYTES + 1
        )
    )
    .expect("write oversized helper frame");
    input.flush().expect("flush oversized helper frame");
    let mut oversized = String::new();
    output
        .read_line(&mut oversized)
        .expect("read oversized-frame reply");
    assert!(
        oversized.contains(&STANDARD.encode("frame_too_large")),
        "oversized frame was not rejected: {oversized}"
    );
    drop(input);
    assert!(
        child.wait().expect("reap generated helper").success(),
        "generated helper did not terminate cleanly"
    );
}
