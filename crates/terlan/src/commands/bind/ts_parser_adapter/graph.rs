//! Resolves local declaration imports before binding plans are constructed.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::super::ts_input_manifest::{TsInputFile, TsInputManifest};
use super::super::ts_type_mapping::TsTypeRef;
use super::declarations::*;
use oxc_ast::ast::*;

#[path = "graph_types.rs"]
mod graph_types;

#[derive(Clone)]
struct Reference {
    file: PathBuf,
    name: String,
}
struct File {
    namespace: String,
    declarations: Vec<TsDeclaration>,
    imports: BTreeMap<String, Reference>,
    exports: BTreeMap<String, Reference>,
    stars: Vec<PathBuf>,
    globals: BTreeSet<String>,
}
struct Graph {
    root: PathBuf,
    prefix: String,
    explicit: BTreeMap<PathBuf, String>,
    pins: BTreeMap<PathBuf, String>,
    files: BTreeMap<PathBuf, File>,
    inputs: Vec<TsInputFile>,
}

pub(in crate::commands::bind) fn load_declaration_graph(
    root: &Path,
    manifest: &mut TsInputManifest,
) -> Result<TsDeclarationFile, String> {
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    let mut graph = Graph {
        prefix: manifest.inputs[0].namespace.clone(),
        root: root.clone(),
        explicit: BTreeMap::new(),
        pins: BTreeMap::new(),
        files: BTreeMap::new(),
        inputs: Vec::new(),
    };
    for input in &manifest.inputs {
        let path = root
            .join(&input.path)
            .canonicalize()
            .map_err(|error| error.to_string())?;
        if graph
            .explicit
            .get(&path)
            .is_some_and(|namespace| namespace != &input.namespace)
        {
            return Err(format!(
                "ts_bindgen.conflicting_input_namespace: {}",
                path.display()
            ));
        }
        graph.pins.insert(path.clone(), input.sha256.clone());
        graph.explicit.insert(path, input.namespace.clone());
    }
    for input in &manifest.inputs {
        graph.load(&root.join(&input.path))?;
    }
    manifest.inputs.extend(
        graph
            .inputs
            .iter()
            .filter(|input| {
                !manifest
                    .inputs
                    .iter()
                    .any(|existing| existing.path == input.path)
            })
            .cloned()
            .collect::<Vec<_>>(),
    );
    let mut declarations = Vec::new();
    // Keep explicit input order; dependencies follow deterministically.
    let mut paths = manifest
        .inputs
        .iter()
        .map(|input| root.join(&input.path))
        .collect::<Vec<_>>();
    paths.dedup();
    for path in paths {
        let Some(file) = graph.files.get(&path) else {
            continue;
        };
        for declaration in &file.declarations {
            let mut declaration = declaration.clone();
            graph_types::qualify(&graph, &path, &mut declaration);
            declarations.push(declaration);
        }
        // Named re-exports expose the same identity and operations as their source.
        for name in graph.public_names(&path, &mut BTreeSet::new()) {
            if file
                .declarations
                .iter()
                .any(|declaration| match declaration {
                    TsDeclaration::Interface(value) => {
                        value.namespace.is_empty()
                            && super::super::ts_dom_module_mapping::terlan_type_name(&value.name)
                                == super::super::ts_dom_module_mapping::terlan_type_name(&name)
                    }
                    TsDeclaration::TypeAlias(value) => {
                        value.namespace.is_empty()
                            && super::super::ts_dom_module_mapping::terlan_type_name(&value.name)
                                == super::super::ts_dom_module_mapping::terlan_type_name(&name)
                    }
                    _ => false,
                })
            {
                continue;
            }
            if let Some(target) = graph.export(&path, &name, &mut BTreeSet::new()) {
                let type_params = graph.type_parameters(&target);
                let ty = if type_params.is_empty() {
                    TsTypeRef::Named(target)
                } else {
                    TsTypeRef::Generic {
                        name: target,
                        args: type_params.iter().cloned().map(TsTypeRef::Named).collect(),
                    }
                };
                declarations.push(TsDeclaration::TypeAlias(TsTypeAliasDeclaration {
                    imported: false,
                    namespace: file.namespace.clone(),
                    name: name.clone(),
                    doc: None,
                    type_params,
                    ty,
                }));
            } else {
                declarations.push(TsDeclaration::Unsupported(TsUnsupportedDeclaration {
                    source: format!("{}.{name}", file.namespace),
                    reason: "ts_bindgen.unresolved_export",
                    detail:
                        "public re-export does not resolve uniquely to a supported type declaration"
                            .into(),
                }));
            }
        }
    }
    graph_types::inherit(&mut declarations);
    Ok(TsDeclarationFile { declarations })
}

impl Graph {
    fn public_names(&self, path: &Path, seen: &mut BTreeSet<PathBuf>) -> BTreeSet<String> {
        if !seen.insert(path.into()) {
            return BTreeSet::new();
        }
        let file = &self.files[path];
        let mut names = file.exports.keys().cloned().collect::<BTreeSet<_>>();
        for star in &file.stars {
            names.extend(
                self.public_names(star, seen)
                    .into_iter()
                    .filter(|name| name != "default"),
            );
        }
        names
    }

    fn type_parameters(&self, target: &str) -> Vec<String> {
        for (path, file) in &self.files {
            for declaration in &file.declarations {
                let (namespace, name, parameters) = match declaration {
                    TsDeclaration::Interface(value) => {
                        (&value.namespace, &value.name, &value.type_params)
                    }
                    TsDeclaration::TypeAlias(value) => {
                        (&value.namespace, &value.name, &value.type_params)
                    }
                    _ => continue,
                };
                let source = if namespace.is_empty() {
                    name.clone()
                } else {
                    format!("{namespace}.{name}")
                };
                if self.local(path, &source).as_deref() == Some(target) {
                    return parameters.clone();
                }
            }
        }
        Vec::new()
    }
    fn load(&mut self, path: &Path) -> Result<(), String> {
        let path = path.canonicalize().map_err(|error| {
            format!("ts_bindgen.read_input_failed: {}: {error}", path.display())
        })?;
        if self.files.contains_key(&path) {
            return Ok(());
        }
        let relative = path
            .strip_prefix(&self.root)
            .map_err(|_| format!("ts_bindgen.import_outside_root: {}", path.display()))?;
        let namespace = self.explicit.get(&path).cloned().unwrap_or_else(|| {
            let suffix = relative
                .to_string_lossy()
                .trim_end_matches(".d.ts")
                .split(['/', '\\'])
                .map(|part| {
                    let mut result = String::from("file_");
                    for byte in part.bytes() {
                        if byte.is_ascii_lowercase() || byte.is_ascii_digit() {
                            result.push(char::from(byte));
                        } else {
                            result.push_str(&format!("_{byte:02x}"));
                        }
                    }
                    result
                })
                .collect::<Vec<_>>()
                .join(".");
            format!("{}.declarations.{suffix}", self.prefix)
        });
        let source = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
        let source_hash = super::super::ts_generated_artifact::sha256_hex(source.as_bytes());
        if self.pins.get(&path).is_some_and(|pin| pin != &source_hash) {
            return Err(format!(
                "ts_bindgen.input_manifest_sha256_mismatch: {} changed while loading",
                path.display()
            ));
        }
        let parsed = parse_ts_declaration_file(&source)
            .map_err(|error| format!("{}: {}: {}", error.reason, path.display(), error.message))?;
        let allocator = oxc_allocator::Allocator::default();
        let program =
            oxc_parser::Parser::new(&allocator, &source, oxc_span::SourceType::d_ts()).parse();
        let mut imports = BTreeMap::new();
        let mut exports = BTreeMap::new();
        let mut stars = Vec::new();
        let mut dependencies = BTreeSet::new();
        let mut resolved_imports = BTreeSet::new();
        let mut unresolved_exports = Vec::new();
        let unhandled_export = program.program.body.iter().any(|statement| {
            matches!(
                statement,
                Statement::TSExportAssignment(_) | Statement::TSNamespaceExportDeclaration(_)
            )
        });
        let mut globals = BTreeSet::new();
        if !program.program.body.iter().any(|statement| {
            matches!(
                statement,
                Statement::ImportDeclaration(_)
                    | Statement::ExportNamedDeclaration(_)
                    | Statement::ExportAllDeclaration(_)
                    | Statement::ExportDefaultDeclaration(_)
                    | Statement::TSExportAssignment(_)
            )
        }) {
            for declaration in &parsed.declarations {
                let (scope, name) = match declaration {
                    TsDeclaration::Interface(value) => (&value.namespace, &value.name),
                    TsDeclaration::TypeAlias(value) => (&value.namespace, &value.name),
                    _ => continue,
                };
                globals.insert(if scope.is_empty() {
                    name.clone()
                } else {
                    scope.split('.').next().unwrap().into()
                });
            }
        }
        for statement in &program.program.body {
            match statement {
                Statement::ExportDefaultDeclaration(export) => {
                    let name = match &export.declaration {
                        ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                            class.id.as_ref().map(|id| id.name.to_string())
                        }
                        ExportDefaultDeclarationKind::TSInterfaceDeclaration(interface) => {
                            Some(interface.id.name.to_string())
                        }
                        ExportDefaultDeclarationKind::Identifier(id) => Some(id.name.to_string()),
                        _ => None,
                    };
                    if let Some(name) = name {
                        exports.insert(
                            "default".into(),
                            Reference {
                                file: path.clone(),
                                name,
                            },
                        );
                    }
                }
                Statement::TSGlobalDeclaration(global) => {
                    for item in &global.body.body {
                        let declaration = match item {
                            Statement::ExportNamedDeclaration(export) => {
                                export.declaration.as_ref()
                            }
                            _ => item.as_declaration(),
                        };
                        if let Some(declaration) = declaration {
                            match declaration {
                                Declaration::TSInterfaceDeclaration(value) => {
                                    globals.insert(value.id.name.to_string());
                                }
                                Declaration::TSModuleDeclaration(value) => {
                                    if let TSModuleDeclarationName::Identifier(name) = &value.id {
                                        globals.insert(name.name.to_string());
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Statement::ImportDeclaration(import) => {
                    let Some(target) = self.resolve_file(&path, import.source.value.as_str())
                    else {
                        continue;
                    };
                    dependencies.insert(target.clone());
                    resolved_imports.insert(format!("import:{}", import.source.value));
                    for specifier in import.specifiers.iter().flatten() {
                        let (local, name) = match specifier {
                            ImportDeclarationSpecifier::ImportSpecifier(value) => (
                                value.local.name.to_string(),
                                value.imported.name().to_string(),
                            ),
                            ImportDeclarationSpecifier::ImportDefaultSpecifier(value) => {
                                (value.local.name.to_string(), "default".into())
                            }
                            ImportDeclarationSpecifier::ImportNamespaceSpecifier(value) => {
                                (value.local.name.to_string(), "*".into())
                            }
                        };
                        imports.insert(
                            local,
                            Reference {
                                file: target.clone(),
                                name,
                            },
                        );
                    }
                }
                Statement::ExportNamedDeclaration(export) => {
                    let target = export
                        .source
                        .as_ref()
                        .and_then(|source| self.resolve_file(&path, source.value.as_str()));
                    if let Some(target) = &target {
                        dependencies.insert(target.clone());
                    }
                    if let Some(declaration) = &export.declaration {
                        let name = match declaration {
                            Declaration::TSInterfaceDeclaration(value) => {
                                Some(value.id.name.as_str())
                            }
                            Declaration::TSTypeAliasDeclaration(value) => {
                                Some(value.id.name.as_str())
                            }
                            Declaration::ClassDeclaration(value) => {
                                value.id.as_ref().map(|id| id.name.as_str())
                            }
                            _ => None,
                        };
                        if let Some(name) = name {
                            exports.insert(
                                name.to_string(),
                                Reference {
                                    file: path.clone(),
                                    name: name.into(),
                                },
                            );
                        }
                    }
                    for specifier in &export.specifiers {
                        exports.insert(
                            specifier.exported.name().to_string(),
                            Reference {
                                file: target.clone().unwrap_or_else(|| path.clone()),
                                name: specifier.local.name().to_string(),
                            },
                        );
                    }
                }
                Statement::ExportAllDeclaration(export) => {
                    if let Some(target) = self.resolve_file(&path, export.source.value.as_str()) {
                        dependencies.insert(target.clone());
                        if let Some(name) = &export.exported {
                            exports.insert(
                                name.name().to_string(),
                                Reference {
                                    file: target,
                                    name: "*".into(),
                                },
                            );
                        } else {
                            stars.push(target);
                        }
                    } else {
                        unresolved_exports
                            .push(TsDeclaration::Unsupported(TsUnsupportedDeclaration {
                            source: format!("export:{}", export.source.value),
                            reason: "ts_bindgen.unresolved_export",
                            detail:
                                "cannot load the re-exported declaration file inside the input root"
                                    .into(),
                        }));
                    }
                }
                _ => {}
            }
        }
        let mut declarations:Vec<_> = parsed.declarations.into_iter().filter(|declaration| !matches!(declaration,
            TsDeclaration::Unsupported(value) if (value.reason == "ts_bindgen.unsupported_top_level_import" && resolved_imports.contains(&value.source)) ||
            (value.reason == "ts_bindgen.unsupported_top_level_export" && value.source == "export" && !unhandled_export)
        )).collect();
        declarations.extend(unresolved_exports);
        self.inputs.push(TsInputFile {
            path: relative.to_string_lossy().replace('\\', "/"),
            namespace: namespace.clone(),
            kind: "typescript-declaration".into(),
            sha256: source_hash,
        });
        self.files.insert(
            path,
            File {
                namespace,
                declarations,
                imports,
                exports,
                stars,
                globals,
            },
        );
        for dependency in dependencies {
            self.load(&dependency)?;
        }
        Ok(())
    }

    fn resolve_file(&self, importer: &Path, specifier: &str) -> Option<PathBuf> {
        if !specifier.starts_with('.') {
            return None;
        }
        let path = importer.parent()?.join(specifier);
        let mut candidates = vec![path.clone()];
        if specifier.ends_with(".ts") && !specifier.ends_with(".d.ts") || specifier.ends_with(".js")
        {
            candidates.insert(0, path.with_extension("d.ts"));
        }
        candidates.extend([
            PathBuf::from(format!("{}.d.ts", path.display())),
            path.join("index.d.ts"),
        ]);
        candidates.into_iter().find_map(|candidate| {
            let candidate = candidate.canonicalize().ok()?;
            (candidate.starts_with(&self.root)
                && candidate.is_file()
                && candidate.to_string_lossy().ends_with(".d.ts"))
            .then_some(candidate)
        })
    }

    fn local(&self, path: &Path, name: &str) -> Option<String> {
        let file = self.files.get(path)?;
        file.declarations.iter().find_map(|declaration| {
            let (namespace, local) = match declaration {
                TsDeclaration::Interface(value) => (&value.namespace, &value.name),
                TsDeclaration::TypeAlias(value) => (&value.namespace, &value.name),
                _ => return None,
            };
            let full = if namespace.is_empty() {
                local.clone()
            } else {
                format!("{namespace}.{local}")
            };
            (full == name).then(|| {
                let local = super::super::ts_dom_module_mapping::terlan_type_name(local);
                if namespace.is_empty() {
                    format!("{}.{local}", file.namespace)
                } else {
                    format!("{}.{namespace}.{local}", file.namespace)
                }
            })
        })
    }

    fn export(
        &self,
        path: &Path,
        name: &str,
        visited: &mut BTreeSet<(PathBuf, String)>,
    ) -> Option<String> {
        if !visited.insert((path.to_path_buf(), name.into())) {
            return None;
        }
        let file = self.files.get(path)?;
        if let Some(reference) = file.exports.get(name) {
            if reference.file == path {
                return self
                    .local(path, &reference.name)
                    .or_else(|| self.lookup(path, "", &reference.name, visited));
            }
            return self.export(&reference.file, &reference.name, visited);
        }
        let mut candidates = BTreeSet::new();
        for star in &file.stars {
            if let Some(target) = self.export(star, name, &mut visited.clone()) {
                candidates.insert(target);
            }
        }
        (candidates.len() == 1).then(|| candidates.into_iter().next().unwrap())
    }

    fn lookup(
        &self,
        path: &Path,
        namespace: &str,
        name: &str,
        visited: &mut BTreeSet<(PathBuf, String)>,
    ) -> Option<String> {
        if !namespace.is_empty() {
            if let Some(found) = self.local(path, &format!("{namespace}.{name}")) {
                return Some(found);
            }
        }
        if let Some(found) = self.local(path, name) {
            return Some(found);
        }
        let (head, tail) = name.split_once('.').unwrap_or((name, ""));
        if let Some(reference) = self.files.get(path)?.imports.get(head) {
            let exported = if reference.name == "*" {
                tail
            } else {
                &reference.name
            };
            return self.export(&reference.file, exported, visited);
        }
        let global = self
            .files
            .iter()
            .filter(|(_, file)| file.globals.contains(head))
            .filter_map(|(path, _)| self.local(path, name))
            .collect::<BTreeSet<_>>();
        if global.len() == 1 {
            return global.into_iter().next();
        }
        None
    }
}
