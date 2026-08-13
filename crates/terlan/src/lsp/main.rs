#![forbid(unsafe_code)]

use std::process::ExitCode;

fn print_usage() {
    println!("terlan-lsp --stdio");
    println!("Starts the Terlan language server on standard I/O.");
}

fn main() -> ExitCode {
    let args = std::env::args().collect::<Vec<_>>();
    if args
        .iter()
        .any(|argument| matches!(argument.as_str(), "-h" | "--help"))
    {
        print_usage();
        return ExitCode::SUCCESS;
    }
    if args.len() > 1 && args[1] != "--stdio" {
        eprintln!("unexpected argument: {}", args[1]);
        print_usage();
        return ExitCode::from(2);
    }
    terlan::lsp::run_stdio_server()
}
