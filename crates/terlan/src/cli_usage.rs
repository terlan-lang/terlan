/// Stable command-local formatter usage shared by global and local help.
pub(crate) const FMT_USAGE: &str = "terlc fmt [--check|--write] <file.terl|file.terli|file.terls|dir>...\n       terlc fmt --migrate-repeated-lets <file.terl|dir>";

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
        "terlc package protocol --out-dir <dir>",
        "terlc package publish --dry-run [project-dir] --out-dir <dir>",
        "terlc package publish --mirror <dir> [project-dir] --out-dir <dir>",
        "terlc package publish --registry <url> --publisher-key-id <id> --signing-seed-file <path> [project-dir] --out-dir <dir>",
        "terlc package add <name> <requirement> --registry <url> --trust-root <pin.json> [--offline] --out-dir <project-dir>",
        "terlc package remove <name> --registry <url> --trust-root <pin.json> [--offline] --out-dir <project-dir>",
        "terlc package resolve --registry <url> --trust-root <pin.json> [--package <name> --version <version>] [--offline] --out-dir <project-dir>",
        "terlc package update [package]... --registry <url> --trust-root <pin.json> [--offline] --out-dir <project-dir>",
        "terlc package tree --out-dir <project-dir>",
        "terlc package audit --out-dir <project-dir>",
        "terlc package yank --mirror <dir> --package <name> --version <version> [--reason-class <class>] [--message <text>] [--replacement <package>]",
        "terlc clean [project-dir]",
        "terlc doctor [project-dir]",
        "terlc inspect [project-dir] --snapshot",
        "terlc serve [web-dir] [--host <host>] [--port <port>] [--poll-ms <ms>] [--handler-runtime static] [--check|--check-config]",
        "terlc integration-test [project-dir] [--host <host>] [--port <port>] [--http-check METHOD:PATH:STATUS[:CONTAINS[:BODY]]]",
        "terlc static <emit|serve|check> <file.terl>",
        "terlc support bundle [project-dir|image.tvm] [--diagnostic <report.json>] [--out <bundle.json>]",
        "terlc test [file.terl|dir]... [--target terlan-vm|js|wasm] [--name <function>]... [--bench [--warmup <count>] [--samples <count>]]",
        "terlc doc <file.terl|dir|std> [--format html|markdown|json] [--out-dir <dir>]",
        "terlc api <emit|check|import>",
        "terlc db <init|new|validate|status|migrate|rebuild|reset>",
        "terlc debug <image.tvm> [--break <module.function|file:line>] [--script <file.terldbg>] [--json-events]",
        "terlc repl [--help] [--debug] [<file.terl|project-dir>]",
        FMT_USAGE,
        "terlc lint [--fix] [--only <rule-id>]... <file.terl|file.terli|file.terls|dir>...",
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

/// Prints usage for one known command and reports whether it was recognized.
pub(super) fn print_command_usage(command: &str) -> bool {
    match command {
        "help" => println!("terlc help [command]"),
        "init" => println!("terlc init [project-name] [--profile default|web|static]"),
        "bind" => println!(
            "terlc bind native --crate <crate-name> --out <dir>\nterlc bind js-dom --manifest <path> --out <dir>\nterlc bind cpp --manifest <path> --out <dir>\nterlc bind c --manifest <path> --out <dir>"
        ),
        "check" => println!("terlc check <file.terl|file.terli|dir> [--emit-phase-manifest <path>]"),
        "build" => println!(
            "terlc build [file.terl|dir] [--target terlan-vm|js|wasm.core] [--release] [--out-dir <dir>]"
        ),
        "run" => {
            println!("terlc run [project-dir|file.terl] [--target terlan-vm]");
            println!(
                "terlc run <artifact.wasm> [--export <name>] [--arg <type:value>] [--host-return <module.name=type:value>] [--expect <type:value>] [--repeat <count>] [--timeout-ms <ms>]"
            );
        }
        "scripts" => println!("terlc scripts [project-dir]"),
        "package" => println!(
            "terlc package fetch [project-dir] [--target <triple>] [--artifact <archive.tar.zst>]...\nterlc package protocol --out-dir <dir>\nterlc package publish --dry-run [project-dir] --out-dir <dir>\nterlc package publish --mirror <dir> [project-dir] --out-dir <dir>\nterlc package publish --registry <url> --publisher-key-id <id> --signing-seed-file <path> [project-dir] --out-dir <dir>\nterlc package add <name> <requirement> --registry <url> --trust-root <pin.json> [--offline] --out-dir <project-dir>\nterlc package remove <name> --registry <url> --trust-root <pin.json> [--offline] --out-dir <project-dir>\nterlc package resolve --registry <url> --trust-root <pin.json> [--package <name> --version <version>] [--offline] --out-dir <project-dir>\nterlc package update [package]... --registry <url> --trust-root <pin.json> [--offline] --out-dir <project-dir>\nterlc package tree --out-dir <project-dir>\nterlc package audit --out-dir <project-dir>\nterlc package yank --mirror <dir> --package <name> --version <version> [--reason-class <class>] [--message <text>] [--replacement <package>]"
        ),
        "clean" => println!("terlc clean [project-dir]"),
        "doctor" => println!("terlc doctor [project-dir]"),
        "inspect" => println!("terlc inspect [project-dir] --snapshot"),
        "serve" => println!(
            "terlc serve [web-dir] [--host <host>] [--port <port>] [--poll-ms <ms>] [--handler-runtime static] [--check|--check-config]"
        ),
        "integration-test" => println!(
            "terlc integration-test [project-dir] [--host <host>] [--port <port>] [--compose-service <name>] [--skip-db] [--skip-build] [--migrations <dir>] [--wait-secs <seconds>] [--http-check METHOD:PATH:STATUS[:CONTAINS[:BODY]]]"
        ),
        "static" => {
            println!(
                "terlc static emit <file.terl> [--out-dir <dir>] [--validate-output] [--docs (--as-of <YYYY-MM-DD>|--preview)] [--base-path <path>] [--asset-include <pattern>] [--asset-exclude <pattern>]"
            );
            println!(
                "terlc static serve <file.terl> [--out-dir <dir>] [--host <host>] [--port <port>] [--poll-ms <ms>] [--source-dir <dir>] [--validate-output] [--docs (--as-of <YYYY-MM-DD>|--preview)] [--base-path <path>]"
            );
            println!(
                "terlc static check <file.terl> [--out-dir <dir>] [--docs (--as-of <YYYY-MM-DD>|--preview)] [--base-path <path>] [--asset-include <pattern>] [--asset-exclude <pattern>]"
            );
        }
        "support" => println!(
            "terlc support bundle [project-dir|image.tvm] [--diagnostic <report.json>] [--out <bundle.json>]"
        ),
        "emit-js" => println!("terlc emit-js <file.terl> [--out-dir <dir>] [--declarations]"),
        "test" => println!(
            "terlc test [file.terl|dir] [--target terlan-vm|js|wasm] [--name <test_function>]... [--emit-test-manifest <path>] [--emit-test-result-manifest <path>]"
        ),
        "interface" => println!("terlc interface <file.terl|file.terli> [--out-dir <dir>]"),
        "doc" => println!(
            "terlc doc <file.terl|dir|std> [--format html|markdown|json] [--out-dir <dir>] [--check] [--missing-docs]"
        ),
        "api" => {
            println!(
                "terlc api emit [--source <file.terl>] [--service-name <name>] [--service-version <version>] [--out-dir <dir>]"
            );
            println!("terlc api check [--api-dir <dir>]");
            println!(
                "terlc api import <openapi.yaml|openapi.json> --module <Module.Name> --out <dir>"
            );
        }
        "db" => {
            println!("terlc db init [migrations-dir]");
            println!("terlc db new <name> [migrations-dir]");
            println!("terlc db validate [migrations-dir]");
            println!("terlc db status [--database-url URL] [migrations-dir]");
            println!("terlc db migrate [--database-url URL] [migrations-dir]");
            println!("terlc db rebuild --dev [--database-url URL] [migrations-dir]");
            println!("terlc db reset --dev [--database-url URL] [migrations-dir]");
        }
        "debug" => debug_usage_lines().iter().for_each(|line| println!("{line}")),
        "doctest" => println!("terlc doctest <file.terl>"),
        "emit-native-metadata" => {
            println!("terlc emit-native-metadata <file.terl> [--out-dir <dir>]")
        }
        "repl" => {
            println!("terlc repl [--help|-h] [--debug] [<file.terl|project-dir>]");
            println!("Interactive mode accepts normal Terlan entries terminated with '.'.");
            println!("Available commands: :help, :quit, :reset, :debug, :load <file.terl|project-dir>");
        }
        "fmt" => println!("{FMT_USAGE}"),
        "lint" => println!(
            "terlc lint [--fix] [--only <rule-id>]... <file.terl|file.terli|file.terls|dir>..."
        ),
        "migrate" => {
            println!("terlc migrate pattern-head [--write] [--json] <file.terl|file.terli|dir>")
        }
        "hover" => println!("terlc hover <file.terl> --line <line> (--column|--col) <column>"),
        "lsp" => println!("terlc lsp --stdio"),
        "version" => println!("terlc version | terlc --version | terlc -V"),
        "syntax-contract" => {
            println!("terlc syntax-contract [--fingerprint] [--out <path>]");
            println!("terlc syntax-contract --check <path>");
            println!("terlc syntax-contract --validate <path> [--no-strict]");
        }
        _ => return false,
    }
    true
}
