use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use serde::Serialize;
use terlan::compiler::syntax::{
    parse_expr_as_syntax_output, parse_interface_module_as_syntax_output,
    parse_module_as_syntax_output, parse_script_as_syntax_output,
};

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("terlan-syntax-oracle: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let [kind, input, output] = arguments.as_slice() else {
        return Err(("usage: terlan-syntax-oracle KIND INPUT OUTPUT".to_owned()).into());
    };
    let source =
        fs::read_to_string(input).map_err(|error| format!("cannot read {input}: {error}"))?;
    match kind.as_str() {
        "module" => write_output(
            output,
            &parse_module_as_syntax_output(&source)
                .map_err(|error| format!("cannot parse module {input}: {error:?}"))?,
        ),
        "interface" => write_output(
            output,
            &parse_interface_module_as_syntax_output(&source)
                .map_err(|error| format!("cannot parse interface {input}: {error:?}"))?,
        ),
        "script" => write_output(
            output,
            &parse_script_as_syntax_output(&source, script_module_name(input))
                .map_err(|error| format!("cannot parse script {input}: {error:?}"))?,
        ),
        "expression" => write_output(
            output,
            &parse_expr_as_syntax_output(&source)
                .map_err(|error| format!("cannot parse expression {input}: {error:?}"))?,
        ),
        _ => Err((format!(
            "unknown syntax kind {kind:?}; expected module, interface, script, or expression"
        ))
        .into()),
    }
}

fn script_module_name(input: &str) -> &str {
    Path::new(input)
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("script")
}

fn write_output<T: Serialize>(path: &str, value: &T) -> Result<(), Box<dyn std::error::Error>> {
    let encoded = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("cannot encode normalized syntax output: {error}"))?;
    Ok(fs::write(path, encoded).map_err(|error| format!("cannot write {path}: {error}"))?)
}
