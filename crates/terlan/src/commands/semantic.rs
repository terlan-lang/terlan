//! Thin developer-toolchain launcher for the Terlan semantic service.
//! MCP, schema admission, queries and resource policy remain Terlan code.

use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};

use crate::{CliCommand, CliState};

fn service_home() -> Result<PathBuf, String> {
    if let Some(configured) = std::env::var_os("TERLAN_SEMANTIC_HOME") {
        let home = PathBuf::from(configured);
        if home.join("terlan.toml").is_file() && home.join("schemas/query.schema.json").is_file() {
            return Ok(home);
        }
        return Err("TERLAN_SEMANTIC_HOME must name the compiler/semantic package".into());
    }
    let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
    for ancestor in cwd.ancestors() {
        let home = ancestor.join("compiler/semantic");
        if home.join("terlan.toml").is_file() {
            return Ok(home);
        }
    }
    Err("semantic service sources not found; set TERLAN_SEMANTIC_HOME to compiler/semantic".into())
}

pub(crate) fn run(cmd: CliCommand, mut state: CliState) -> ExitCode {
    let result = (|| -> Result<ExitCode, String> {
        let [operation, store, selection] = cmd.args.as_slice() else {
            return Err("usage: terlc semantic query <store> <request.json> | terlc semantic mcp <store> <snapshot-id>".into());
        };
        let script = match operation.as_str() {
            "query" => "Query.terls",
            "mcp" => "Mcp.terls",
            _ => return Err("semantic operation must be query or mcp".into()),
        };
        if state.target_profile != crate::TargetProfile::Vm || state.no_emit {
            return Err(
                "semantic service execution requires the VM target and artifact emission".into(),
            );
        }
        let home = service_home()?
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let source = home.join("scripts").join(script);
        state.out_dir = state.out_dir.join("semantic").join(operation);
        let mut build = Command::new(std::env::current_exe().map_err(|error| error.to_string())?);
        build
            .arg("build")
            .arg(&source)
            .arg("--out-dir")
            .arg(&state.out_dir)
            .args(["--diagnostic-format", "text", "--color", "never"])
            .stdin(Stdio::null());
        if let Some(cache) = &state.cache_dir {
            build.arg("--cache-dir").arg(cache);
        }
        // Compiler output goes to stderr, including failed admission/builds.
        // Only the successfully built Terlan application owns protocol stdout.
        let output = build
            .output()
            .map_err(|error| format!("cannot build semantic service: {error}"))?;
        let mut diagnostics = io::stderr().lock();
        diagnostics
            .write_all(&output.stdout)
            .and_then(|()| diagnostics.write_all(&output.stderr))
            .map_err(|error| format!("cannot write semantic build diagnostics: {error}"))?;
        if !output.status.success() {
            return Ok(ExitCode::from(1));
        }
        let mut arguments = vec![store.clone(), selection.clone()];
        if operation == "mcp" {
            arguments.push(home.join("schemas").to_string_lossy().into_owned());
        }
        super::run::run_prebuilt_source_stdio(&state, &source, &arguments)
    })();
    match result {
        Ok(status) => status,
        Err(message) => {
            eprintln!("error[semantic.launch]: {message}");
            ExitCode::from(1)
        }
    }
}
