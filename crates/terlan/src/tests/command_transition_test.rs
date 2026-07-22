use super::*;

/// Guards REPL expression execution through the persistent AOT service.
///
/// Inputs:
/// - The local REPL command source files.
/// - The local Rust VM source file.
///
/// Output:
/// - Test success when REPL expression execution owns a persistent compiler
///   service, publishes already checked CoreIR, and emits native artifacts.
///
/// Transformation:
/// - Reads the REPL command/VM sources as text and checks the
///   interactive execution invariant.
#[test]
fn repl_expression_execution_uses_persistent_aot_service() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/commands/repl");
    let source = ["mod.rs", "mod_part_001.rs", "mod_part_002.rs"]
        .iter()
        .map(|name| fs::read_to_string(root.join(name)).expect("read repl command source"))
        .collect::<Vec<_>>()
        .join("\n");
    let vm = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/runtime/vm.rs"))
        .expect("read vm source");

    assert!(
        source.contains("ReplCompilerService")
            && source.contains("publish_compiled_source")
            && !source.contains("write_repl_pure_native_artifact")
            && !source.contains("load_pure_native_artifact")
            && !source.contains("run_compiled_repl_expression_on_beam"),
        "REPL expression execution must compile, publish, and execute with AOT-native VM generations"
    );
    assert!(
        !vm.contains("Command::new(\"erlc\")") && !vm.contains("Command::new(\"erl\")"),
        "Rust VM execution must not invoke Vm target runtime commands"
    );
    assert!(
        source.contains("ReplRuntime::Vm") && !source.contains("ReplRuntime::Beam"),
        "REPL expression execution must keep only the Rust VM branch in the public CLI"
    );
}
