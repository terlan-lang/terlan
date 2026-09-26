use std::env;
use std::fs;
use std::process::ExitCode;

use serde::Serialize;
use serde_json::{json, Value};
use terlan::compiler::syntax::parse_module_as_syntax_output;

#[derive(Serialize)]
struct SurfaceDeclaration {
    class: String,
    symbol_name: String,
    is_public: bool,
}

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("terlan-surface-oracle: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: Vec<String>) -> Result<(), String> {
    let [input, output] = arguments.as_slice() else {
        return Err("usage: terlan-surface-oracle INPUT OUTPUT".to_owned());
    };
    let source =
        fs::read_to_string(input).map_err(|error| format!("cannot read {input}: {error}"))?;
    let module = parse_module_as_syntax_output(&source)
        .map_err(|error| format!("cannot parse module {input}: {error:?}"))?;
    let declarations = module
        .declarations
        .into_iter()
        .map(|declaration| {
            let payload = serde_json::to_value(declaration.payload)
                .map_err(|error| format!("cannot normalize declaration payload: {error}"))?;
            Ok(SurfaceDeclaration {
                symbol_name: symbol_name(&declaration.class, &payload),
                is_public: is_public(&declaration.class, &payload),
                class: declaration.class,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let artifact = json!({
        "schema": "terlan.self_host.surface.v1",
        "valid": true,
        "module_name": module.module_name,
        "declarations": declarations,
    });
    let encoded = serde_json::to_vec_pretty(&artifact)
        .map_err(|error| format!("cannot encode surface artifact: {error}"))?;
    fs::write(output, encoded).map_err(|error| format!("cannot write {output}: {error}"))
}

fn symbol_name(class: &str, payload: &Value) -> String {
    if class == "ImportDecl" {
        return String::new();
    }
    ["name", "module_name", "trait_name", "target_name"]
        .into_iter()
        .find_map(|field| payload.get(field).and_then(Value::as_str))
        .or_else(|| {
            (class == "ShapeDecl")
                .then(|| raw_shape_name(payload))
                .flatten()
        })
        .unwrap_or("")
        .to_owned()
}

fn is_public(class: &str, payload: &Value) -> bool {
    payload
        .get("is_public")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| {
            class == "ShapeDecl"
                && payload
                    .get("text")
                    .and_then(Value::as_str)
                    .is_some_and(|text| text.trim_start().starts_with("pub shape "))
        })
}

fn raw_shape_name(payload: &Value) -> Option<&str> {
    let text = payload.get("text")?.as_str()?.trim_start();
    let shape = text
        .strip_prefix("pub ")
        .unwrap_or(text)
        .strip_prefix("shape ")?;
    let end = shape
        .find(|character: char| character == '(' || character == '[' || character.is_whitespace())
        .unwrap_or(shape.len());
    Some(&shape[..end])
}
