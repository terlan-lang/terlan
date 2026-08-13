/// Stable command-local formatter usage shared by global and local help.
pub(crate) const FMT_USAGE: &str =
    "terlc fmt [--check|--write|--migrate-repeated-lets] <file.terl|file.terli|file.terls|dir>";

/// Returns the stable public command summary.
pub(crate) fn public_usage_lines() -> &'static [&'static str] {
    &[
        "terlc help [command]",
        "terlc init [project-name] [--profile default|web|static]",
        "terlc check <file.terl|file.terli|dir>",
        "terlc build [file.terl|file.terls|dir] [--target terlan-vm|js|wasm.core] [--release] [--out-dir <dir>]",
        "terlc run [project-dir|file.terl|file.terls] [--target terlan-vm]",
        "terlc run <artifact.wasm> [--export <name>] [--arg <type:value>] [--host-return <module.name=type:value>] [--expect <type:value>] [--repeat <count>] [--timeout-ms <ms>]",
        "terlc scripts [project-dir]",
        "terlc package fetch [project-dir] [--target <triple>] [--artifact <archive.tar.zst>]...",
        "terlc clean [project-dir]",
        "terlc doctor [project-dir]",
        "terlc inspect [project-dir] --snapshot",
        "terlc serve [web-dir] [--host <host>] [--port <port>] [--poll-ms <ms>] [--handler-runtime static] [--check|--check-config]",
        "terlc integration-test [project-dir] [--host <host>] [--port <port>] [--http-check METHOD:PATH:STATUS[:CONTAINS[:BODY]]]",
        "terlc static <emit|serve|check> <file.terl>",
        "terlc support bundle [project-dir|image.tvm] [--diagnostic <report.json>] [--out <bundle.json>]",
        "terlc test [file.terl|dir] [--target terlan-vm|js|wasm] [--name <function>] [--bench [--warmup <count>] [--samples <count>]]",
        "terlc doc <file.terl|dir|std> [--format html|markdown|json] [--out-dir <dir>]",
        "terlc api <emit|check|import>",
        "terlc db <init|new|validate|status|migrate|rebuild|reset>",
        "terlc debug <image.tvm> [--break <module.function|file:line>] [--script <file.terldbg>] [--json-events]",
        "terlc repl [--help] [--debug] [<file.terl|project-dir>]",
        FMT_USAGE,
        "terlc lint [--fix] [--only <rule-id>] <file.terl|file.terli|file.terls|dir>",
        "terlc migrate pattern-head [--write] [--json] <file.terl|file.terli|dir>",
        "terlc version | terlc --version | terlc -V",
        "Global options: --diagnostic-format text|json --color auto|always|never --timings",
    ]
}

/// Returns command-local debugger help lines.
pub(crate) fn debug_usage_lines() -> &'static [&'static str] {
    &[
        concat!(
            "terlc debug <image.tvm> ",
            "[--break <module.function|file:line>] ",
            "[--script <file.terldbg>] [--json-events]"
        ),
        concat!(
            "Breakpoints: module.function, file:line, or either followed by ",
            "`where <condition>`."
        ),
        concat!(
            "Script commands: help, run, list, break, remove, enable, disable, ",
            "pause, continue, step, next, finish, bt, frame, locals, args, ",
            "print, eval, processes, process, mailbox, resources, trace, ",
            "untrace, restarts, restart, use, abort, quit."
        ),
        concat!(
            "The command admits the native image and executes breakpoints, stepping, ",
            "inspection, tracing, and typed restarts through a VM-owned shard."
        ),
    ]
}
