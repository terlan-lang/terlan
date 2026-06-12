use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::CliCommand;

/// Parsed command-local init arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
struct InitArgs {
    target_dir: PathBuf,
    package_name: String,
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
/// - Resolves the package name and target directory, validates the package
///   identity, writes `terlan.toml`, and writes a target-neutral
///   `src/<package_root>/Main.tl` hello-world module plus a sample
///   `tests/<package_root>/main_test.tl` test module.
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
            println!("  terlc build");
            println!("  ./_build/bin/{}", args.package_name);
            println!(
                "  terlc test tests/{}/main_test.tl",
                source_package_root(&args.package_name)
            );
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
/// - `Ok(InitArgs)` for exactly one positional project name/path.
/// - `Err(message)` for flags, missing project name, too many arguments, or
///   invalid package names.
///
/// Transformation:
/// - Converts the selected path into a target directory and package identity.
fn parse_init_args(args: &[String]) -> Result<InitArgs, String> {
    if args.iter().any(|arg| arg.starts_with("--")) {
        return Err("terlc init does not accept options in 0.0.1".to_string());
    }
    if args.len() != 1 {
        return Err("terlc init requires one new project name".to_string());
    }

    let target_dir = PathBuf::from(&args[0]);
    let package_name = package_name_from_path(&target_dir)?;
    validate_package_name(&package_name)?;

    Ok(InitArgs {
        target_dir,
        package_name,
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
/// - Refuses existing project directories, creates the target directory and
///   package source/test directories, then writes deterministic 0.0.1
///   `terlan.toml`, `Main.tl`, and `main_test.tl` files.
fn write_project(args: &InitArgs) -> Result<(), String> {
    let source_root = source_package_root(&args.package_name);
    let manifest_path = args.target_dir.join("terlan.toml");
    let main_path = args
        .target_dir
        .join("src")
        .join(&source_root)
        .join("Main.tl");
    let test_path = args
        .target_dir
        .join("tests")
        .join(&source_root)
        .join("main_test.tl");

    refuse_existing_project_dir(&args.target_dir)?;

    fs::create_dir_all(main_path.parent().expect("Main.tl always has parent")).map_err(|err| {
        format!(
            "cannot create source directory {}: {err}",
            main_path
                .parent()
                .expect("Main.tl always has parent")
                .display()
        )
    })?;
    fs::create_dir_all(test_path.parent().expect("main_test.tl always has parent")).map_err(
        |err| {
            format!(
                "cannot create test directory {}: {err}",
                test_path
                    .parent()
                    .expect("main_test.tl always has parent")
                    .display()
            )
        },
    )?;

    fs::write(&manifest_path, render_manifest(&args.package_name))
        .map_err(|err| format!("cannot write {}: {err}", manifest_path.display()))?;
    fs::write(&main_path, render_main_module(&source_root))
        .map_err(|err| format!("cannot write {}: {err}", main_path.display()))?;
    fs::write(&test_path, render_test_module(&source_root))
        .map_err(|err| format!("cannot write {}: {err}", test_path.display()))?;
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
///
/// Output:
/// - Complete `terlan.toml` text.
///
/// Transformation:
/// - Formats the minimal 0.0.1 manifest-backed `beam-thin` project contract.
fn render_manifest(package_name: &str) -> String {
    format!(
        "[package]\nname = \"{package_name}\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n"
    )
}

/// Renders the hello-world main module.
///
/// Inputs:
/// - `source_root`: source package root after package-name normalization.
///
/// Output:
/// - Complete `src/<package_root>/Main.tl` text.
///
/// Transformation:
/// - Emits the 0.0.1 entrypoint shape `<package_root>.Main.main(): Unit` using
///   portable `std.io.Console.println`.
fn render_main_module(source_root: &str) -> String {
    format!(
        "module {source_root}.Main.\n\npub main(): Unit ->\n    std.io.Console.println(\"hello from Terlan\").\n"
    )
}

/// Renders the sample test module.
///
/// Inputs:
/// - `source_root`: source package root after package-name normalization.
///
/// Output:
/// - Complete `tests/<package_root>/main_test.tl` text.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Creates a unique temporary test directory.
    ///
    /// Inputs:
    /// - `name`: readable test stem.
    ///
    /// Output:
    /// - Path to a not-yet-existing directory under the system temp directory.
    ///
    /// Transformation:
    /// - Combines process id and current nanoseconds to avoid collisions.
    fn temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("timestamp")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "terlan_init_{name}_{}_{}",
            std::process::id(),
            nanos
        ))
    }

    #[test]
    fn parse_init_args_accepts_named_project() {
        let parsed = parse_init_args(&["hello".to_string()]).expect("parse init args");

        assert_eq!(parsed.target_dir, PathBuf::from("hello"));
        assert_eq!(parsed.package_name, "hello");
    }

    #[test]
    fn parse_init_args_rejects_missing_project_name() {
        let err = parse_init_args(&[]).expect_err("missing project name");

        assert!(err.contains("requires one new project name"));
    }

    #[test]
    fn parse_init_args_rejects_invalid_package_name() {
        let err = parse_init_args(&["Hello".to_string()]).expect_err("invalid package");

        assert!(err.contains("must start with a lowercase ASCII letter"));
    }

    #[test]
    fn render_manifest_uses_release_project_contract() {
        assert_eq!(
            render_manifest("hello"),
            "[package]\nname = \"hello\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n"
        );
    }

    #[test]
    fn write_project_creates_manifest_and_main_module() {
        let dir = temp_dir("writes_project");
        let args = InitArgs {
            target_dir: dir.clone(),
            package_name: "hello".to_string(),
        };

        write_project(&args).expect("write project");

        assert_eq!(
            fs::read_to_string(dir.join("terlan.toml")).expect("manifest"),
            render_manifest("hello")
        );
        assert_eq!(
            fs::read_to_string(dir.join("src/hello/Main.tl")).expect("main module"),
            render_main_module("hello")
        );
        assert_eq!(
            fs::read_to_string(dir.join("tests/hello/main_test.tl")).expect("test module"),
            render_test_module("hello")
        );
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn write_project_normalizes_hyphenated_source_root() {
        let dir = temp_dir("hyphen_project");
        let args = InitArgs {
            target_dir: dir.clone(),
            package_name: "hello-app".to_string(),
        };

        write_project(&args).expect("write project");

        assert!(dir.join("src/hello_app/Main.tl").exists());
        assert!(dir.join("tests/hello_app/main_test.tl").exists());
        assert!(fs::read_to_string(dir.join("src/hello_app/Main.tl"))
            .expect("main module")
            .contains("module hello_app.Main."));
        assert!(fs::read_to_string(dir.join("tests/hello_app/main_test.tl"))
            .expect("test module")
            .contains("module hello_app.MainTest."));
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn write_project_refuses_existing_directory() {
        let dir = temp_dir("refuse_directory");
        fs::create_dir_all(&dir).expect("create dir");
        let args = InitArgs {
            target_dir: dir.clone(),
            package_name: "hello".to_string(),
        };

        let err = write_project(&args).expect_err("should refuse overwrite");

        assert!(err.contains("refuses to write into existing directory"));
        fs::remove_dir_all(dir).expect("cleanup");
    }
}
