//! Loads generated TypeScript host contracts for the selected JavaScript build.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::OnceLock,
};

#[derive(serde::Deserialize)]
struct Manifest {
    outputs: Vec<Module>,
}

#[derive(serde::Deserialize)]
struct Module {
    module: String,
    #[serde(default)]
    summary: Option<PathBuf>,
    #[serde(default)]
    members: Vec<Member>,
    #[serde(default)]
    typescript_type: Option<TypeScriptType>,
}

#[derive(Clone, serde::Deserialize)]
pub(crate) struct TypeScriptType {
    pub(crate) source: PathBuf,
    pub(crate) name: String,
}

#[derive(Clone, serde::Deserialize)]
pub(crate) struct Member {
    pub(crate) name: String,
    pub(crate) js_name: String,
    pub(crate) kind: String,
    pub(crate) arity: usize,
    pub(crate) return_type: String,
    #[serde(default)]
    pub(crate) parameters: Vec<Parameter>,
    #[serde(default)]
    pub(crate) js_names: Vec<String>,
    #[serde(default)]
    pub(crate) undefined_only: bool,
}

#[derive(Clone, serde::Deserialize)]
pub(crate) struct Parameter {
    #[serde(default)]
    pub(crate) undefined_only: bool,
    pub(crate) optional: bool,
    #[serde(default)]
    pub(crate) rest: bool,
    #[serde(rename = "type")]
    pub(crate) ty: String,
}

#[derive(Default)]
pub(crate) struct Registry {
    pub(crate) modules: BTreeMap<String, Vec<Member>>,
    summaries: BTreeMap<String, PathBuf>,
    typescript_types: BTreeMap<String, TypeScriptType>,
}

/// Loads the bundled DOM contracts and optional generated package manifests.
pub(crate) fn registry() -> Result<&'static Registry, String> {
    static REGISTRY: OnceLock<Result<Registry, String>> = OnceLock::new();
    REGISTRY
        .get_or_init(|| {
            let mut registry = Registry::default();
            registry.merge(
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../std/js/manifests/std_js_bindings.json"
                )),
                None,
            )?;
            if let Some(paths) = std::env::var_os("TERLAN_JS_BINDING_MANIFESTS") {
                for path in std::env::split_paths(&paths) {
                    let source = std::fs::read_to_string(&path).map_err(|error| {
                        format!(
                            "cannot read JavaScript binding manifest `{}`: {error}",
                            path.display()
                        )
                    })?;
                    registry.merge(&source, Some(&path))?;
                }
            }
            Ok(registry)
        })
        .as_ref()
        .map_err(Clone::clone)
}

/// Returns a package-generated summary before consulting bundled interfaces.
pub(crate) fn summary_path(module: &str) -> Option<PathBuf> {
    registry().ok()?.summaries.get(module).cloned()
}

pub(crate) fn typescript_type(module: &str) -> Option<TypeScriptType> {
    registry().ok()?.typescript_types.get(module).cloned()
}

impl Registry {
    fn merge(&mut self, source: &str, manifest_path: Option<&Path>) -> Result<(), String> {
        let manifest: Manifest = serde_json::from_str(source)
            .map_err(|error| format!("invalid JavaScript binding manifest: {error}"))?;
        let mut seen = std::collections::BTreeSet::new();
        for module in manifest.outputs {
            if !seen.insert(module.module.clone()) {
                return Err(format!(
                    "duplicate JavaScript binding module `{}`",
                    module.module
                ));
            }
            for member in &module.members {
                let expected = match member.kind.as_str() {
                    "property" => 1,
                    "setter"
                        if member.parameters.len() == 1
                            && member.js_names.len() == 1
                            && member.js_names[0] == member.js_name =>
                    {
                        2
                    }
                    "record"
                        if member.js_names.len() == member.parameters.len()
                            && member
                                .js_names
                                .iter()
                                .collect::<std::collections::BTreeSet<_>>()
                                .len()
                                == member.js_names.len() =>
                    {
                        member.parameters.len()
                    }
                    "method" => member.parameters.len() + 1,
                    "mixin" if member.js_names.len() == member.parameters.len() => {
                        member.parameters.len() + 1
                    }
                    other => return Err(format!("unknown JavaScript binding kind `{other}`")),
                };
                if member.arity != expected {
                    return Err(format!("invalid JavaScript binding arity for `{}.{}`: expected {expected}, found {}", module.module, member.name, member.arity));
                }
            }
            if let (Some(path), Some(relative)) = (manifest_path, module.summary) {
                if relative.is_absolute()
                    || relative
                        .components()
                        .any(|part| matches!(part, std::path::Component::ParentDir))
                {
                    return Err(format!(
                        "JavaScript binding summary must be package-relative: `{}`",
                        relative.display()
                    ));
                }
                let summary = path
                    .ancestors()
                    .skip(1)
                    .map(|root| root.join(&relative))
                    .find(|candidate| candidate.is_file())
                    .ok_or_else(|| {
                        format!(
                            "missing JavaScript binding summary `{}` for `{}`",
                            relative.display(),
                            module.module
                        )
                    })?;
                self.summaries.insert(module.module.clone(), summary);
            }
            if let (Some(path), Some(mut declaration)) = (manifest_path, module.typescript_type) {
                if declaration.source.is_absolute()
                    || declaration
                        .source
                        .components()
                        .any(|part| matches!(part, std::path::Component::ParentDir))
                {
                    return Err("TypeScript source must be package-relative".into());
                }
                declaration.source = path
                    .ancestors()
                    .skip(1)
                    .map(|root| root.join(&declaration.source))
                    .find(|candidate| candidate.is_file())
                    .ok_or_else(|| format!("missing TypeScript source for `{}`", module.module))?
                    .canonicalize()
                    .map_err(|error| error.to_string())?;
                self.typescript_types
                    .insert(module.module.clone(), declaration);
            }
            self.modules.insert(module.module, module.members);
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "js_binding_registry_inline_test.rs"]
mod tests;
