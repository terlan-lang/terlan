//! Validation of source interface dependencies before trusting their signatures.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use crate::terlan_hir::{parse_interface_text, syntax_module_output_to_interface, ModuleInterface};
use crate::terlan_syntax::{parse_interface_module_as_syntax_output, SyntaxModuleOutput};
use crate::terlan_typeck::{DiagSeverity, Diagnostic};

use super::interface_contract::{
    expand_interface_graph, prepare_interface_module, read_declared_interface,
};
use super::interface_loading::source_dependency_modules;

/// Checks only the reachable source interface graph. Unrelated unfinished
/// interfaces do not prevent independent module work; generated `.typi`
/// dependencies continue to use their checked-summary contract.
pub(crate) fn check_interface_dependencies(
    path: &Path,
    module: &SyntaxModuleOutput,
    cache_dir: Option<&Path>,
    interfaces: &mut HashMap<String, ModuleInterface>,
    source_graph: Option<&super::interface_graph::SourceInterfaceGraph>,
) -> Vec<Diagnostic> {
    let mut required = source_dependency_modules(module);
    if path
        .extension()
        .is_some_and(|extension| extension == "terl")
    {
        if let Ok(Some(contract)) = read_declared_interface(path) {
            required.extend(source_dependency_modules(&contract));
        }
    }
    if required.is_empty() {
        return Vec::new();
    }
    let mut catalog = HashMap::new();
    let mut malformed = Vec::new();
    // Match loader precedence: adjacent handwritten interfaces override caches.
    let directories = source_graph.map_or_else(
        || {
            BTreeSet::from([path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or(Path::new("."))
                .to_path_buf()])
        },
        |graph| graph.directories.clone(),
    );
    let roots = cache_dir
        .into_iter()
        .chain(directories.iter().map(|path| path.as_path()));
    for (precedence, root) in roots.enumerate() {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        let mut files = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|file| {
                file.extension()
                    .is_some_and(|extension| extension == "terli")
            })
            .collect::<Vec<_>>();
        files.sort();
        for file in files {
            let source = match std::fs::read_to_string(&file) {
                Ok(source) => source,
                Err(error) => {
                    malformed.push((
                        precedence,
                        file,
                        format!("cannot read dependency interface: {error}"),
                    ));
                    continue;
                }
            };
            if let Some((name, _)) = parse_interface_text(&source) {
                catalog.insert(name, (precedence, file, source));
            } else {
                malformed.push((
                    precedence,
                    file,
                    "invalid dependency interface syntax".to_string(),
                ));
            }
        }
    }
    let mut visited = BTreeSet::from([module.module_name.clone()]);
    // Project and batch sources are checked by their owner. Cached copies of
    // those identities cannot replace the source graph supplied by the owner.
    if let Some(graph) = source_graph {
        visited.extend(graph.source_modules.iter().cloned());
    }
    let mut selected = Vec::new();
    let mut diagnostics = Vec::new();
    while let Some(name) = required.pop_first() {
        if !visited.insert(name.clone()) {
            continue;
        }
        for (precedence, file, message) in &malformed {
            if catalog
                .get(&name)
                .is_some_and(|(selected, _, _)| selected > precedence)
            {
                continue;
            }
            if file
                .file_stem()
                .and_then(|stem| stem.to_str())
                .is_some_and(|stem| name == stem || name.ends_with(&format!(".{stem}")))
            {
                diagnostics.push(error(module, file, message));
            }
        }
        let Some((_, file, source)) = catalog.remove(&name) else {
            continue;
        };
        match parse_interface_module_as_syntax_output(&source) {
            Ok(syntax) => {
                required.extend(source_dependency_modules(&syntax));
                for (dependency, interface) in
                    super::interface_loading::load_external_interfaces_for_module(
                        &file.to_string_lossy(),
                        cache_dir,
                        &syntax,
                    )
                {
                    interfaces.entry(dependency).or_insert(interface);
                }
                interfaces.insert(name, syntax_module_output_to_interface(&syntax));
                selected.push((file, syntax));
            }
            Err(message) => diagnostics.push(error(
                module,
                &file,
                &format!("invalid dependency interface: {message:?}"),
            )),
        }
    }
    let modules = selected
        .iter()
        .map(|(_, syntax)| syntax.clone())
        .collect::<Vec<_>>();
    expand_interface_graph(&modules, interfaces);
    for (file, syntax) in selected {
        let (_, _, errors) = prepare_interface_module(syntax, interfaces);
        diagnostics.extend(errors.into_iter().map(|diagnostic| Diagnostic {
            span: module.span.into(),
            message: format!("{}: {}", file.display(), diagnostic.message),
            severity: diagnostic.severity,
        }));
    }
    diagnostics
}

fn error(module: &SyntaxModuleOutput, path: &Path, message: &str) -> Diagnostic {
    Diagnostic {
        span: module.span.into(),
        message: format!("{}: {message}", path.display()),
        severity: DiagSeverity::Error,
    }
}
