use std::env;
use std::path::Path;
use std::process::ExitCode;

#[path = "mod.rs"]
pub mod terlan_quality;

use crate::terlan_quality::{
    run_cli_exact_selectors, run_erlang_modernization_inventory, run_erlang_runtime_matrix,
    run_executable_docs_vm, run_internal_docs, run_module_readmes,
    run_native_binding_generator_contract, run_no_default_tokio_runtime, run_oxc_boundary,
    run_package_git_source, run_package_lockfile_contract, run_rust_quality, run_rustdoc,
    run_std_generated_metadata, run_std_source_naming, run_test_hierarchy, run_vm_artifact_format, run_vm_coverage_100, write_rustdoc_baseline,
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
        Some("std-source-naming") => match run_std_source_naming(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[std-source-naming] {} hand-authored std sources match module filenames.",
                    summary.checked_source_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(1)
            }
        },
        Some("std-generated-metadata") => match run_std_generated_metadata(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[std-generated-metadata] {} generated std artifacts have minimal headers.",
                    summary.checked_file_count
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
        Some("erlang-modernization-inventory") => {
            match run_erlang_modernization_inventory(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[erlang-modernization] emitted {} EM0 artifacts for {} kept apps; {} removed apps absent.",
                        summary.artifact_count,
                        summary.kept_app_count,
                        summary.removed_app_count
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => {
                    eprintln!("{message}");
                    ExitCode::from(1)
                }
            }
        }
        Some("erlang-runtime-matrix") => match run_erlang_runtime_matrix(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[erlang-runtime-matrix] {} runtime lanes passed `{}`.",
                    summary.lane_count, summary.command
                );
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(1)
            }
        },
        Some("vm-artifact-format") => match run_vm_artifact_format(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[vm-artifact-format] {} artifact contract groups enforced.",
                    summary.required_group_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(1)
            }
        },
        Some("vm-coverage-100") => match run_vm_coverage_100(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[vm-coverage-100] {} VM-owned files enforce 100% line/function coverage.",
                    summary.coverage_file_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(1)
            }
        },
        Some("native-binding-generator-contract") => {
            match run_native_binding_generator_contract(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[native-binding-generator-contract] {} required terms and {} rejection terms enforced.",
                        summary.required_term_count, summary.rejection_term_count
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => {
                    eprintln!("{message}");
                    ExitCode::from(1)
                }
            }
        }
        Some("no-default-tokio-runtime") => match run_no_default_tokio_runtime(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[no-default-tokio-runtime] {} Tokio references classified by {} inventory rows.",
                    summary.scanned_reference_count, summary.inventory_row_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(1)
            }
        },
        Some("package-git-source") => match run_package_git_source(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[package-git-source] {} Git source contract terms enforced.",
                    summary.required_term_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(1)
            }
        },
        Some("package-lockfile-contract") => match run_package_lockfile_contract(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[package-lockfile-contract] {} lockfile contract terms enforced.",
                    summary.required_term_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(1)
            }
        },
        Some("executable-docs-vm") => match run_executable_docs_vm(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[executable-docs-vm] {} Markdown files, {} Terlan blocks, {} complete modules, {} fragments checked.",
                    summary.markdown_file_count,
                    summary.terlan_block_count,
                    summary.complete_module_count,
                    summary.fragment_count
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
                "usage: terlan-quality <rust-quality|rust-docs|module-readmes|cli-exact-selectors|test-hierarchy|std-source-naming|std-generated-metadata|internal-docs|oxc-boundary|erlang-modernization-inventory|erlang-runtime-matrix|vm-artifact-format|vm-coverage-100|native-binding-generator-contract|no-default-tokio-runtime|package-git-source|package-lockfile-contract|executable-docs-vm>"
            );
            ExitCode::from(2)
        }
    }
}
