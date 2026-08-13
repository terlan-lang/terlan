#![forbid(unsafe_code)]

use quote::ToTokens;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use syn::visit::{self, Visit};
use syn::{
    Attribute, Expr, ExprPath, FnArg, ImplItemFn, ItemEnum, ItemFn, ItemMod, ItemStruct, ItemTrait,
    ItemType, ItemUnion, Lit, LitStr, Meta, PathArguments, ReturnType, Type, Visibility,
};

mod api_boundary_input;
use api_boundary_input::write_api_boundary_input;
mod structural_input;
use structural_input::write_structural_input;
mod shared_helper_input;
use shared_helper_input::write_shared_helper_input;

const THIN_BINARIES: [(&str, &str); 8] = [
    ("crates/terlan/src/main.rs", "terlan::run_cli_from_env"),
    ("crates/terlan/src/vm/main.rs", "terlan::vm::run_from_env"),
    (
        "crates/terlan/src/lsp/main.rs",
        "terlan::lsp::run_stdio_server",
    ),
    (
        "crates/terlan/src/native_worker/main.rs",
        "terlan::native_worker::run_from_env",
    ),
    (
        "crates/terlan/src/quality/main.rs",
        "terlan::quality::run_from_env",
    ),
    (
        "crates/terlan/src/quality/native_no_std_target_feasibility_main.rs",
        "terlan::quality::run_native_target_feasibility_from_workspace",
    ),
    (
        "crates/terlan/src/quality/lean_proof_closeout_main.rs",
        "terlan::quality::run_lean_proof_closeout_from_workspace",
    ),
    (
        "crates/terlan/src/benchmark/main.rs",
        "terlan::benchmark::run_from_env",
    ),
];

const FEATURE_MODULES: [(&str, &str); 3] = [
    ("lsp", "editor-lsp"),
    ("quality", "quality-tools"),
    ("benchmark", "benchmark-tools"),
];

#[derive(Debug)]
enum AuditError {
    Message(String),
}

impl std::fmt::Display for AuditError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Message(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for AuditError {}

#[derive(Clone, Debug)]
struct TypeDeclaration {
    kind: &'static str,
    name: String,
    exact: String,
    equal_shape: String,
    path: String,
}

struct TypeVisitor<'a> {
    path: &'a str,
    implementation: bool,
    declarations: Vec<TypeDeclaration>,
    functions: Vec<FunctionDeclaration>,
    lint_attributes: Vec<LintAttribute>,
    crate_references: Vec<PathTarget>,
}

#[derive(Clone, Debug)]
struct FunctionDeclaration {
    name: String,
    path: String,
    visibility: &'static str,
    argument_count: usize,
    returns_string_error: bool,
    explicit_ffi_parameters: bool,
    implementation: bool,
}

#[derive(Clone, Debug)]
struct LintAttribute {
    path: String,
    kind: String,
    tokens: String,
}

#[derive(Clone, Debug)]
struct PathTarget {
    path: String,
    target: String,
}

struct PathAttributeVisitor<'a> {
    path: &'a str,
    attributes: Vec<PathTarget>,
}

#[derive(Clone, Debug)]
struct ThinBinaryDiagnostic {
    path: String,
    message: String,
}

struct ThinBinaryVisitor<'a> {
    expected_call: &'a str,
    call_found: bool,
    owns_module: bool,
}

impl Visit<'_> for ThinBinaryVisitor<'_> {
    fn visit_expr_path(&mut self, expression: &ExprPath) {
        let rendered = expression
            .path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>()
            .join("::");
        if rendered == self.expected_call {
            self.call_found = true;
        }
        visit::visit_expr_path(self, expression);
    }

    fn visit_item_mod(&mut self, module: &ItemMod) {
        self.owns_module = true;
        visit::visit_item_mod(self, module);
    }
}

fn thin_binary_diagnostics(root: &Path) -> Result<Vec<ThinBinaryDiagnostic>, AuditError> {
    let mut diagnostics = Vec::new();
    for (relative, expected_call) in THIN_BINARIES {
        let path = root.join(relative);
        if !path.is_file() {
            diagnostics.push(ThinBinaryDiagnostic {
                path: relative.to_owned(),
                message: "missing binary entrypoint".to_owned(),
            });
            continue;
        }
        let source = fs::read_to_string(&path).map_err(|error| {
            AuditError::Message(format!("cannot read `{}`: {error}", path.display()))
        })?;
        let file = syn::parse_file(&source).map_err(|error| {
            AuditError::Message(format!("cannot parse `{relative}` as Rust: {error}"))
        })?;
        let mut visitor = ThinBinaryVisitor {
            expected_call,
            call_found: false,
            owns_module: false,
        };
        visitor.visit_file(&file);
        if !visitor.call_found {
            diagnostics.push(ThinBinaryDiagnostic {
                path: relative.to_owned(),
                message: format!(
                    "entrypoint must delegate through normal library API `{expected_call}`"
                ),
            });
        }
        if visitor.owns_module {
            diagnostics.push(ThinBinaryDiagnostic {
                path: relative.to_owned(),
                message: "thin binary entrypoint may not own implementation modules".to_owned(),
            });
        }
    }
    Ok(diagnostics)
}

fn has_cfg_feature(attributes: &[Attribute], expected: &str) -> bool {
    attributes.iter().any(|attribute| {
        if !attribute.path().is_ident("cfg") {
            return false;
        }
        let mut matched = false;
        let parsed = attribute.parse_nested_meta(|meta| {
            if meta.path.is_ident("feature") {
                let literal: LitStr = meta.value()?.parse()?;
                matched |= literal.value() == expected;
            }
            Ok(())
        });
        parsed.is_ok() && matched
    })
}

fn feature_module_diagnostics(root: &Path) -> Result<Vec<ThinBinaryDiagnostic>, AuditError> {
    let relative = "crates/terlan/src/lib.rs";
    let path = root.join(relative);
    let source = fs::read_to_string(&path).map_err(|error| {
        AuditError::Message(format!("cannot read `{}`: {error}", path.display()))
    })?;
    let file = syn::parse_file(&source)
        .map_err(|error| AuditError::Message(format!("cannot parse `{relative}`: {error}")))?;
    let mut diagnostics = Vec::new();
    for (module_name, feature) in FEATURE_MODULES {
        let isolated = file.items.iter().any(|item| {
            let syn::Item::Mod(module) = item else {
                return false;
            };
            module.ident == module_name
                && matches!(module.vis, Visibility::Public(_))
                && has_cfg_feature(&module.attrs, feature)
        });
        if !isolated {
            diagnostics.push(ThinBinaryDiagnostic {
                path: relative.to_owned(),
                message: format!("`{module_name}` must be isolated by `{feature}`"),
            });
        }
    }
    Ok(diagnostics)
}

fn cross_tree_path_attribute_target(attribute: &Attribute) -> Option<String> {
    if !attribute.path().is_ident("path") {
        return None;
    }
    let Meta::NameValue(name_value) = &attribute.meta else {
        return None;
    };
    let Expr::Lit(expression) = &name_value.value else {
        return None;
    };
    let Lit::Str(target) = &expression.lit else {
        return None;
    };
    let target = target.value();
    Path::new(&target)
        .components()
        .any(|component| component == std::path::Component::ParentDir)
        .then_some(target)
}

impl<'ast> Visit<'ast> for PathAttributeVisitor<'_> {
    fn visit_attribute(&mut self, attribute: &'ast Attribute) {
        if let Some(target) = cross_tree_path_attribute_target(attribute) {
            self.attributes.push(PathTarget {
                path: self.path.to_owned(),
                target,
            });
        }
        visit::visit_attribute(self, attribute);
    }
}

fn visibility(visibility: &Visibility) -> &'static str {
    match visibility {
        Visibility::Public(_) => "public",
        Visibility::Restricted(restricted) if restricted.path.is_ident("crate") => {
            "cross-subsystem"
        }
        Visibility::Restricted(_) => "module",
        Visibility::Inherited => "private",
    }
}

fn is_string_type(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Path(path)
            if path.qself.is_none()
                && path.path.segments.last().is_some_and(|segment| segment.ident == "String")
    )
}

fn returns_string_error(output: &ReturnType) -> bool {
    let ReturnType::Type(_, ty) = output else {
        return false;
    };
    let Type::Path(path) = ty.as_ref() else {
        return false;
    };
    let Some(result) = path.path.segments.last() else {
        return false;
    };
    if result.ident != "Result" {
        return false;
    }
    let PathArguments::AngleBracketed(arguments) = &result.arguments else {
        return false;
    };
    arguments
        .args
        .iter()
        .filter_map(|argument| match argument {
            syn::GenericArgument::Type(ty) => Some(ty),
            _ => None,
        })
        .nth(1)
        .is_some_and(is_string_type)
}

fn has_ffi_parameter_rationale(attributes: &[Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        attribute.path().is_ident("doc")
            && attribute
                .meta
                .to_token_stream()
                .to_string()
                .contains("cq4-ffi-explicit-parameters")
    })
}

impl TypeVisitor<'_> {
    fn record<T: ToTokens>(&mut self, kind: &'static str, name: &syn::Ident, item: &T) {
        let tokens = item.to_token_stream().to_string();
        let equal_shape = tokens.replacen(&name.to_string(), "__CanonicalType", 1);
        self.declarations.push(TypeDeclaration {
            kind,
            name: name.to_string(),
            exact: tokens,
            equal_shape,
            path: self.path.to_owned(),
        });
    }

    fn record_function(
        &mut self,
        name: &syn::Ident,
        item_visibility: &Visibility,
        arguments: impl Iterator<Item = FnArg>,
        output: &ReturnType,
        attributes: &[Attribute],
        explicit_ffi_parameters: bool,
    ) {
        self.functions.push(FunctionDeclaration {
            name: name.to_string(),
            path: self.path.to_owned(),
            visibility: visibility(item_visibility),
            argument_count: arguments.count(),
            returns_string_error: returns_string_error(output),
            explicit_ffi_parameters: explicit_ffi_parameters
                && has_ffi_parameter_rationale(attributes),
            implementation: self.implementation,
        });
    }
}

impl<'ast> Visit<'ast> for TypeVisitor<'_> {
    fn visit_attribute(&mut self, attribute: &'ast Attribute) {
        let kind = if attribute.path().is_ident("allow") {
            Some("allow")
        } else if attribute.path().is_ident("expect") {
            Some("expect")
        } else if attribute.path().is_ident("cfg_attr")
            && attribute
                .meta
                .to_token_stream()
                .to_string()
                .contains("allow")
        {
            Some("cfg-allow")
        } else {
            None
        };
        if let Some(kind) = kind {
            self.lint_attributes.push(LintAttribute {
                path: self.path.to_owned(),
                kind: kind.to_owned(),
                tokens: attribute.meta.to_token_stream().to_string(),
            });
        }
        visit::visit_attribute(self, attribute);
    }

    fn visit_item_fn(&mut self, item: &'ast ItemFn) {
        self.record_function(
            &item.sig.ident,
            &item.vis,
            item.sig.inputs.iter().cloned(),
            &item.sig.output,
            &item.attrs,
            item.sig.abi.is_some(),
        );
        visit::visit_item_fn(self, item);
    }

    fn visit_impl_item_fn(&mut self, item: &'ast ImplItemFn) {
        self.record_function(
            &item.sig.ident,
            &item.vis,
            item.sig.inputs.iter().cloned(),
            &item.sig.output,
            &item.attrs,
            item.sig.abi.is_some(),
        );
        visit::visit_impl_item_fn(self, item);
    }

    fn visit_item_enum(&mut self, item: &'ast ItemEnum) {
        self.record("enum", &item.ident, item);
        visit::visit_item_enum(self, item);
    }

    fn visit_item_struct(&mut self, item: &'ast ItemStruct) {
        self.record("struct", &item.ident, item);
        visit::visit_item_struct(self, item);
    }

    fn visit_item_trait(&mut self, item: &'ast ItemTrait) {
        self.record("trait", &item.ident, item);
        visit::visit_item_trait(self, item);
    }

    fn visit_item_type(&mut self, item: &'ast ItemType) {
        self.record("type", &item.ident, item);
        visit::visit_item_type(self, item);
    }

    fn visit_item_union(&mut self, item: &'ast ItemUnion) {
        self.record("union", &item.ident, item);
        visit::visit_item_union(self, item);
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        let mut segments = path.segments.iter();
        if segments
            .next()
            .is_some_and(|segment| segment.ident == "crate")
        {
            if let Some(target) = segments.next() {
                self.crate_references.push(PathTarget {
                    path: self.path.to_owned(),
                    target: target.ident.to_string(),
                });
            }
        }
        visit::visit_path(self, path);
    }
}

fn collect_files(path: &Path, files: &mut Vec<PathBuf>) -> Result<(), AuditError> {
    let entries = fs::read_dir(path).map_err(|error| {
        AuditError::Message(format!("cannot read `{}`: {error}", path.display()))
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            AuditError::Message(format!(
                "cannot inspect an entry below `{}`: {error}",
                path.display()
            ))
        })?;
        let child = entry.path();
        if child.is_dir() {
            if !matches!(
                child.file_name().and_then(|name| name.to_str()),
                Some("target" | ".git")
            ) {
                collect_files(&child, files)?;
            }
        } else if child.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            files.push(child);
        }
    }
    Ok(())
}

fn is_implementation(path: &Path) -> bool {
    let text = path.to_string_lossy();
    !text.contains("/tests/")
        && !text.contains("_test/")
        && !text.contains("/fixtures/")
        && !text.contains("/generated/")
        && !text.ends_with("_test.rs")
}

fn concept(name: &str) -> String {
    name.chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn audit(root: &Path, api_boundary_input: Option<&Path>) -> Result<Value, AuditError> {
    let mut files = Vec::new();
    for member_source in [
        root.join("crates/terlan/src"),
        root.join("std/native/libpq/generated/native/rust/src"),
        root.join("tools/rust_boundary_audit/src"),
    ] {
        collect_files(&member_source, &mut files)?;
    }
    files.sort();

    let mut declarations = Vec::new();
    let mut parse_failures = Vec::new();
    let mut functions = Vec::new();
    let mut lint_attributes = Vec::new();
    let mut crate_references = Vec::new();
    let mut source_files = Vec::new();
    for path in files {
        let source = fs::read_to_string(&path).map_err(|error| {
            AuditError::Message(format!("cannot read `{}`: {error}", path.display()))
        })?;
        let relative = path
            .strip_prefix(root)
            .map_err(|error| {
                AuditError::Message(format!("cannot relativize `{}`: {error}", path.display()))
            })?
            .to_string_lossy()
            .into_owned();
        if relative.starts_with("crates/terlan/src/") {
            source_files.push(json!({
                "path": relative,
                "physical_lines": source.lines().count(),
            }));
        }
        match syn::parse_file(&source) {
            Ok(file) => {
                let mut visitor = TypeVisitor {
                    path: &relative,
                    implementation: is_implementation(&path),
                    declarations: Vec::new(),
                    functions: Vec::new(),
                    lint_attributes: Vec::new(),
                    crate_references: Vec::new(),
                };
                visitor.visit_file(&file);
                if is_implementation(&path) {
                    declarations.extend(visitor.declarations);
                }
                functions.extend(visitor.functions);
                lint_attributes.extend(visitor.lint_attributes);
                crate_references.extend(visitor.crate_references);
            }
            Err(error) => parse_failures.push(json!({
                "path": relative,
                "error": error.to_string(),
            })),
        }
    }

    let mut boundary_files = Vec::new();
    collect_files(&root.join("crates"), &mut boundary_files)?;
    boundary_files.sort();
    let mut cross_tree_path_attributes = Vec::new();
    let mut source_boundary_parse_failures = Vec::new();
    for path in boundary_files {
        let source = fs::read_to_string(&path).map_err(|error| {
            AuditError::Message(format!("cannot read `{}`: {error}", path.display()))
        })?;
        let relative = path
            .strip_prefix(root)
            .map_err(|error| {
                AuditError::Message(format!("cannot relativize `{}`: {error}", path.display()))
            })?
            .to_string_lossy()
            .into_owned();
        match syn::parse_file(&source) {
            Ok(file) => {
                let mut visitor = PathAttributeVisitor {
                    path: &relative,
                    attributes: Vec::new(),
                };
                visitor.visit_file(&file);
                cross_tree_path_attributes.extend(visitor.attributes);
            }
            Err(error) => source_boundary_parse_failures.push(json!({
                "path": relative,
                "error": error.to_string(),
            })),
        }
    }

    let mut exact = BTreeMap::<(&str, &str), Vec<&TypeDeclaration>>::new();
    let mut equal_shape = BTreeMap::<(&str, &str), Vec<&TypeDeclaration>>::new();
    let mut named = BTreeMap::<String, Vec<&TypeDeclaration>>::new();
    for declaration in &declarations {
        exact
            .entry((declaration.kind, &declaration.exact))
            .or_default()
            .push(declaration);
        equal_shape
            .entry((declaration.kind, &declaration.equal_shape))
            .or_default()
            .push(declaration);
        named
            .entry(concept(&declaration.name))
            .or_default()
            .push(declaration);
    }
    let exact_duplicates = exact
        .into_iter()
        .filter(|(_, rows)| rows.len() > 1)
        .map(|((kind, normalized), rows)| {
            json!({
                "kind": kind,
                "normalized": normalized,
                "declarations": rows.into_iter().map(|row| json!({
                    "name": row.name,
                    "path": row.path,
                })).collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();
    let equal_shape_candidates = equal_shape
        .into_iter()
        .filter(|(_, rows)| rows.len() > 1)
        .map(|((kind, normalized), rows)| {
            json!({
                "kind": kind,
                "normalized": normalized,
                "declarations": rows.into_iter().map(|row| json!({
                    "name": row.name,
                    "path": row.path,
                })).collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();
    let same_name_candidates = named
        .into_iter()
        .filter(|(_, rows)| rows.len() > 1)
        .map(|(name, rows)| {
            json!({
                "concept": name,
                "declarations": rows.into_iter().map(|row| json!({
                    "kind": row.kind,
                    "name": row.name,
                    "path": row.path,
                })).collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();
    let thin_binary_diagnostics = thin_binary_diagnostics(root)?;
    let feature_module_diagnostics = feature_module_diagnostics(root)?;
    if let Some(path) = api_boundary_input {
        write_api_boundary_input(
            path,
            &functions,
            parse_failures.len(),
            lint_attributes.len(),
        )?;
    }

    Ok(json!({
        "schema_version": 1,
        "implementation_files_parsed": declarations
            .iter()
            .map(|declaration| &declaration.path)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        "type_declarations": declarations.len(),
        "parse_failures": parse_failures,
        "source_boundary_parse_failures": source_boundary_parse_failures,
        "cross_tree_path_attributes": cross_tree_path_attributes.into_iter().map(|attribute| json!({
            "path": attribute.path,
            "target": attribute.target,
        })).collect::<Vec<_>>(),
        "thin_binary_diagnostics": thin_binary_diagnostics.into_iter().map(|diagnostic| json!({
            "path": diagnostic.path,
            "message": diagnostic.message,
        })).collect::<Vec<_>>(),
        "feature_module_diagnostics": feature_module_diagnostics.into_iter().map(|diagnostic| json!({
            "path": diagnostic.path,
            "message": diagnostic.message,
        })).collect::<Vec<_>>(),
        "exact_normalized_duplicates": exact_duplicates,
        "equal_shape_candidates": equal_shape_candidates,
        "same_name_candidates": same_name_candidates,
        "functions": functions.into_iter().map(|function| json!({
            "name": function.name,
            "path": function.path,
            "visibility": function.visibility,
            "argument_count": function.argument_count,
            "returns_string_error": function.returns_string_error,
            "explicit_ffi_parameters": function.explicit_ffi_parameters,
            "implementation": function.implementation,
        })).collect::<Vec<_>>(),
        "lint_attributes": lint_attributes.into_iter().map(|attribute| json!({
            "path": attribute.path,
            "kind": attribute.kind,
            "tokens": attribute.tokens,
        })).collect::<Vec<_>>(),
        "crate_references": crate_references.into_iter().map(|reference| json!({
            "path": reference.path,
            "target": reference.target,
        })).collect::<Vec<_>>(),
        "source_files": source_files,
    }))
}

#[cfg(test)]
mod tests {
    use super::{
        cross_tree_path_attribute_target, has_cfg_feature, ThinBinaryVisitor, TypeVisitor,
    };
    use syn::visit::Visit;

    #[test]
    fn cross_tree_path_attributes_ignore_literals_and_accept_real_attributes() {
        let parsed = syn::parse_file(
            r##"
            const FIXTURE: &str = r#"#[path = "../fake.rs"]"#;
            #[path = "../real.rs"]
            mod real;
            "##,
        );
        assert!(parsed.is_ok());
        if let Ok(file) = parsed {
            let target = file.items.iter().find_map(|item| match item {
                syn::Item::Mod(module) => module
                    .attrs
                    .iter()
                    .find_map(cross_tree_path_attribute_target),
                _ => None,
            });
            assert_eq!(target.as_deref(), Some("../real.rs"));
        }
    }

    #[test]
    fn thin_binary_ast_check_ignores_literals_and_detects_calls_and_modules() {
        let file = syn::parse_file(
            r#"
            const FIXTURE: &str = "terlan::fake::run";
            mod owned;
            fn main() { terlan::real::run(); }
            "#,
        )
        .expect("parse thin binary fixture");
        let mut expected = ThinBinaryVisitor {
            expected_call: "terlan::real::run",
            call_found: false,
            owns_module: false,
        };
        expected.visit_file(&file);
        assert!(expected.call_found);
        assert!(expected.owns_module);

        let mut literal = ThinBinaryVisitor {
            expected_call: "terlan::fake::run",
            call_found: false,
            owns_module: false,
        };
        literal.visit_file(&file);
        assert!(!literal.call_found);
    }

    #[test]
    fn feature_isolation_accepts_only_the_expected_cfg_feature() {
        let file = syn::parse_file(
            r#"
            #[cfg(feature = "editor-lsp")]
            #[doc = "editor surface"]
            pub mod lsp;
            "#,
        )
        .expect("parse feature fixture");
        let syn::Item::Mod(module) = &file.items[0] else {
            panic!("expected module item");
        };
        assert!(has_cfg_feature(&module.attrs, "editor-lsp"));
        assert!(!has_cfg_feature(&module.attrs, "quality-tools"));
    }

    #[test]
    fn crate_reference_inventory_uses_rust_paths_not_text_matches() {
        let file = syn::parse_file(
            r#"
            const FIXTURE: &str = "crate::fake::Thing";
            fn load(value: crate::runtime::Value) -> crate::support::Result {
                crate::runtime::convert(value)
            }
            "#,
        )
        .expect("parse crate-reference fixture");
        let mut visitor = TypeVisitor {
            path: "fixture.rs",
            implementation: true,
            declarations: Vec::new(),
            functions: Vec::new(),
            lint_attributes: Vec::new(),
            crate_references: Vec::new(),
        };
        visitor.visit_file(&file);
        let targets = visitor
            .crate_references
            .into_iter()
            .map(|reference| reference.target)
            .collect::<Vec<_>>();
        assert_eq!(targets, ["runtime", "support", "runtime"]);
    }
}

fn run() -> Result<(), AuditError> {
    let root = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .canonicalize()
        .map_err(|error| AuditError::Message(format!("cannot resolve workspace root: {error}")))?;
    let mut arguments = env::args().skip(2);
    let mut api_boundary_input = None;
    let mut shared_helper_input = None;
    let mut structural_input = None;
    while let Some(argument) = arguments.next() {
        let destination = arguments
            .next()
            .map(PathBuf::from)
            .ok_or_else(|| AuditError::Message(format!("{argument} requires a path")))?;
        match argument.as_str() {
            "--api-boundary-input" => api_boundary_input = Some(destination),
            "--shared-helper-input" => shared_helper_input = Some(destination),
            "--structural-input" => structural_input = Some(destination),
            _ => {
                return Err(AuditError::Message(format!(
                    "unsupported audit argument `{argument}`"
                )))
            }
        }
    }
    let report = audit(&root, api_boundary_input.as_deref())?;
    if let Some(path) = shared_helper_input {
        write_shared_helper_input(&root, &path)?;
    }
    if let Some(path) = structural_input {
        write_structural_input(&path, &report)?;
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|error| {
            AuditError::Message(format!("cannot serialize report: {error}"))
        })?
    );
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Rust boundary AST audit failed: {error}");
            ExitCode::from(2)
        }
    }
}
