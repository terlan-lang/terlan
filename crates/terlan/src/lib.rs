#![deny(unsafe_code)]
#![allow(dead_code, unused_imports)]

macro_rules! vm_capability_component {
    ($($item:item)*) => {
        $(#[allow(dead_code)] $item)*
    };
}

macro_rules! vm_map_profile_component {
    ($($item:item)*) => {
        $(#[cfg(test)] $item)*
    };
}

macro_rules! vm_code_server_test_component {
    ($($item:item)*) => {
        $(#[cfg(test)] $item)*
    };
}

pub mod backends;
pub mod compiler;
pub(crate) mod database_schema;
pub mod formal_pipeline;
pub mod html;
#[cfg(feature = "editor-lsp")]
pub mod lsp;
pub(crate) mod mobile;
pub mod runtime;
pub mod support;
pub mod validation;

pub(crate) use compiler::hir as terlan_hir;
pub(crate) use compiler::purity as terlan_purity;
pub(crate) use compiler::syntax as terlan_syntax;
pub(crate) use compiler::typeck as terlan_typeck;
pub(crate) use compiler::value_lifecycle;
pub(crate) use html as terlan_html;
#[cfg(feature = "editor-lsp")]
pub(crate) use lsp as terlan_lsp;
pub(crate) use runtime::native as terlan_native;
pub(crate) use runtime::native_boundary as terlan_native_boundary;

use std::path::PathBuf;
use std::process::ExitCode;
use validation::native_policy::NativePolicy;
use validation::target_profile::TargetProfile;

mod commands;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ColorChoice {
    #[default]
    Auto,
    Always,
    Never,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiagnosticFormat {
    Text { color: ColorChoice },
    Json,
}

impl Default for DiagnosticFormat {
    fn default() -> Self {
        Self::Text {
            color: ColorChoice::Auto,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum DocFormat {
    Markdown,
    #[default]
    Html,
    Json,
}

#[derive(Clone)]
struct CliState {
    no_emit: bool,
    incremental: bool,
    timings: bool,
    experimental: bool,
    out_dir: PathBuf,
    cache_dir: Option<PathBuf>,
    trace_invalidation: bool,
    diagnostic_format: DiagnosticFormat,
    doc_format: DocFormat,
    native_policy: NativePolicy,
    target_profile: TargetProfile,
}

impl Default for CliState {
    fn default() -> Self {
        Self {
            no_emit: false,
            incremental: false,
            timings: false,
            experimental: false,
            out_dir: PathBuf::from("_build"),
            cache_dir: None,
            trace_invalidation: false,
            diagnostic_format: DiagnosticFormat::default(),
            doc_format: DocFormat::Html,
            native_policy: NativePolicy::NativeBoundaryOptional,
            target_profile: TargetProfile::Vm,
        }
    }
}

#[derive(Default, Clone)]
struct CliCommand {
    verb: Option<String>,
    args: Vec<String>,
}

fn print_usage() {}

fn print_command_usage(_command: &str) -> bool {
    false
}

#[cfg(test)]
fn run_cli(mut args: Vec<String>) -> ExitCode {
    if args.first().is_some_and(|argument| argument == "package") {
        args.remove(0);
        return commands::build::run_package_command(CliCommand {
            verb: Some("package".to_string()),
            args,
        });
    }
    ExitCode::from(2)
}

/// Runs only the persisted-image serve command in the compiler-free binary.
pub fn run_serve_runtime(mut args: Vec<String>) -> ExitCode {
    if args.first().is_some_and(|argument| argument == "serve") {
        args.remove(0);
    }
    std::env::set_var("TERLAN_SERVE_RUNTIME_ONLY", "1");
    commands::serve::run(
        CliCommand {
            verb: Some("serve".to_string()),
            args,
        },
        CliState::default(),
    )
}
