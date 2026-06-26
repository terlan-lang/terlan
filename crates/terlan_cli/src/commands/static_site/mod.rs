use std::collections::{hash_map::DefaultHasher, BTreeMap};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::thread;
use std::time::{Duration, UNIX_EPOCH};

use terlan_syntax::{SyntaxImportKind, SyntaxModuleOutput};

use crate::commands::artifacts::{
    collect_syntax_asset_imports_matching, collect_syntax_markdown_frontend_inputs,
    collect_syntax_template_inputs,
};
use crate::validation::static_output::{
    validate_static_css_output_files, validate_static_html_output,
};
use crate::validation::target_profile::TargetProfileCheckOptions;
use crate::{CliCommand, CliState};

mod command;
mod filters;
mod html_usage;
mod render;
mod render_lookup;
mod render_markdown;
mod render_values;
mod routes;
pub(crate) use command::run;
pub(crate) use filters::AssetFilters;
pub(crate) use html_usage::*;
pub(crate) use render::{render_syntax_static_entrypoint, StaticSyntaxRenderError};
pub(crate) use render_markdown::render_syntax_static_markdown_layout;
pub(crate) use routes::*;

/// Reserved template prop name used for component children.
pub(crate) const TEMPLATE_CHILDREN_SLOT: &str = "children";

/// Parsed command-local arguments for `terlc static emit`.
///
/// Inputs:
/// - Produced by `parse_emit_static_args`.
///
/// Output:
/// - Source file path, validation mode, and asset filters.
///
/// Transformation:
/// - Carries normalized static emit settings into command execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EmitStaticArgs {
    pub(crate) file: String,
    pub(crate) validate_output: bool,
    pub(crate) asset_filters: AssetFilters,
    pub(crate) base_path: Option<String>,
}

/// Parsed command-local arguments for `terlc static serve`.
///
/// Inputs:
/// - Produced by `parse_serve_static_args`.
///
/// Output:
/// - Static source, bind address, polling interval, source directory override,
///   check-only mode, and embedded static emit settings.
///
/// Transformation:
/// - Combines dev-server flags with reusable static emit settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ServeStaticArgs {
    pub(crate) file: String,
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) poll_ms: u64,
    pub(crate) source_dir: Option<PathBuf>,
    pub(crate) check_only: bool,
    pub(crate) emit_args: EmitStaticArgs,
}

/// Parses command-local arguments for `terlc static emit`.
///
/// Inputs:
/// - `args`: arguments after the `static emit` subcommand.
///
/// Output:
/// - Parsed static emit settings or an error message.
///
/// Transformation:
/// - Scans one file argument plus `--validate-output`, `--base-path`,
///   `--asset-include`, and `--asset-exclude` flags.
pub(crate) fn parse_emit_static_args(args: &[String]) -> Result<EmitStaticArgs, String> {
    let mut file = None;
    let mut validate_output = false;
    let mut includes = Vec::new();
    let mut excludes = Vec::new();
    let mut base_path = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--validate-output" => {
                validate_output = true;
                index += 1;
            }
            "--base-path" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--base-path requires a path".to_string());
                };
                base_path = Some(normalize_static_base_path(value)?);
                index += 2;
            }
            "--asset-include" => {
                let Some(pattern) = args.get(index + 1) else {
                    return Err("--asset-include requires a pattern".to_string());
                };
                includes.push(pattern.clone());
                index += 2;
            }
            "--asset-exclude" => {
                let Some(pattern) = args.get(index + 1) else {
                    return Err("--asset-exclude requires a pattern".to_string());
                };
                excludes.push(pattern.clone());
                index += 2;
            }
            option if option.starts_with("--") => {
                return Err(format!("unsupported static emit option: {}", option));
            }
            path => {
                if file.replace(path.to_string()).is_some() {
                    return Err("static emit expects exactly one file argument".to_string());
                }
                index += 1;
            }
        }
    }

    let Some(file) = file else {
        return Err("static emit expects exactly one file argument".to_string());
    };

    Ok(EmitStaticArgs {
        file,
        validate_output,
        asset_filters: AssetFilters { includes, excludes },
        base_path,
    })
}

/// Normalizes a static-site base path for generated HTML.
///
/// Inputs:
/// - `value`: CLI-provided URL path prefix such as `/terlan`.
///
/// Output:
/// - Normalized path ending in `/`, or an error message.
///
/// Transformation:
/// - Requires an absolute URL path, rejects traversal and unsafe HTML
///   attribute characters, and normalizes `/docs` to `/docs/` for use in an
///   HTML `<base href>` tag.
fn normalize_static_base_path(value: &str) -> Result<String, String> {
    if value.is_empty() {
        return Err("--base-path cannot be empty".to_string());
    }
    if !value.starts_with('/') {
        return Err(format!("--base-path must start with `/`: `{value}`"));
    }
    if value
        .chars()
        .any(|ch| ch.is_whitespace() || matches!(ch, '"' | '\'' | '<' | '>'))
    {
        return Err(format!("--base-path contains unsafe characters: `{value}`"));
    }
    if value.contains('?') || value.contains('#') {
        return Err(format!(
            "--base-path must be a path, not a URL query or fragment: `{value}`"
        ));
    }
    if value == "/" {
        return Ok("/".to_string());
    }
    for segment in value.trim_matches('/').split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(format!("invalid --base-path segment in `{value}`"));
        }
    }

    Ok(format!("{}/", value.trim_end_matches('/')))
}

/// Executes the static emit command runner.
///
/// Inputs:
/// - `cmd`: parsed command containing one source path and static flags.
/// - `state`: global CLI state, including output directory, cache directory,
///   diagnostic format, and native policy.
///
/// Output:
/// - `ExitCode::SUCCESS` when static output files are written.
/// - `ExitCode::from(2)` for malformed arguments.
/// - `ExitCode::from(1)` for read, compile, route, render, validation, asset,
///   directory, or write failures.
///
/// Transformation:
/// - Compiles one source module, discovers static entrypoints/routes, renders
///   HTML, copies static assets, and optionally validates generated output.
pub(crate) fn run_emit_static(cmd: CliCommand, state: CliState) -> ExitCode {
    let args = match parse_emit_static_args(&cmd.args) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{}", message);
            return ExitCode::from(2);
        }
    };

    let path = &args.file;
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(err) => {
            eprintln!("failed to read {}: {}", path, err);
            return ExitCode::from(1);
        }
    };

    let compiled =
        match crate::formal_pipeline::compile_syntax_module_through_phases_with_profile_options(
            path,
            &source,
            state.diagnostic_format,
            state.cache_dir.as_deref(),
            state.native_policy,
            state.target_profile,
            TargetProfileCheckOptions {
                allow_asset_imports: true,
                allow_rust_backed_std_modules: state.native_policy
                    != crate::validation::native_policy::NativePolicy::Pure,
            },
        ) {
            Ok(compiled) => compiled,
            Err(exit_code) => return exit_code,
        };
    let syntax_output = compiled.syntax_output;
    let entrypoints = discover_syntax_static_entrypoints(&syntax_output);
    let routes = match discover_syntax_static_routes(&syntax_output) {
        Ok(routes) => routes,
        Err(message) => {
            eprintln!("{}", message);
            return ExitCode::from(1);
        }
    };
    if let Err(message) = validate_syntax_static_route_handlers(&syntax_output, &routes) {
        eprintln!("{}", message);
        return ExitCode::from(1);
    }
    let templates = match collect_syntax_template_inputs(&syntax_output, Path::new(path)) {
        Ok(templates) => templates,
        Err(message) => {
            eprintln!("{}", message);
            return ExitCode::from(1);
        }
    };
    let markdown_inputs =
        match collect_syntax_markdown_frontend_inputs(&syntax_output, Path::new(path)) {
            Ok(markdown_inputs) => markdown_inputs,
            Err(message) => {
                eprintln!("{}", message);
                return ExitCode::from(1);
            }
        };
    let markdown_routes = match discover_markdown_static_routes(&markdown_inputs) {
        Ok(markdown_routes) => markdown_routes,
        Err(message) => {
            eprintln!("{}", message);
            return ExitCode::from(1);
        }
    };
    if let Err(message) = reject_static_route_path_collisions(&routes, &markdown_routes) {
        eprintln!("{}", message);
        return ExitCode::from(1);
    }
    let markdown_imports = markdown_inputs
        .iter()
        .map(|input| (input.alias.clone(), input.document.clone()))
        .collect::<BTreeMap<_, _>>();

    if let Err(err) = fs::create_dir_all(&state.out_dir) {
        eprintln!(
            "failed to create static output directory `{}`: {}",
            state.out_dir.display(),
            err
        );
        return ExitCode::from(1);
    }

    let copied_css_outputs = match copy_syntax_static_asset_imports(
        &syntax_output,
        Path::new(path),
        &state.out_dir,
        &args.asset_filters,
    ) {
        Ok(paths) => paths,
        Err(message) => {
            eprintln!("{}", message);
            return ExitCode::from(1);
        }
    };
    for entrypoint in entrypoints {
        let mut html = match render_syntax_static_entrypoint(
            &syntax_output,
            &templates,
            &markdown_imports,
            &entrypoint,
        ) {
            Ok(html) => html,
            Err(StaticSyntaxRenderError::Invalid(message)) => {
                eprintln!("{}", message);
                return ExitCode::from(1);
            }
        };
        if let Some(base_path) = &args.base_path {
            html = terlan_html::inject_html_base_path(&html, base_path);
        }
        let target = state.out_dir.join(format!("{}.html", entrypoint));
        if args.validate_output {
            if let Err(message) = validate_static_html_output(&html, &target) {
                eprintln!("{}", message);
                return ExitCode::from(1);
            }
        }
        if let Err(err) = fs::write(&target, html.as_bytes()) {
            eprintln!(
                "failed to write static output `{}`: {}",
                target.display(),
                err
            );
            return ExitCode::from(1);
        }
    }

    for route in routes {
        let mut html = match render_syntax_static_entrypoint(
            &syntax_output,
            &templates,
            &markdown_imports,
            &route.handler,
        ) {
            Ok(html) => html,
            Err(StaticSyntaxRenderError::Invalid(message)) => {
                eprintln!("{}", message);
                return ExitCode::from(1);
            }
        };
        if let Some(base_path) = &args.base_path {
            html = terlan_html::inject_html_base_path(&html, base_path);
        }
        let relative_target = match static_route_output_path(&route.path) {
            Ok(path) => path,
            Err(message) => {
                eprintln!("{}", message);
                return ExitCode::from(1);
            }
        };
        let target = state.out_dir.join(relative_target);
        if let Some(parent) = target.parent() {
            if let Err(err) = fs::create_dir_all(parent) {
                eprintln!(
                    "failed to create static route output directory `{}`: {}",
                    parent.display(),
                    err
                );
                return ExitCode::from(1);
            }
        }
        if args.validate_output {
            if let Err(message) = validate_static_html_output(&html, &target) {
                eprintln!("{}", message);
                return ExitCode::from(1);
            }
        }
        if let Err(err) = fs::write(&target, html.as_bytes()) {
            eprintln!(
                "failed to write static route output `{}`: {}",
                target.display(),
                err
            );
            return ExitCode::from(1);
        }
    }

    for route in markdown_routes {
        let Some(document) = markdown_imports.get(&route.alias) else {
            eprintln!(
                "static Markdown route `{}` references unknown Markdown import `{}`",
                route.path, route.alias
            );
            return ExitCode::from(1);
        };
        let mut html = if let Some(layout) = &route.layout {
            match render_syntax_static_markdown_layout(
                &syntax_output,
                &templates,
                layout,
                route.title.as_deref(),
                document,
            ) {
                Ok(html) => html,
                Err(StaticSyntaxRenderError::Invalid(message)) => {
                    eprintln!("{}", message);
                    return ExitCode::from(1);
                }
            }
        } else {
            document.rendered_html.clone()
        };
        if let Some(base_path) = &args.base_path {
            html = terlan_html::inject_html_base_path(&html, base_path);
        }
        let relative_target = match static_route_output_path(&route.path) {
            Ok(path) => path,
            Err(message) => {
                eprintln!("{}", message);
                return ExitCode::from(1);
            }
        };
        let target = state.out_dir.join(relative_target);
        if let Some(parent) = target.parent() {
            if let Err(err) = fs::create_dir_all(parent) {
                eprintln!(
                    "failed to create static Markdown route output directory `{}`: {}",
                    parent.display(),
                    err
                );
                return ExitCode::from(1);
            }
        }
        if args.validate_output {
            if let Err(message) = validate_static_html_output(&html, &target) {
                eprintln!("{}", message);
                return ExitCode::from(1);
            }
        }
        if let Err(err) = fs::write(&target, html.as_bytes()) {
            eprintln!(
                "failed to write static Markdown route output `{}`: {}",
                target.display(),
                err
            );
            return ExitCode::from(1);
        }
    }

    if args.validate_output {
        if let Err(message) = validate_static_css_output_files(&copied_css_outputs) {
            eprintln!("{}", message);
            return ExitCode::from(1);
        }
    }

    ExitCode::SUCCESS
}

/// Parses command-local arguments for `terlc static serve`.
///
/// Inputs:
/// - `args`: arguments after the `static serve` subcommand.
///
/// Output:
/// - Parsed dev-server settings or an error message.
///
/// Transformation:
/// - Scans bind, polling, source-dir, check-only, validation, base-path,
///   asset-filter, and single file arguments.
pub(crate) fn parse_serve_static_args(args: &[String]) -> Result<ServeStaticArgs, String> {
    let mut file = None;
    let mut host = "127.0.0.1".to_string();
    let mut port = 8080;
    let mut poll_ms = 500;
    let mut source_dir = None;
    let mut check_only = false;
    let mut validate_output = false;
    let mut includes = Vec::new();
    let mut excludes = Vec::new();
    let mut base_path = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--host" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--host requires a value".to_string());
                };
                host = value.clone();
                index += 2;
            }
            "--port" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--port requires a value".to_string());
                };
                port = value
                    .parse()
                    .map_err(|_| format!("invalid --port value: {}", value))?;
                index += 2;
            }
            "--poll-ms" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--poll-ms requires a value".to_string());
                };
                poll_ms = value
                    .parse()
                    .map_err(|_| format!("invalid --poll-ms value: {}", value))?;
                if poll_ms == 0 {
                    return Err("--poll-ms must be greater than 0".to_string());
                }
                index += 2;
            }
            "--source-dir" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--source-dir requires a value".to_string());
                };
                source_dir = Some(PathBuf::from(value));
                index += 2;
            }
            "--validate-output" => {
                validate_output = true;
                index += 1;
            }
            "--base-path" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--base-path requires a path".to_string());
                };
                base_path = Some(normalize_static_base_path(value)?);
                index += 2;
            }
            "--check" => {
                check_only = true;
                index += 1;
            }
            "--asset-include" => {
                let Some(pattern) = args.get(index + 1) else {
                    return Err("--asset-include requires a pattern".to_string());
                };
                includes.push(pattern.clone());
                index += 2;
            }
            "--asset-exclude" => {
                let Some(pattern) = args.get(index + 1) else {
                    return Err("--asset-exclude requires a pattern".to_string());
                };
                excludes.push(pattern.clone());
                index += 2;
            }
            option if option.starts_with("--") => {
                return Err(format!("unsupported static serve option: {}", option));
            }
            path => {
                if file.replace(path.to_string()).is_some() {
                    return Err("static serve expects exactly one file argument".to_string());
                }
                index += 1;
            }
        }
    }

    let Some(file) = file else {
        return Err("static serve expects exactly one file argument".to_string());
    };

    let asset_filters = AssetFilters { includes, excludes };
    let emit_args = EmitStaticArgs {
        file: file.clone(),
        validate_output,
        asset_filters,
        base_path,
    };

    Ok(ServeStaticArgs {
        file,
        host,
        port,
        poll_ms,
        source_dir,
        check_only,
        emit_args,
    })
}

/// Executes the static serve command runner.
///
/// Inputs:
/// - `cmd`: parsed command containing source path and dev-server flags.
/// - `state`: global CLI state used for static output generation.
///
/// Output:
/// - `ExitCode::from(2)` for malformed arguments.
/// - `ExitCode::from(1)` for initial compile or bind failures.
/// - Otherwise this command runs until the process exits.
///
/// Transformation:
/// - Performs an initial static emit, returns early for check-only mode, starts
///   an HTTP server, polls source/output directories, recompiles on source
///   changes, and broadcasts reload events.
pub(crate) fn run_serve_static(cmd: CliCommand, state: CliState) -> ExitCode {
    let args = match parse_serve_static_args(&cmd.args) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{}", message);
            return ExitCode::from(2);
        }
    };

    let source_dir = args.source_dir.clone().unwrap_or_else(|| {
        Path::new(&args.file)
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf()
    });
    let out_dir = state.out_dir.clone();
    let exclude_dir = canonicalize_optional(&out_dir);

    eprintln!(
        "terlc static: compiling {} -> {}",
        args.file,
        out_dir.display()
    );
    if run_emit_static_with_args(&args.emit_args, state.clone()) != ExitCode::SUCCESS {
        return ExitCode::from(1);
    }
    if args.check_only {
        return ExitCode::SUCCESS;
    }

    let server = match crate::commands::serve::spawn_directory_server(
        out_dir.clone(),
        args.host.clone(),
        args.port,
        args.poll_ms,
        "terlc static",
    ) {
        Ok(server) => server,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };
    eprintln!("terlc static: shared server {}", server.local_addr);

    let poll_interval = Duration::from_millis(args.poll_ms);
    let mut source_hash = directory_fingerprint(&source_dir, exclude_dir.as_deref());

    loop {
        thread::sleep(poll_interval);

        let next_source_hash = directory_fingerprint(&source_dir, exclude_dir.as_deref());
        if next_source_hash != source_hash {
            eprintln!("terlc static: source changed; recompiling");
            source_hash = next_source_hash;
            if run_emit_static_with_args(&args.emit_args, state.clone()) != ExitCode::SUCCESS {
                eprintln!("terlc static: compile failed; keeping previous output");
                continue;
            }
        }
    }
}

/// Replays parsed static emit arguments through the command runner.
///
/// Inputs:
/// - `args`: parsed static emit arguments.
/// - `state`: global CLI state.
///
/// Output:
/// - Same exit-code contract as `run_emit_static`.
///
/// Transformation:
/// - Reconstructs command-local strings so static serve can reuse the exact
///   static emit command path.
fn run_emit_static_with_args(args: &EmitStaticArgs, state: CliState) -> ExitCode {
    let mut cmd_args = vec![args.file.clone()];
    if args.validate_output {
        cmd_args.push("--validate-output".to_string());
    }
    if let Some(base_path) = &args.base_path {
        cmd_args.push("--base-path".to_string());
        cmd_args.push(base_path.clone());
    }
    for include in &args.asset_filters.includes {
        cmd_args.push("--asset-include".to_string());
        cmd_args.push(include.clone());
    }
    for exclude in &args.asset_filters.excludes {
        cmd_args.push("--asset-exclude".to_string());
        cmd_args.push(exclude.clone());
    }

    run_emit_static(
        CliCommand {
            verb: Some("emit-static".to_string()),
            args: cmd_args,
        },
        state,
    )
}

/// Canonicalizes a path when possible.
///
/// Inputs:
/// - `path`: path to canonicalize.
///
/// Output:
/// - Canonical path or `None` when canonicalization fails.
///
/// Transformation:
/// - Converts filesystem errors into an optional result for exclusion checks.
fn canonicalize_optional(path: &Path) -> Option<PathBuf> {
    fs::canonicalize(path).ok()
}

/// Computes a coarse fingerprint for a directory tree.
///
/// Inputs:
/// - `root`: directory to fingerprint.
/// - `exclude`: optional canonical directory subtree to ignore.
///
/// Output:
/// - Hash of paths, file lengths, and modification times.
///
/// Transformation:
/// - Recursively collects files, sorts paths, and hashes metadata.
pub(crate) fn directory_fingerprint(root: &Path, exclude: Option<&Path>) -> u64 {
    let mut files = Vec::new();
    collect_directory_files(root, exclude, &mut files);
    files.sort();

    let mut hasher = DefaultHasher::new();
    root.hash(&mut hasher);
    for path in files {
        path.hash(&mut hasher);
        if let Ok(metadata) = fs::metadata(&path) {
            metadata.len().hash(&mut hasher);
            if let Ok(modified) = metadata.modified() {
                if let Ok(duration) = modified.duration_since(UNIX_EPOCH) {
                    duration.as_nanos().hash(&mut hasher);
                }
            }
        }
    }
    hasher.finish()
}

/// Collects files under a directory for fingerprinting.
///
/// Inputs:
/// - `root`: directory to scan.
/// - `exclude`: optional canonical subtree to ignore.
/// - `files`: output accumulator.
///
/// Output:
/// - No return value; mutates `files`.
///
/// Transformation:
/// - Recursively descends directories and records regular files.
fn collect_directory_files(root: &Path, exclude: Option<&Path>, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if exclude.is_some_and(|exclude| {
            fs::canonicalize(&path)
                .map(|canonical| canonical.starts_with(exclude))
                .unwrap_or(false)
        }) {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.is_dir() {
            collect_directory_files(&path, exclude, files);
        } else if metadata.is_file() {
            files.push(path);
        }
    }
}

/// Copies file and CSS imports for static output.
///
/// Inputs:
/// - `module`: syntax output containing import declarations.
/// - `source_path`: source file path used to resolve relative imports.
/// - `out_dir`: static output directory.
/// - `filters`: asset include/exclude filters.
///
/// Output:
/// - Copied CSS output paths or an error message.
///
/// Transformation:
/// - Loads and validates shared syntax asset imports, filters them, copies them
///   into the output directory, and tracks copied CSS files for validation.
fn copy_syntax_static_asset_imports(
    module: &SyntaxModuleOutput,
    source_path: &Path,
    out_dir: &Path,
    filters: &AssetFilters,
) -> Result<Vec<PathBuf>, String> {
    let mut copied_css_outputs = Vec::new();
    let imports = collect_syntax_asset_imports_matching(module, source_path, |kind, path| {
        matches!(kind, SyntaxImportKind::File | SyntaxImportKind::Css) && filters.allows(path)
    })?;

    for import in imports {
        let Some(file_name) = import.resolved_path.file_name() else {
            return Err(format!(
                "static asset import `{}` has no filename",
                import.resolved_path.display()
            ));
        };
        let target = out_dir.join(file_name);
        fs::copy(&import.resolved_path, &target).map_err(|err| {
            format!(
                "failed to copy static asset `{}` to `{}`: {}",
                import.resolved_path.display(),
                target.display(),
                err
            )
        })?;
        if import.kind == SyntaxImportKind::Css {
            copied_css_outputs.push(target);
        }
    }

    copied_css_outputs.sort();
    Ok(copied_css_outputs)
}

#[cfg(test)]
#[path = "mod_test.rs"]
mod mod_test;
