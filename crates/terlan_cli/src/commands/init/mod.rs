use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::CliCommand;

/// Parsed command-local init arguments.
///
/// Inputs:
/// - Produced by `parse_init_args` from command-local CLI arguments.
///
/// Output:
/// - Target directory, package name, and selected scaffold profile.
///
/// Transformation:
/// - Keeps filesystem target selection separate from template rendering so the
///   writer can remain deterministic and testable.
#[derive(Debug, Clone, PartialEq, Eq)]
struct InitArgs {
    target_dir: PathBuf,
    package_name: String,
    profile: InitProfile,
}

/// Project scaffold profile selected by `terlc init`.
///
/// Inputs:
/// - Parsed from `--profile`.
///
/// Output:
/// - A small closed set of scaffold shapes.
///
/// Transformation:
/// - Keeps release-facing templates explicit so the init command does not grow
///   implicit framework behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InitProfile {
    Default,
    Web,
}

impl InitProfile {
    /// Parses a profile name.
    ///
    /// Inputs:
    /// - `value`: command-line profile value.
    ///
    /// Output:
    /// - Matching `InitProfile` or a stable command error.
    ///
    /// Transformation:
    /// - Accepts only reviewed scaffold names.
    fn parse(value: &str) -> Result<Self, String> {
        if value == "default" {
            return Ok(Self::Default);
        }
        if value == "web" {
            return Ok(Self::Web);
        }
        Err(format!(
            "unsupported init profile `{value}`; supported profiles: default, web"
        ))
    }
}

/// Executes the `init` CLI command.
///
/// Inputs:
/// - `cmd`: parsed CLI command containing one project path/name.
///
/// Output:
/// - `ExitCode::SUCCESS` when the project scaffold is written.
/// - `ExitCode::from(2)` for malformed command arguments or invalid package
///   names.
/// - `ExitCode::from(1)` for filesystem errors or overwrite refusal.
///
/// Transformation:
/// - Resolves the package name, target directory, and scaffold profile,
///   validates the package identity, writes `terlan.toml`, and writes profile
///   source/test files.
pub(crate) fn run(cmd: CliCommand) -> ExitCode {
    let args = match parse_init_args(&cmd.args) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            crate::print_usage();
            return ExitCode::from(2);
        }
    };

    match write_project(&args) {
        Ok(()) => {
            println!("Created Terlan project `{}`.", args.package_name);
            println!("Next steps:");
            println!("  cd {}", args.target_dir.display());
            for step in next_steps(args.profile, &args.package_name) {
                println!("  {step}");
            }
            println!("  terlc test");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}

/// Parses command-local arguments for `terlc init`.
///
/// Inputs:
/// - `args`: raw command arguments after global parsing.
///
/// Output:
/// - `Ok(InitArgs)` for exactly one positional project name/path and optional
///   `--profile <default|web>`.
/// - `Err(message)` for unsupported flags, missing project name, too many
///   arguments, missing profile value, invalid profile, or invalid package
///   names.
///
/// Transformation:
/// - Converts the selected path into a target directory and package identity,
///   while keeping the selected scaffold profile explicit.
fn parse_init_args(args: &[String]) -> Result<InitArgs, String> {
    let mut target = None;
    let mut profile = InitProfile::Default;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--profile" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("terlc init --profile requires a value".to_string());
                };
                profile = InitProfile::parse(value)?;
            }
            option if option.starts_with('-') => {
                return Err(format!("unsupported init option: {option}"));
            }
            path => {
                if target.replace(path.to_string()).is_some() {
                    return Err("terlc init requires one new project name".to_string());
                }
            }
        }
        index += 1;
    }

    let Some(target) = target else {
        return Err("terlc init requires one new project name".to_string());
    };

    let target_dir = PathBuf::from(target);
    let package_name = package_name_from_path(&target_dir)?;
    validate_package_name(&package_name)?;

    Ok(InitArgs {
        target_dir,
        package_name,
        profile,
    })
}

/// Extracts the package name from a target path.
///
/// Inputs:
/// - `path`: directory selected for project initialization.
///
/// Output:
/// - Final path component as UTF-8 text.
/// - `Err(message)` when no valid directory name can be read.
///
/// Transformation:
/// - Reads only the final path segment; it does not touch the filesystem.
fn package_name_from_path(path: &Path) -> Result<String, String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            format!(
                "cannot derive package name from init path `{}`",
                path.display()
            )
        })
}

/// Validates a 0.0.1 package name.
///
/// Inputs:
/// - `name`: candidate package name.
///
/// Output:
/// - `Ok(())` when the name can appear in `terlan.toml`.
/// - `Err(message)` when the name is not supported by 0.0.1 project layout.
///
/// Transformation:
/// - Enforces the same conservative package-root shape used by project builds:
///   lowercase ASCII leading letter followed by lowercase ASCII letters,
///   digits, `_`, or `-`.
fn validate_package_name(name: &str) -> Result<(), String> {
    let mut chars = name.chars();
    match chars.next() {
        Some(ch) if ch.is_ascii_lowercase() => {}
        _ => {
            return Err(format!(
                "invalid package name `{name}`: must start with a lowercase ASCII letter"
            ));
        }
    }
    if !chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-') {
        return Err(format!(
            "invalid package name `{name}`: use lowercase letters, digits, `_`, or `-`"
        ));
    }
    Ok(())
}

/// Converts a package name to the source package root.
///
/// Inputs:
/// - `package_name`: validated package name.
///
/// Output:
/// - Source package root used in module paths and directories.
///
/// Transformation:
/// - Replaces `-` with `_` because Terlan source path segments are identifiers.
fn source_package_root(package_name: &str) -> String {
    package_name.replace('-', "_")
}

/// Writes the project scaffold.
///
/// Inputs:
/// - `args`: validated target directory and package name.
///
/// Output:
/// - `Ok(())` when all files are created.
/// - `Err(message)` when the target directory already exists or the
///   filesystem write fails.
///
/// Transformation:
/// - Refuses existing project directories, creates the target directory, then
///   writes deterministic files for the selected profile.
fn write_project(args: &InitArgs) -> Result<(), String> {
    let source_root = source_package_root(&args.package_name);
    let manifest_path = args.target_dir.join("terlan.toml");
    let main_path = args
        .target_dir
        .join("src")
        .join(&source_root)
        .join("Main.terl");
    let test_path = args
        .target_dir
        .join("tests")
        .join(&source_root)
        .join("main_test.terl");
    let web_path = args
        .target_dir
        .join("src")
        .join(&source_root)
        .join("Web.terl");
    let http_path = args
        .target_dir
        .join("src")
        .join(&source_root)
        .join("Http.terl");
    let assets_path = args.target_dir.join("assets");

    refuse_existing_project_dir(&args.target_dir)?;

    fs::create_dir_all(main_path.parent().expect("Main.terl always has parent")).map_err(
        |err| {
            format!(
                "cannot create source directory {}: {err}",
                main_path
                    .parent()
                    .expect("Main.terl always has parent")
                    .display()
            )
        },
    )?;
    fs::create_dir_all(
        test_path
            .parent()
            .expect("main_test.terl always has parent"),
    )
    .map_err(|err| {
        format!(
            "cannot create test directory {}: {err}",
            test_path
                .parent()
                .expect("main_test.terl always has parent")
                .display()
        )
    })?;

    fs::write(
        &manifest_path,
        render_manifest(&args.package_name, args.profile),
    )
    .map_err(|err| format!("cannot write {}: {err}", manifest_path.display()))?;
    fs::write(&main_path, render_main_module(&source_root))
        .map_err(|err| format!("cannot write {}: {err}", main_path.display()))?;
    fs::write(&test_path, render_test_module(&source_root))
        .map_err(|err| format!("cannot write {}: {err}", test_path.display()))?;
    if args.profile == InitProfile::Web {
        fs::create_dir_all(&assets_path).map_err(|err| {
            format!(
                "cannot create asset directory {}: {err}",
                assets_path.display()
            )
        })?;
        fs::write(&web_path, render_web_module(&source_root))
            .map_err(|err| format!("cannot write {}: {err}", web_path.display()))?;
        fs::write(&http_path, render_http_handler_module(&source_root))
            .map_err(|err| format!("cannot write {}: {err}", http_path.display()))?;
    }
    Ok(())
}

/// Refuses to scaffold into an existing project directory.
///
/// Inputs:
/// - `path`: requested project directory.
///
/// Output:
/// - `Ok(())` when the directory path does not exist.
/// - `Err(message)` when the path already exists.
///
/// Transformation:
/// - Checks filesystem existence without opening or modifying the path.
fn refuse_existing_project_dir(path: &Path) -> Result<(), String> {
    if path.exists() {
        return Err(format!(
            "terlc init refuses to write into existing directory: {}",
            path.display()
        ));
    }
    Ok(())
}

/// Renders the project manifest.
///
/// Inputs:
/// - `package_name`: validated package name.
/// - `profile`: scaffold profile controlling optional project sections.
///
/// Output:
/// - Complete `terlan.toml` text.
///
/// Transformation:
/// - Formats the minimal manifest-backed `beam-thin` project contract and adds
///   the Terlan-owned web asset section for browser scaffolds.
fn render_manifest(package_name: &str, profile: InitProfile) -> String {
    let mut manifest = format!(
        "[package]\nname = \"{package_name}\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n"
    );
    if profile == InitProfile::Web {
        manifest.push_str("\n[web.assets]\ndirectory = \"assets\"\n");
    }
    manifest
}

/// Renders the hello-world main module.
///
/// Inputs:
/// - `source_root`: source package root after package-name normalization.
///
/// Output:
/// - Complete `src/<package_root>/Main.terl` text.
///
/// Transformation:
/// - Emits the 0.0.1 entrypoint shape `<package_root>.Main.main(): Unit` using
///   an explicit import for portable `std.io.Console.println`.
fn render_main_module(source_root: &str) -> String {
    format!(
        "module {source_root}.Main.\n\nimport std.io.Console.{{println}}.\n\npub main(): Unit ->\n    println(\"hello from Terlan\").\n"
    )
}

/// Renders the browser-side web module for the web profile.
///
/// Inputs:
/// - `source_root`: source package root after package-name normalization.
///
/// Output:
/// - Complete `src/<package_root>/Web.terl` text.
///
/// Transformation:
/// - Emits a minimal Terlan module that can be compiled by the JavaScript
///   browser target without depending on DOM bindings or generated assets.
fn render_web_module(source_root: &str) -> String {
    format!(
        "module {source_root}.Web.\n\npub message(): String ->\n    \"hello from Terlan web\".\n"
    )
}

/// Renders the BEAM-backed HTTP handler module for the web profile.
///
/// Inputs:
/// - `source_root`: source package root after package-name normalization.
///
/// Output:
/// - Complete `src/<package_root>/Http.terl` text.
///
/// Transformation:
/// - Emits the current internal server bridge shape so generated web projects
///   have a real Terlan handler module before the public `std.http` adapter
///   hides the tuple protocol.
fn render_http_handler_module(source_root: &str) -> String {
    format!(
        "module {source_root}.Http.\n\npub handle(request: Dynamic): Dynamic ->\n    {{Atom[\"terlan_response\"], 200, \"text/plain; charset=utf-8\", \"hello from Terlan handler\"}}.\n"
    )
}

/// Renders the sample test module.
///
/// Inputs:
/// - `source_root`: source package root after package-name normalization.
///
/// Output:
/// - Complete `tests/<package_root>/main_test.terl` text.
///
/// Transformation:
/// - Emits one annotation-based 0.0.1 test using `std.test.Test` and a
///   compiler-known `String` receiver method so generated projects demonstrate
///   both build and test entry points.
fn render_test_module(source_root: &str) -> String {
    format!(
        "module {source_root}.MainTest.\n\n@test\npub hello_text_is_stable(): Bool ->\n    std.test.Test.assert_equal(\"hello from Terlan\", \"hello from Terlan\".to_string()).\n"
    )
}

/// Returns user-facing next steps for a scaffold profile.
///
/// Inputs:
/// - `profile`: selected scaffold profile.
/// - `package_name`: validated package name used for executable hints.
///
/// Output:
/// - Ordered CLI commands to print after successful initialization.
///
/// Transformation:
/// - Keeps default projects focused on BEAM executable output and web projects
///   focused on the browser package plus local server loop.
fn next_steps(profile: InitProfile, package_name: &str) -> Vec<String> {
    match profile {
        InitProfile::Default => vec![
            "terlc build".to_string(),
            format!("./_build/bin/{package_name}"),
        ],
        InitProfile::Web => vec![
            "terlc build --target erlang".to_string(),
            "terlc build --target js.browser".to_string(),
            "terlc serve".to_string(),
        ],
    }
}

#[cfg(test)]
#[path = "init_test.rs"]
mod init_test;
