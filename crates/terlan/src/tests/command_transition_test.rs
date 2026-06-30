use super::*;

/// Guards the standard emit command against direct syntax-output Erlang lowering.
///
/// Inputs:
/// - The local `commands/emit/mod.rs` source file.
///
/// Output:
/// - Test success when the command uses the CoreIR-gated backend entry point
///   and does not import/call the direct syntax-output Erlang emitter.
///
/// Transformation:
/// - Reads the command source as text and checks the transition invariant
///   required while direct syntax-output emitters still exist for parity
///   and compatibility paths.
#[test]
fn emit_command_uses_core_ir_gated_erlang_lowering() {
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/commands/emit/mod.rs"))
            .expect("read emit command source");

    assert!(
        source.contains("try_emit_core_module_to_erlang_with_syntax_bridge"),
        "emit command must use the CoreIR-gated Erlang backend"
    );
    assert!(
            !source.contains(
                "try_emit_syntax_module_output_to_erlang_with_interfaces_file_imports_templates_and_markdown"
            ),
            "emit command must not call direct syntax-output Erlang lowering"
        );
}

/// Guards REPL expression execution against target-runtime execution.
///
/// Inputs:
/// - The local `commands/repl/mod.rs` source file.
/// - The local Rust VM source file.
///
/// Output:
/// - Test success when REPL expression execution loads CoreIR into the
///   compiler-owned Rust VM and does not invoke Erlang compiler/runtime
///   commands.
///
/// Transformation:
/// - Reads the REPL command/VM sources as text and checks the
///   interactive execution invariant.
#[test]
fn repl_expression_execution_uses_core_ir_evaluator() {
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/commands/repl/mod.rs"))
            .expect("read repl command source");
    let vm = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/runtime/vm.rs"))
        .expect("read vm source");

    assert!(
        source.contains("run_compiled_repl_expression_in_vm")
            && source.contains("run_compiled_repl_expression_on_beam"),
        "REPL expression execution must load compiler CoreIR into TerlanVm"
    );
    assert!(
        !vm.contains("Command::new(\"erlc\")") && !vm.contains("Command::new(\"erl\")"),
        "Rust VM execution must not invoke Erlang target runtime commands"
    );
    assert!(
        source.contains("ReplRuntime::Beam") && source.contains("ReplRuntime::Vm"),
        "REPL expression execution must keep both BEAM and Rust VM branches explicit"
    );
}
