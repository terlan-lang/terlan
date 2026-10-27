//! Generates host bindings directly from local TypeScript declarations.

use super::ts_input_manifest::{
    load_ts_input_manifest, TsInputFile, TsInputGenerator, TsInputManifest, TsSourcePackage,
};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Debug, PartialEq, Eq)]
struct Options {
    input: PathBuf,
    namespace: Option<String>,
    output: PathBuf,
}

pub(super) fn run(args: &[String]) -> ExitCode {
    let options = match parse(args) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}\nterlc bind typescript --input <file.d.ts> --namespace <module.prefix> --out <dir>\nterlc bind typescript --manifest <path> --out <dir>");
            return ExitCode::from(2);
        }
    };
    let result = std::env::current_dir()
        .map_err(|error| error.to_string())
        .and_then(|root| generate(&root, &options));
    match result {
        Ok(()) => {
            println!(
                "generated TypeScript bindings from {} at {}",
                options.input.display(),
                options.output.display()
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn parse(args: &[String]) -> Result<Options, String> {
    let mut values = std::collections::BTreeMap::new();
    let mut cursor = 0;
    while cursor < args.len() {
        let key = args[cursor].as_str();
        if !matches!(key, "--input" | "--namespace" | "--manifest" | "--out") {
            return Err(format!("unexpected terlc bind typescript argument `{key}`"));
        }
        let value = args
            .get(cursor + 1)
            .filter(|value| !value.trim().is_empty() && !value.starts_with("--"))
            .ok_or_else(|| format!("{key} requires a value"))?;
        if values.insert(key, value).is_some() {
            return Err(format!("{key} can be supplied only once"));
        }
        cursor += 2;
    }
    let required = |name| {
        values
            .get(name)
            .copied()
            .ok_or_else(|| format!("terlc bind typescript requires {name}"))
    };
    let (input, namespace) = if let Some(manifest) = values.get("--manifest") {
        if values.contains_key("--input") || values.contains_key("--namespace") {
            return Err("--manifest cannot be combined with --input or --namespace".into());
        }
        (PathBuf::from(manifest), None)
    } else {
        (
            PathBuf::from(required("--input")?),
            Some(required("--namespace")?.clone()),
        )
    };
    Ok(Options {
        input,
        namespace,
        output: PathBuf::from(required("--out")?),
    })
}

fn generate(root: &Path, options: &Options) -> Result<(), String> {
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    let manifest = match &options.namespace {
        Some(namespace) => declaration_manifest(&root, &options.input, namespace)?,
        None => load_ts_input_manifest(&root, &options.input)?,
    };
    let pinned = serde_json::to_string_pretty(&manifest).map_err(|error| error.to_string())?;
    super::generate_js_dom_bindings_from_manifest(
        &root,
        Path::new("typescript-inputs.json"),
        &options.output,
        manifest,
    )?;
    std::fs::write(
        options.output.join("typescript-inputs.json"),
        format!("{pinned}\n"),
    )
    .map_err(|error| format!("cannot write TypeScript binding provenance: {error}"))
}

fn declaration_manifest(
    root: &Path,
    input_path: &Path,
    namespace: &str,
) -> Result<TsInputManifest, String> {
    let input = root.join(input_path).canonicalize().map_err(|error| {
        format!(
            "cannot read TypeScript declaration {}: {error}",
            input_path.display()
        )
    })?;
    let relative = input
        .strip_prefix(root)
        .map_err(|_| "TypeScript declaration must be inside the current directory".to_string())?;
    if !relative.to_string_lossy().ends_with(".d.ts") {
        return Err("TypeScript binding input must be a .d.ts declaration file".into());
    }
    let path = relative
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/");
    Ok(TsInputManifest {
        schema: "terlan.std.js.input-manifest.v1".into(),
        generator: TsInputGenerator {
            name: "terlc".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            profile: "typescript-declarations".into(),
            oxc_parser: true,
        },
        target_profile: "js.browser".into(),
        source_package: TsSourcePackage {
            name: namespace.into(),
            version: "local".into(),
            resolution: path.clone(),
        },
        inputs: vec![TsInputFile {
            path,
            sha256: crate::support::sha256sum_file(&input).map_err(|error| error.to_string())?,
            kind: "typescript-declaration".into(),
            namespace: namespace.into(),
        }],
    })
}

#[cfg(test)]
#[path = "typescript_test.rs"]
mod tests;
