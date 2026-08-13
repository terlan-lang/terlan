use super::*;

pub(super) fn run_native_package_command(
    command: Option<&str>,
    args: &mut impl Iterator<Item = String>,
) -> Option<ExitCode> {
    let _ = &mut *args;
    let result = match command {
        Some("native-boundary-terminology") => {
            match run_native_boundary_terminology(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[native-boundary-terminology] {} golden docs checked.",
                        summary.checked_doc_count
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("native-boundary-security") => match run_native_boundary_security(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[native-boundary-security] {} operations covered by {} policy rules.",
                    summary.operation_count, summary.policy_rule_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
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
                Err(message) => failure(message),
            }
        }
        Some("cuda-package-availability") => match run_cuda_package_availability(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[cuda-package-availability] status={}, driver={}, device={}, toolkit={}, libtorch-cuda={}, nvcc={}, cuda-root={}.",
                    summary.status.as_str(),
                    summary.driver_available,
                    summary.device_available,
                    summary.toolkit_available,
                    summary.libtorch_cuda_available,
                    summary.nvcc_available,
                    summary.cuda_root_available
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("cuda-package-check") => match run_cuda_package_check(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[cuda-package-check] status={}, driver={}, device={}, toolkit={}, libtorch-cuda={}, nvcc={}, cuda-root={}.",
                    summary.status.as_str(),
                    summary.driver_available,
                    summary.device_available,
                    summary.toolkit_available,
                    summary.libtorch_cuda_available,
                    summary.nvcc_available,
                    summary.cuda_root_available
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("vm-release-install-validation") => {
            match run_vm_release_install_validation(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[vm-release-install-validation] {} files checked by {} release/install rules.",
                        summary.checked_file_count, summary.required_rule_count
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("executable-docs-vm") => match run_executable_docs_vm(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[executable-docs-vm] {} Markdown files, {} Terlan blocks, {} complete modules, {} fragments checked; report written to {}.",
                    summary.markdown_file_count,
                    summary.terlan_block_count,
                    summary.complete_module_count,
                    summary.fragment_count,
                    summary.report_path.display()
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        _ => return None,
    };
    Some(result)
}
