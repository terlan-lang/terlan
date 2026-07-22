use super::*;

/// Keeps local-call ownership behind one public descriptor export after pure
/// functions move to native AOT.
#[test]
fn asm_labels_parity_classifies_pure_local_call_graph_as_native_aot() {
    let dir = make_temp_dir("asm_labels_artifact");
    let source_path = dir.join("asm_labels.terl");
    let out_dir = dir.join("build");
    fs::write(
        &source_path,
        "module asm_labels.\n\nfoo(): Int -> foo(16).\n\nfoo(value: Int): Int -> value + 1.\n\nbar(): Int -> foo() + foo(1).\n\nrecur(value: Int): Int ->\n    if {\n        value == 0 -> 0;\n        true -> recur(value - 1)\n    }.\n\npub choose(value: Int): Int ->\n    case value {\n        0 -> foo();\n        _ -> 0\n    }.\n\npub main(): Int -> bar() + recur(2).\n\npub marker(): String -> \"{call_only,fake/9}\".\n",
    )
    .expect("write call-dependency fixture");
    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![source_path.display().to_string()],
    };

    assert_eq!(run(cmd, state), ExitCode::SUCCESS);
    let image = fs::read(out_dir.join("vm/asm_labels.tvm")).expect("read native image");
    let target = crate::runtime::native_image::host_tvm_target().expect("host TVM target");
    let inspection = crate::runtime::native_image::inspect_tvm_image(&image, &target.triple)
        .expect("inspect call-dependency native image");
    assert_eq!(inspection.descriptor.exports.len(), 1);
    assert_eq!(inspection.descriptor.exports[0].name, "asm_labels.main/0");
}
