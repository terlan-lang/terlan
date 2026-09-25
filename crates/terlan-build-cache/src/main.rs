//! Compiler-independent maintenance of the explicitly owned Rust cache namespace.
#![forbid(unsafe_code)]

#[cfg(target_os = "linux")]
mod admission;
#[cfg(target_os = "linux")]
mod incremental;
#[cfg(target_os = "linux")]
mod layout;
#[cfg(target_os = "linux")]
mod owner;

#[cfg(target_os = "linux")]
fn main() -> std::process::ExitCode {
    match run() {
        Ok(true) => std::process::ExitCode::SUCCESS,
        Ok(false) => std::process::ExitCode::from(2),
        Err(error) => {
            eprintln!("error[build.cache]: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(target_os = "linux")]
fn run() -> std::io::Result<bool> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && args[0] == "owner-protocol" {
        println!("{}", owner::PROTOCOL);
        return Ok(true);
    }
    if args.first().is_some_and(|arg| arg == "admit") {
        return admission::run(&std::env::current_dir()?, &args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "owner") {
        return owner::run(&std::env::current_dir()?, &args[1..]);
    }
    let [command] = args.as_slice() else {
        return Err(std::io::Error::other(
            "usage: terlan-build-cache audit|prune (from repository root)",
        ));
    };
    let prune = match command.to_str() {
        Some("audit") => false,
        Some("prune") => true,
        _ => return Err(std::io::Error::other("expected audit or prune")),
    };
    let report = incremental::maintain(
        &std::env::current_dir()?,
        prune,
        incremental::Policy::default(),
        std::time::SystemTime::now(),
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(report.budget_verified)
}

#[cfg(not(target_os = "linux"))]
fn main() -> std::process::ExitCode {
    eprintln!("error[build.cache]: Rust session-lock interoperability is only validated on Linux");
    std::process::ExitCode::FAILURE
}
