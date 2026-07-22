/// Returns whether REPL command-local arguments request help output.
///
/// Inputs:
/// - `args`: command-local arguments after the `repl` verb.
///
/// Output:
/// - `true` when the invocation is exactly `--help` or `-h`.
/// - `false` for seed paths, empty args, or malformed argument lists.
///
/// Transformation:
/// - Performs an exact single-argument match with no filesystem access and no
///   interactive loop side effects.
pub(super) fn is_repl_help_args(args: &[String]) -> bool {
    matches!(args, [arg] if matches!(arg.as_str(), "--help" | "-h"))
}

/// Prints REPL command help.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Writes REPL usage, source-entry rules, and control commands to stdout.
///
/// Transformation:
/// - Emits user-facing help text without mutating REPL session state.
pub(super) fn print_repl_help() {
    println!("terlc repl [--help|-h] [--debug] [<file.terl|project-dir>]");
    println!("Interactive mode accepts normal Terlan entries terminated with '.'.");
    println!("Expressions execute as admitted native application images.");
    println!("Available commands: :help, :quit, :reset, :debug, :load <file.terl|project-dir>");
}
