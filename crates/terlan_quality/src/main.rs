use std::env;
use std::path::Path;
use std::process::ExitCode;

use terlan_quality::{
    run_cli_exact_selectors, run_internal_docs, run_module_readmes, run_oxc_boundary,
    run_rust_quality, run_rustdoc, run_test_hierarchy, write_rustdoc_baseline,
};

/// Runs repository quality checks from the command line.
///
/// Inputs:
/// - First positional argument naming the quality check.
/// - Optional `--write-baseline` for `rust-docs`.
/// - Current working directory as the repository root.
///
/// Output:
/// - Exit status 0 with a success summary when the check passes.
/// - Exit status 1/2 with stable diagnostics for check failures or bad usage.
///
/// Transformation:
/// - Routes permanent repository checks to Rust implementations while keeping
///   user-facing `terlc` free of internal maintenance commands.
fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("rust-quality") => match run_rust_quality(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[rust-quality] baseline enforced: {} oversized files, {} inline-test files.",
                    summary.oversized_count, summary.inline_test_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(1)
            }
        },
        Some("rust-docs") => {
            let write_baseline = args.any(|arg| arg == "--write-baseline");
            if write_baseline {
                match write_rustdoc_baseline(Path::new(".")) {
                    Ok(count) => {
                        println!("[rustdoc] wrote baseline with {count} undocumented items.");
                        ExitCode::SUCCESS
                    }
                    Err(message) => {
                        eprintln!("{message}");
                        ExitCode::from(1)
                    }
                }
            } else {
                match run_rustdoc(Path::new(".")) {
                    Ok(summary) => {
                        println!(
                            "[rustdoc] baseline enforced: {} undocumented items.",
                            summary.undocumented_count
                        );
                        ExitCode::SUCCESS
                    }
                    Err(message) => {
                        eprintln!("{message}");
                        ExitCode::from(1)
                    }
                }
            }
        }
        Some("module-readmes") => match run_module_readmes(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[module-readmes] baseline enforced: {} missing README files.",
                    summary.missing_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(1)
            }
        },
        Some("cli-exact-selectors") => match run_cli_exact_selectors(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[cli-exact-selector] {} exact selectors resolve.",
                    summary.selector_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(1)
            }
        },
        Some("test-hierarchy") => match run_test_hierarchy(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[test-hierarchy] {} Makefile script gates are release-owned.",
                    summary.invocation_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(1)
            }
        },
        Some("internal-docs") => match run_internal_docs(Path::new(".")) {
            Ok(_) => {
                println!("[internal-docs] published docs contain no roadmap or scratch packets.");
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(1)
            }
        },
        Some("oxc-boundary") => match run_oxc_boundary(Path::new(".")) {
            Ok(_) => {
                println!(
                    "[oxc-boundary] Oxc is confined to JS backend and binding-generator ownership."
                );
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(1)
            }
        },
        Some(command) => {
            eprintln!("unsupported terlan-quality command: {command}");
            ExitCode::from(2)
        }
        None => {
            eprintln!(
                "usage: terlan-quality <rust-quality|rust-docs|module-readmes|cli-exact-selectors|test-hierarchy|internal-docs|oxc-boundary>"
            );
            ExitCode::from(2)
        }
    }
}
