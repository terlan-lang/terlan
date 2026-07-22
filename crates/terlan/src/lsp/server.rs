use std::process::ExitCode;

use tower_lsp::{LspService, Server};

use super::Backend;

/// Runs the Terlan LSP server over stdio.
///
/// Inputs:
/// - Process stdin/stdout.
///
/// Output:
/// - Process exit code.
///
/// Transformation:
/// - Creates a single-threaded Tokio runtime and runs the async LSP service,
///   converting startup or server errors into CLI-friendly exit codes.
pub fn run_stdio_server() -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("failed to start async runtime for LSP server: {error}");
            return ExitCode::from(1);
        }
    };

    if let Err(error) = runtime.block_on(run_stdio_server_async()) {
        eprintln!("terlan-lsp failed: {error}");
        return ExitCode::from(1);
    }

    ExitCode::SUCCESS
}

async fn run_stdio_server_async() -> std::io::Result<()> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(Backend::new);

    Server::new(stdin, stdout, socket).serve(service).await;
    Ok(())
}
