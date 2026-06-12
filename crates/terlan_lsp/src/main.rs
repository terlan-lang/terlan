use std::process::ExitCode;

fn print_usage() {
    println!("terlan-lsp --stdio");
    println!("Starts the Terlan language server on standard I/O.");
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "-h" | "--help"))
    {
        print_usage();
        return ExitCode::SUCCESS;
    }

    if args.len() > 1 && args[1] != "--stdio" {
        eprintln!("unexpected argument: {}", args[1]);
        print_usage();
        return ExitCode::from(2);
    }

    terlan_lsp::run_stdio_server()
}
