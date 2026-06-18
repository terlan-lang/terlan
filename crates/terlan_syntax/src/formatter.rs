use crate::parse_tree::{
    AnnotationKeyOption, AnnotationSchemaDecl, AnnotationSchemaEntry, AnnotationValue, BinaryOp,
    CaseClause, ConstructorDecl, ConstructorParam, Decl, ExportDecl, Expr, FunctionClause,
    FunctionDecl, HtmlAttr, HtmlAttrValue, HtmlNode, ImportDecl, ImportKind, MapExprField,
    MapField, MethodDecl, Module, Param, Pattern, StructDecl, StructFieldDecl, TemplateDecl,
    TraitDecl, TraitImplDecl, TypeDecl, TypeExpr, UnaryOp, UnsupportedDecl,
};
use crate::parser::{parse_interface_module, parse_module, ParseError};

/// Formats canonical Terlan source text.
///
/// Inputs:
/// - `source`: raw `.terl` module text.
///
/// Output:
/// - Pretty-printed Terlan source on success.
/// - `ParseError` when the source cannot be parsed as a canonical module.
///
/// Transformation:
/// - Parses the source into the parser's private parse tree and immediately
///   renders it back to canonical source text. The parse tree is not exposed to
///   callers.
pub fn format_source_module(source: &str) -> Result<String, ParseError> {
    parse_module(source).map(|module| format_module(&module))
}

/// Formats canonical Terlan interface text.
///
/// Inputs:
/// - `source`: raw `.terli` interface summary text.
///
/// Output:
/// - Pretty-printed interface text on success.
/// - `ParseError` when the source cannot be parsed as an interface module.
///
/// Transformation:
/// - Parses interface-only declaration forms such as export summaries into the
///   parser's private parse tree and renders them without exposing that tree.
pub fn format_interface_source_module(source: &str) -> Result<String, ParseError> {
    parse_interface_module(source).map(|module| format_module(&module))
}

/// Formats a parsed Terlan module or interface parse tree back into source text.
///
/// Inputs:
/// - `module`: parsed parse tree from either the canonical `.terl` source parser or the
///   `.terli` interface parser.
///
/// Output:
/// - Pretty-printed Terlan text with a module header and formatted declarations.
///
/// Transformation:
/// - Walks declarations in source order and delegates each declaration to the
///   matching formatter. Normal `.terl` parsing rejects `Decl::Export`; if export
///   declarations appear here they are interface summaries from `.terli` parsing.
pub(crate) fn format_module(module: &Module) -> String {
    let mut out = String::new();
    out.push_str("module ");
    out.push_str(&module.name);
    out.push_str(".\n\n");

    for (i, decl) in module.declarations.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&format_decl(decl));
        out.push('\n');
    }

    out
}

/// Formats one parsed declaration.
///
/// Inputs:
/// - `decl`: parse tree declaration to format.
///
/// Output:
/// - Declaration source text including its terminating period or block terminator.
///
/// Transformation:
/// - Dispatches by declaration variant. `Decl::Export` is retained only so
///   interface modules can round-trip export summaries; canonical `.terl` source
///   uses declaration-site `pub`.
fn format_decl(decl: &Decl) -> String {
    match decl {
        Decl::Import(import) => format_import(import),
        Decl::Export(export) => format_export(export),
        Decl::Type(ty) => format_type_decl(ty),
        Decl::Function(function) => format_function(function),
        Decl::Method(method) => format_method(method),
        Decl::Trait(trait_decl) => format_trait_decl(trait_decl),
        Decl::TraitImpl(trait_impl_decl) => format_trait_impl_decl(trait_impl_decl),
        Decl::AnnotationSchema(annotation_schema_decl) => {
            format_annotation_schema_decl(annotation_schema_decl)
        }
        Decl::Template(template_decl) => format_template_decl(template_decl),
        Decl::Struct(struct_decl) => format_struct_decl(struct_decl),
        Decl::Constructor(constructor) => format_constructor_decl(constructor),
        Decl::Raw(raw) => format_raw_decl(raw),
    }
}

/// Formats an annotation schema declaration.
///
/// Inputs:
/// - `schema`: parsed annotation schema declaration.
///
/// Output:
/// - Terlan source text for the schema declaration.
///
/// Transformation:
/// - Emits the path and each schema entry in declaration order, preserving
///   public visibility and terminating the declaration with `.`.
fn format_annotation_schema_decl(schema: &AnnotationSchemaDecl) -> String {
    let mut out = String::new();
    if schema.is_public {
        out.push_str("pub ");
    }
    out.push_str("annotation ");
    out.push_str(&schema.path.join("."));
    out.push_str(" {\n");
    for entry in &schema.entries {
        out.push_str("    ");
        out.push_str(&format_annotation_schema_entry(entry));
        out.push('\n');
    }
    out.push_str("}.");
    out
}

/// Formats one annotation schema body entry.
///
/// Inputs:
/// - `entry`: parsed schema entry.
///
/// Output:
/// - Source text for the entry including its terminating semicolon.
///
/// Transformation:
/// - Converts target-set and key-schema entries into the canonical block body
///   spelling used by the formatter.
fn format_annotation_schema_entry(entry: &AnnotationSchemaEntry) -> String {
    match entry {
        AnnotationSchemaEntry::AppliesTo { targets, .. } => {
            format!("applies_to: {};", format_annotation_targets(targets))
        }
        AnnotationSchemaEntry::Key {
            key,
            value_type,
            options,
            ..
        } => {
            let mut out = format!("{}: {}", key.join("."), value_type.text);
            if !options.is_empty() {
                out.push_str(" { ");
                out.push_str(
                    &options
                        .iter()
                        .map(format_annotation_key_option)
                        .collect::<Vec<_>>()
                        .join("; "),
                );
                out.push_str(" }");
            }
            out.push(';');
            out
        }
    }
}

/// Formats one annotation key option.
///
/// Inputs:
/// - `option`: parsed schema key option.
///
/// Output:
/// - Source text for the option without a trailing separator.
///
/// Transformation:
/// - Converts typed option values back to their schema-block spelling.
fn format_annotation_key_option(option: &AnnotationKeyOption) -> String {
    match option {
        AnnotationKeyOption::Required { value, .. } => format!("required: {value}"),
        AnnotationKeyOption::Repeatable { value, .. } => format!("repeatable: {value}"),
        AnnotationKeyOption::Default { value, .. } => {
            format!("default: {}", format_annotation_value(value))
        }
        AnnotationKeyOption::AppliesTo { targets, .. } => {
            format!("applies_to: {}", format_annotation_targets(targets))
        }
    }
}

/// Formats a target set in schema syntax.
///
/// Inputs:
/// - `targets`: one or more declaration target names.
///
/// Output:
/// - A single target or bracketed target list.
///
/// Transformation:
/// - Keeps single-target schemas compact and formats multiple targets as a
///   comma-separated list.
fn format_annotation_targets(targets: &[String]) -> String {
    if targets.len() == 1 {
        return targets[0].clone();
    }
    format!("[{}]", targets.join(", "))
}

/// Formats an annotation metadata value.
///
/// Inputs:
/// - `value`: parsed annotation metadata value.
///
/// Output:
/// - Source text for the value.
///
/// Transformation:
/// - Recursively formats lists and objects while preserving literal text for
///   numeric and string values.
fn format_annotation_value(value: &AnnotationValue) -> String {
    match value {
        AnnotationValue::Name(segments) => segments.join("."),
        AnnotationValue::Bool(value) => value.to_string(),
        AnnotationValue::Int(text)
        | AnnotationValue::Float(text)
        | AnnotationValue::String(text) => text.clone(),
        AnnotationValue::List(values) => format!(
            "[{}]",
            values
                .iter()
                .map(format_annotation_value)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        AnnotationValue::Object(entries) => format!(
            "{{ {} }}",
            entries
                .iter()
                .map(|entry| format!(
                    "{}: {}",
                    entry.key.join("."),
                    format_annotation_value(&entry.value)
                ))
                .collect::<Vec<_>>()
                .join("; ")
        ),
    }
}

/// Formats an import declaration.
///
/// Inputs: parsed import declaration. Output: canonical import source text.
/// Transformation: handles asset imports, type imports, selected imports, and
/// redundant default type import spelling.
fn format_import(import: &ImportDecl) -> String {
    if matches!(
        import.kind,
        ImportKind::File | ImportKind::Css | ImportKind::Markdown
    ) {
        let keyword = match import.kind {
            ImportKind::File => "file",
            ImportKind::Css => "css",
            ImportKind::Markdown => "markdown",
            ImportKind::Module => unreachable!("module imports are formatted below"),
        };
        let path = import.source_path.as_deref().unwrap_or_default();
        let alias = import
            .items
            .first()
            .map(|item| item.name.as_str())
            .unwrap_or_default();
        return format!("import {keyword} \"{}\" as {}.", escape_string(path), alias);
    }

    let mut out = String::from("import ");
    if import.is_type {
        out.push_str("type ");
    }
    out.push_str(&import.module_name);
    out.push('.');

    if import.items.len() == 1 {
        if import.is_type && is_redundant_default_type_import(import) {
            out.push_str(&import.module_name);
            out.push('.');
            return out;
        }
        out.push(' ');
        out.push_str(&format_import_item(&import.items[0]));
    } else {
        out.push(' ');
        out.push('{');
        out.push_str(
            &import
                .items
                .iter()
                .map(format_import_item)
                .collect::<Vec<_>>()
                .join(", "),
        );
        out.push('}');
    }

    out.push('.');
    out
}

/// Returns whether a type import repeats its default exported type name.
///
/// Inputs: parsed import declaration. Output: redundancy flag. Transformation:
/// compares the selected item with the module's final segment when no alias is
/// present.
fn is_redundant_default_type_import(import: &ImportDecl) -> bool {
    let Some(item) = import.items.first() else {
        return false;
    };
    item.as_alias.is_none()
        && import
            .module_name
            .rsplit('.')
            .next()
            .is_some_and(|last_segment| last_segment == item.name)
}

/// Formats one selected import item.
///
/// Inputs: parsed import item. Output: source item text. Transformation:
/// appends `as Alias` only when an alias is present.
fn format_import_item(item: &crate::parse_tree::ImportItem) -> String {
    let mut text = String::from(&item.name);
    if let Some(alias) = &item.as_alias {
        text.push(' ');
        text.push_str("as ");
        text.push_str(alias);
    }
    text
}

/// Formats a template declaration.
///
/// Inputs: parsed template declaration. Output: canonical template source text.
/// Transformation: emits source path and ordered props, using an empty block
/// when no props exist.
fn format_template_decl(template: &TemplateDecl) -> String {
    let mut out = format!(
        "template {} from \"{}\"",
        template.name,
        escape_string(&template.source_path)
    );
    if template.props.is_empty() {
        out.push_str(" {}.");
        return out;
    }

    out.push_str(" {\n");
    for (index, prop) in template.props.iter().enumerate() {
        out.push_str("    ");
        out.push_str(&prop.name);
        out.push_str(": ");
        out.push_str(&prop.annotation.text);
        if index + 1 < template.props.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("}.");
    out
}

/// Escapes string content for quoted source output.
///
/// Inputs: raw string value. Output: escaped string payload without outer
/// quotes. Transformation: escapes backslash, quote, newline, carriage return,
/// and tab.
fn escape_string(value: &str) -> String {
    value
        .chars()
        .flat_map(|ch| match ch {
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\n' => "\\n".chars().collect::<Vec<_>>(),
            '\r' => "\\r".chars().collect::<Vec<_>>(),
            '\t' => "\\t".chars().collect::<Vec<_>>(),
            _ => vec![ch],
        })
        .collect()
}

/// Formats an interface export summary declaration.
///
/// Inputs:
/// - `export`: interface export summary containing `name/arity` entries.
///
/// Output:
/// - `.terli` export summary source text.
///
/// Transformation:
/// - Joins export items as `name/arity` entries. This is intentionally an
///   interface formatter path; normal source modules must use `pub` declarations
///   instead of export lists.
fn format_export(export: &ExportDecl) -> String {
    if export.items.is_empty() {
        return "export {}.".to_string();
    }

    let items: Vec<String> = export
        .items
        .iter()
        .map(|item| format!("{}/{}", item.name, item.arity))
        .collect();

    format!("export {}.", items.join(", "))
}

/// Formats a type declaration.
///
/// Inputs: parsed type declaration. Output: canonical type source text.
/// Transformation: emits visibility, opacity, params, implements clauses, and
/// union variants with stable indentation.
fn format_type_decl(ty: &TypeDecl) -> String {
    let mut out = String::new();
    if ty.is_public {
        out.push_str("pub ");
    }

    if ty.is_opaque {
        out.push_str("opaque ");
    }

    out.push_str("type ");
    out.push_str(&ty.name);

    if !ty.params.is_empty() {
        out.push('[');
        out.push_str(&ty.params.join(", "));
        out.push(']');
    }

    if !ty.implements.is_empty() {
        out.push_str(" implements ");
        out.push_str(
            &ty.implements
                .iter()
                .map(format_type_expr)
                .collect::<Vec<_>>()
                .join(", "),
        );
    }

    if ty.variants.is_empty() {
        out.push('.');
        return out;
    }

    out.push_str(" =\n");
    for (i, variant) in ty.variants.iter().enumerate() {
        if i == 0 {
            out.push_str("      ");
            out.push_str(&format_type_expr(variant));
        } else {
            out.push_str("\n    | ");
            out.push_str(&format_type_expr(variant));
        }
    }
    out.push('.');
    out
}

/// Formats a struct declaration.
///
/// Inputs: parsed struct declaration. Output: canonical struct source text.
/// Transformation: emits visibility, derives/implements clauses, and fields in
/// source order.
fn format_struct_decl(decl: &StructDecl) -> String {
    let mut out = String::new();
    if decl.is_public {
        out.push_str("pub ");
    }
    out.push_str("struct ");
    out.push_str(&decl.name);
    if !decl.derives.is_empty() {
        out.push_str(" derives ");
        out.push_str(&decl.derives.join(", "));
    }
    if !decl.implements.is_empty() {
        out.push_str(" implements ");
        out.push_str(
            &decl
                .implements
                .iter()
                .map(format_type_expr)
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    out.push_str(" {\n");

    for field in &decl.fields {
        out.push_str("    ");
        out.push_str(&format_struct_field(field));
        out.push('\n');
    }
    out.push_str("}.");
    out
}

/// Formats a struct field.
///
/// Inputs: parsed struct field. Output: source field text. Transformation:
/// emits name/type and optional default expression.
fn format_struct_field(field: &StructFieldDecl) -> String {
    let mut out = String::new();
    out.push_str(&field.name);
    out.push_str(": ");
    out.push_str(&format_type_expr(&field.annotation));
    if let Some(default) = &field.default {
        out.push_str(" = ");
        out.push_str(&format_expr(default, 0));
    }
    out
}

/// Formats a constructor declaration.
///
/// Inputs: parsed constructor declaration. Output: canonical constructor block.
/// Transformation: emits visibility, type params, clauses, params, return
/// types, and bodies with stable separators.
fn format_constructor_decl(decl: &ConstructorDecl) -> String {
    let mut out = String::new();
    if decl.is_public {
        out.push_str("pub ");
    }
    out.push_str("constructor ");
    out.push_str(&decl.name);
    if !decl.params.is_empty() {
        out.push('[');
        out.push_str(&decl.params.join(", "));
        out.push(']');
    }
    out.push_str(" {\n");

    for (index, clause) in decl.clauses.iter().enumerate() {
        out.push_str("    (");
        out.push_str(
            &clause
                .params
                .iter()
                .map(format_constructor_param)
                .collect::<Vec<_>>()
                .join(", "),
        );
        out.push_str("): ");
        out.push_str(&format_type_expr(&clause.return_type));
        out.push_str(" ->\n        ");
        out.push_str(&format_expr(&clause.body, 2));
        if index + 1 < decl.clauses.len() {
            out.push_str(";\n\n");
        } else {
            out.push('\n');
        }
    }

    out.push_str("}.");
    out
}

/// Formats one constructor parameter.
///
/// Inputs: parsed constructor parameter. Output: source parameter text.
/// Transformation: emits varargs marker, name/type, and optional default.
fn format_constructor_param(param: &ConstructorParam) -> String {
    let mut out = String::new();
    if param.is_varargs {
        out.push_str("...");
    }
    out.push_str(&param.name);
    out.push_str(": ");
    out.push_str(&format_type_expr(&param.annotation));
    if let Some(default) = &param.default {
        out.push_str(" = ");
        out.push_str(&format_expr(default, 0));
    }
    out
}

/// Formats a function declaration.
///
/// Inputs: parsed function declaration. Output: canonical function source.
/// Transformation: handles bodyless signatures, single-clause inline bodies,
/// and multi-clause function bodies.
fn format_function(function: &FunctionDecl) -> String {
    let mut out = String::new();
    if function.is_public {
        out.push_str("pub ");
        if function.is_macro {
            out.push_str("macro ");
        }
    }

    if function.clauses.is_empty() {
        out.push_str(&format_function_signature(
            &function.name,
            &function.params,
            &function.return_type,
        ));
        out.push('.');
        return out;
    }

    if function.clauses.len() == 1 && single_clause_matches_header(function) {
        out.push_str(&format_function_signature(
            &function.name,
            &function.params,
            &function.return_type,
        ));
        out.push_str(" ->\n    ");
        out.push_str(&format_expr(&function.clauses[0].body, 1));
        out.push('.');
        return out;
    }

    out.push_str(&format_function_signature(
        &function.name,
        &function.params,
        &function.return_type,
    ));
    out.push('.');
    out.push('\n');

    for (i, clause) in function.clauses.iter().enumerate() {
        out.push_str(&format_function_clause(function, clause));
        if i + 1 < function.clauses.len() {
            out.push_str(";\n");
        }
    }

    if !out.ends_with('.') {
        out.push('.');
    }

    out
}

/// Formats a receiver-method declaration.
///
/// Inputs:
/// - `method`: structured method declaration containing receiver, method
///   params, return type, and body clauses.
///
/// Output:
/// - Canonical Terlan receiver-method source text.
///
/// Transformation:
/// - Renders the receiver as `(name: Type)` or `(mut name: Type)` before the
///   method name and formats the first body clause as a declaration body.
///   Multi-clause receiver methods are not currently produced by the parser, so
///   only the first clause is emitted.
fn format_method(method: &MethodDecl) -> String {
    let mut out = String::new();
    if method.is_public {
        out.push_str("pub ");
    }
    out.push('(');
    if method.receiver.is_mutable {
        out.push_str("mut ");
    }
    out.push_str(&method.receiver.name);
    out.push_str(": ");
    out.push_str(&format_type_expr(&method.receiver.annotation));
    out.push_str(") ");
    out.push_str(&format_function_signature(
        &method.name,
        &method.params,
        &method.return_type,
    ));
    if let Some(clause) = method.clauses.first() {
        out.push_str(" ->\n    ");
        out.push_str(&format_expr(&clause.body, 1));
    }
    out.push('.');
    out
}

/// Formats a function signature.
///
/// Inputs: function name, params, and return type. Output: `name(params):
/// Type` text. Transformation: formats params in source order and normalizes
/// type expressions through `format_type_expr`.
fn format_function_signature(name: &str, params: &[Param], ret: &TypeExpr) -> String {
    let mut out = String::new();
    out.push_str(name);
    out.push('(');
    out.push_str(
        &params
            .iter()
            .map(|param| format!("{}: {}", param.name, format_type_expr(&param.annotation)))
            .collect::<Vec<_>>()
            .join(", "),
    );
    out.push(')');
    out.push_str(": ");
    out.push_str(&format_type_expr(ret));
    out
}

/// Formats a trait declaration.
///
/// Inputs: parsed trait declaration. Output: canonical trait source text.
/// Transformation: emits visibility, params, super traits, method signatures,
/// and default bodies.
fn format_trait_decl(trait_decl: &TraitDecl) -> String {
    let mut out = String::new();
    if trait_decl.is_public {
        out.push_str("pub ");
    }
    out.push_str("trait ");
    out.push_str(&trait_decl.name);
    if !trait_decl.params.is_empty() {
        out.push('[');
        out.push_str(&trait_decl.params.join(", "));
        out.push(']');
    }
    if !trait_decl.super_traits.is_empty() {
        out.push_str(" extends ");
        out.push_str(&trait_decl.super_traits.join(", "));
    }
    out.push_str(" {\n");
    for method in &trait_decl.methods {
        out.push_str("    ");
        out.push_str(&method.name);
        if !method.params.is_empty() {
            out.push('(');
            out.push_str(
                &method
                    .params
                    .iter()
                    .map(|param| format!("{}: {}", param.name, param.annotation.text))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            out.push(')');
        } else {
            out.push_str("()");
        }
        out.push_str(": ");
        out.push_str(&method.return_type.text);
        if let Some(default_body) = &method.default_body {
            out.push_str(" ->\n        ");
            out.push_str(&format_expr(default_body, 2));
        }
        out.push_str(".\n");
    }
    out.push_str("}.");
    out
}

/// Formats an explicit trait conformance declaration.
///
/// Inputs:
/// - `trait_impl`: parsed conformance block with trait reference, target type,
///   visibility, and method declarations.
///
/// Output:
/// - Canonical `impl TraitRef for Type { ... }.` source text.
///
/// Transformation:
/// - Renders each implementation method using the normal function formatter
///   without declaration-site `pub`, because visibility belongs to the impl
///   declaration itself.
fn format_trait_impl_decl(trait_impl: &TraitImplDecl) -> String {
    let mut out = String::new();
    if trait_impl.is_public {
        out.push_str("pub ");
    }
    out.push_str("impl ");
    out.push_str(&format_type_expr(&trait_impl.trait_ref));
    out.push_str(" for ");
    out.push_str(&format_type_expr(&trait_impl.for_type));
    out.push_str(" {\n");
    for method in &trait_impl.methods {
        let mut method = method.clone();
        method.is_public = false;
        for line in format_function(&method).lines() {
            out.push_str("    ");
            out.push_str(line);
            out.push('\n');
        }
    }
    out.push_str("}.");
    out
}

/// Formats a raw/unsupported declaration.
///
/// Inputs: raw declaration payload. Output: raw text with terminating period.
/// Transformation: preserves raw declaration text exactly apart from appending
/// the declaration terminator.
fn format_raw_decl(raw: &UnsupportedDecl) -> String {
    format!("{}.", raw.text)
}

/// Formats one multi-clause function clause.
///
/// Inputs: parent function metadata and parsed clause. Output: source clause
/// text. Transformation: uses the parent function name and clause patterns,
/// optional guard, and body.
fn format_function_clause(function: &FunctionDecl, clause: &FunctionClause) -> String {
    let mut out = String::new();
    out.push_str(&function.name);
    out.push('(');
    out.push_str(
        &clause
            .patterns
            .iter()
            .map(format_pattern)
            .collect::<Vec<_>>()
            .join(", "),
    );
    out.push(')');

    if let Some(guard) = &clause.guard {
        out.push(' ');
        out.push_str("when");
        out.push(' ');
        out.push_str(&format_expr(guard, 1));
    }

    out.push_str(" ->\n    ");
    out.push_str(&format_expr(&clause.body, 1));

    out
}

/// Formats a pattern.
///
/// Inputs: parsed pattern. Output: canonical pattern text. Transformation:
/// recursively formats tuples, lists, cons patterns, maps, and records.
fn format_pattern(pattern: &Pattern) -> String {
    match pattern {
        Pattern::Wildcard => "_".to_string(),
        Pattern::Var(name) => name.clone(),
        Pattern::Int(value) => value.to_string(),
        Pattern::Float(value) => value.to_string(),
        Pattern::Atom(value) => value.clone(),
        Pattern::Tuple(items) => {
            let parts = items
                .iter()
                .map(format_pattern)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{{}}}", parts)
        }
        Pattern::List(items) => {
            let parts = items
                .iter()
                .map(format_pattern)
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{}]", parts)
        }
        Pattern::ListCons(head, tail) => {
            format!("[{} | {}]", format_pattern(head), format_pattern(tail))
        }
        Pattern::Map(fields) => {
            if fields.is_empty() {
                "#{}".to_string()
            } else {
                let body = fields
                    .iter()
                    .map(format_map_field)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("#{{{}}}", body)
            }
        }
        Pattern::Record { name, fields } => {
            let body = fields
                .iter()
                .map(format_record_pattern_field)
                .collect::<Vec<_>>()
                .join(", ");
            format!("#{}{{{}}}", name, body)
        }
    }
}

/// Formats a record pattern field.
///
/// Inputs: parsed pattern field. Output: `key = pattern` text. Transformation:
/// recursively formats the field pattern value.
fn format_record_pattern_field(field: &MapField) -> String {
    format!("{} = {}", field.key, format_pattern(&field.value))
}

/// Formats a map pattern field.
///
/// Inputs: parsed map pattern field. Output: key/operator/value text.
/// Transformation: chooses `:=` for required fields and `=>` otherwise.
fn format_map_field(field: &MapField) -> String {
    let sep = if field.required { ":=" } else { "=>" };
    format!("{}{}{}", field.key, sep, format_pattern(&field.value))
}

/// Formats a map expression field.
///
/// Inputs: parsed map expression field. Output: key/operator/value text.
/// Transformation: chooses `:=` for required fields and recursively formats the
/// value expression.
fn format_map_expr_field(field: &MapExprField) -> String {
    let sep = if field.required { ":=" } else { "=>" };
    format!("{}{}{}", field.key, sep, format_expr(&field.value, 0))
}

/// Formats a template or record construction field.
///
/// Inputs: parsed expression field. Output: `key = expr` text. Transformation:
/// recursively formats the value expression.
fn format_template_expr_field(field: &MapExprField) -> String {
    format!("{} = {}", field.key, format_expr(&field.value, 0))
}

/// Formats a type expression.
///
/// Inputs: parsed type expression. Output: source type text. Transformation:
/// trims whitespace and substitutes `Dynamic` for empty type text.
fn format_type_expr(ty: &TypeExpr) -> String {
    let mut text = ty.text.trim().to_string();
    if text.is_empty() {
        text.push_str("Dynamic");
    }
    text
}

/// Formats an expression.
///
/// Inputs: parsed expression and indentation level. Output: canonical
/// expression text. Transformation: recursively formats expression variants and
/// uses indentation for block-like forms.
fn format_expr(expr: &Expr, indent: usize) -> String {
    let spacing = "    ".repeat(indent);
    match expr {
        Expr::Int(value) => value.to_string(),
        Expr::Float(value) => value.to_string(),
        Expr::Atom(value) => value.clone(),
        Expr::AtomLiteral(value) => format!(
            "Atom[\"{}\"]",
            value.replace('\\', "\\\\").replace('"', "\\\"")
        ),
        Expr::Binary(value) => value.clone(),
        Expr::Var(name) => name.clone(),
        Expr::Tuple(items) => {
            let body = items
                .iter()
                .map(|item| format_expr(item, 0))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{{}}}", body)
        }
        Expr::List(items) => {
            let body = items
                .iter()
                .map(|item| format_expr(item, 0))
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{}]", body)
        }
        Expr::FixedArray(items) => {
            let body = items
                .iter()
                .map(|item| format_expr(item, 0))
                .collect::<Vec<_>>()
                .join(", ");
            format!("#[{}]", body)
        }
        Expr::ListCons(head, tail) => {
            format!("[{} | {}]", format_expr(head, 0), format_expr(tail, 0))
        }
        Expr::Index(value, index) => {
            format!("{}[{}]", format_expr(value, 0), format_expr(index, 0))
        }
        Expr::IndexAssign {
            collection,
            index,
            value,
        } => format!(
            "{}[{}] = {}",
            format_expr(collection, 0),
            format_expr(index, 0),
            format_expr(value, 0)
        ),
        Expr::Map(fields) => {
            if fields.is_empty() {
                "#{}".to_string()
            } else {
                let body = fields
                    .iter()
                    .map(format_map_expr_field)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("#{{{}}}", body)
            }
        }
        Expr::RecordAccess { value, name, field } => {
            format!("{}#{}.{}", format_expr(value, 0), name, field)
        }
        Expr::FieldAccess { value, field } => {
            format!("{}.{}", format_expr(value, 0), field)
        }
        Expr::RecordUpdate {
            value,
            name,
            fields,
        } => {
            let body = fields
                .iter()
                .map(format_map_expr_field)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}#{}{{{}}}", format_expr(value, 0), name, body)
        }
        Expr::RecordConstruct { name, fields } => {
            let body = fields
                .iter()
                .map(format_map_expr_field)
                .collect::<Vec<_>>()
                .join(", ");
            format!("#{}{{{}}}", name, body)
        }
        Expr::TemplateInstantiate { name, fields } => {
            let body = fields
                .iter()
                .map(format_template_expr_field)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{} {{{}}}", name, body)
        }
        Expr::ConstructorChain { base, record } => {
            format!("{} with {}", format_expr(base, 0), format_expr(record, 0))
        }
        Expr::ListComprehension {
            expr,
            pattern,
            source,
            guard: _,
        } => {
            let pattern_text = format_pattern(pattern);
            let src = format_expr(source, 0);
            let value = format_expr(expr, 0);
            format!("[{} || {} <- {}]", value, pattern_text, src)
        }
        Expr::Let { bindings, body } => {
            let mut parts = bindings
                .iter()
                .map(|binding| format!("{} = {}", binding.name, format_expr(&binding.value, 0)))
                .collect::<Vec<_>>();
            if let Some(body) = body {
                parts.push(format_expr(body, 0));
            }
            format!("let {}", parts.join("; "))
        }
        Expr::Sequence(expressions) => expressions
            .iter()
            .map(|expr| format_expr(expr, 0))
            .collect::<Vec<_>>()
            .join("; "),
        Expr::Call {
            callee,
            args,
            remote,
            is_fun_value,
        } => {
            let args_text = args
                .iter()
                .map(|arg| format_expr(arg, 0))
                .collect::<Vec<_>>()
                .join(", ");
            if let Some(remote) = remote {
                format!("{}.{}({})", remote, format_expr(callee, 0), args_text)
            } else if *is_fun_value {
                format!("{}.({})", format_expr(callee, 0), args_text)
            } else {
                format!("{}({})", format_expr(callee, 0), args_text)
            }
        }
        Expr::Case { scrutinee, clauses } => {
            let mut out = String::new();
            out.push_str(&format!("case {} {{\n", format_expr(scrutinee, 0)));
            for (i, clause) in clauses.iter().enumerate() {
                out.push_str(&spacing);
                out.push_str(&format_case_clause(clause));
                if i + 1 < clauses.len() {
                    out.push(';');
                }
                out.push('\n');
            }
            out.push_str(&spacing);
            out.push('}');
            out
        }
        Expr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            let mut out = format!("try {} {{", format_expr(body, indent + 1));
            if !of_clauses.is_empty() {
                out.push('\n');
                for (i, clause) in of_clauses.iter().enumerate() {
                    out.push_str(&spacing);
                    out.push_str(&format_case_clause(clause));
                    if i + 1 < of_clauses.len() {
                        out.push(';');
                    }
                    out.push('\n');
                }
            }
            if !catch_clauses.is_empty() {
                out.push_str("catch\n");
                for (i, clause) in catch_clauses.iter().enumerate() {
                    out.push_str(&spacing);
                    out.push_str(&format_case_clause(clause));
                    if i + 1 < catch_clauses.len() {
                        out.push(';');
                    }
                    out.push('\n');
                }
            }
            if let Some(after) = after_clause {
                out.push_str("after ");
                out.push_str(&spacing);
                out.push_str(&format!(
                    "{} -> {}\n",
                    format_expr(&after.trigger, indent + 1),
                    format_expr(&after.body, indent + 1)
                ));
            }
            out.push_str(&spacing);
            out.push('}');
            out
        }
        Expr::If { clauses } => {
            let mut out = String::from("if {\n");
            for (i, clause) in clauses.iter().enumerate() {
                out.push_str(&spacing);
                out.push_str(&format!(
                    "{} -> {}",
                    format_expr(&clause.condition, 0),
                    format_expr(&clause.body, indent + 1)
                ));
                if i + 1 < clauses.len() {
                    out.push(';');
                }
                out.push('\n');
            }
            out.push_str(&spacing);
            out.push('}');
            out
        }
        Expr::Fun { clauses } => clauses
            .first()
            .map(|clause| {
                format!(
                    "({}) -> {}",
                    clause
                        .patterns
                        .iter()
                        .map(format_pattern)
                        .collect::<Vec<_>>()
                        .join(", "),
                    format_expr(&clause.body, indent + 1)
                )
            })
            .unwrap_or_else(|| "() -> {}".to_string()),
        Expr::MacroCall { name, args } if args.is_empty() => format!("?{}", name),
        Expr::MacroCall { name, args } => format!(
            "?{}({})",
            name,
            args.iter()
                .map(|arg| format_expr(arg, 0))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Expr::RawMacro { name, raw } => format!("{} {{{}}}", name, raw),
        Expr::BinaryOp { op, left, right } => {
            format!(
                "{} {} {}",
                format_expr(left, 0),
                op_text(op),
                format_expr(right, 0)
            )
        }
        Expr::UnaryOp { op, expr } => match op {
            UnaryOp::Neg => format!("-{}", format_expr(expr, 0)),
            UnaryOp::Not => format!("not {}", format_expr(expr, 0)),
            UnaryOp::Bang => format!("!{}", format_expr(expr, 0)),
        },
        Expr::Cast { expr, target_type } => {
            format!("{} as {}", format_expr(expr, 0), target_type.text)
        }
        Expr::Quote(expr) => format!("quote {}", format_expr(expr, 0)),
        Expr::Unquote(expr) => format!("unquote({})", format_expr(expr, 0)),
        Expr::HtmlBlock(block) => format_html_block(block.macro_kind.name(), &block.nodes, indent),
    }
}

/// Formats an HTML/raw block expression.
///
/// Inputs: macro/block name, HTML nodes, and indentation. Output: block source
/// text. Transformation: formats children one per line and closes at the parent
/// indentation level.
fn format_html_block(name: &str, nodes: &[HtmlNode], indent: usize) -> String {
    let spacing = "    ".repeat(indent);
    let mut out = format!("{name} {{\n");
    for node in nodes {
        out.push_str(&format_html_node(node, indent + 1));
        out.push('\n');
    }
    out.push_str(&spacing);
    out.push('}');
    out
}

/// Formats one HTML node.
///
/// Inputs: parsed HTML node and indentation. Output: HTML source fragment.
/// Transformation: formats text, interpolation, named slots, and elements
/// recursively.
fn format_html_node(node: &HtmlNode, indent: usize) -> String {
    let spacing = "    ".repeat(indent);
    match node {
        HtmlNode::Text(text) => format!("{}{}", spacing, text),
        HtmlNode::Expr(expr) => format!("{}{{{}}}", spacing, format_expr(expr, indent)),
        HtmlNode::NamedSlot(slot) => {
            let mut out = format!("{}@{} {{\n", spacing, slot.name);
            for child in &slot.children {
                out.push_str(&format_html_node(child, indent + 1));
                out.push('\n');
            }
            out.push_str(&spacing);
            out.push('}');
            out
        }
        HtmlNode::Element(element) => {
            let attrs = format_html_attrs(&element.attrs);
            if element.children.is_empty() {
                return format!("{}<{}{} />", spacing, element.name, attrs);
            }

            let mut out = format!("{}<{}{}>\n", spacing, element.name, attrs);
            for child in &element.children {
                out.push_str(&format_html_node(child, indent + 1));
                out.push('\n');
            }
            out.push_str(&spacing);
            out.push_str("</");
            out.push_str(&element.name);
            out.push('>');
            out
        }
    }
}

/// Formats HTML attributes.
///
/// Inputs: parsed attributes. Output: sorted attribute source text.
/// Transformation: sorts by attribute name for deterministic output and formats
/// static or expression values.
fn format_html_attrs(attrs: &[HtmlAttr]) -> String {
    let mut attrs = attrs.iter().collect::<Vec<_>>();
    attrs.sort_by(|left, right| left.name.cmp(&right.name));
    attrs
        .into_iter()
        .map(|attr| match &attr.value {
            None => format!(" {}", attr.name),
            Some(HtmlAttrValue::Text(value)) => format!(" {}=\"{}\"", attr.name, value),
            Some(HtmlAttrValue::Expr(expr)) => {
                format!(" {}={{{}}}", attr.name, format_expr(expr, 0))
            }
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Returns canonical source text for a binary operator.
///
/// Inputs: parser binary operator. Output: operator spelling. Transformation:
/// maps the closed operator enum to the formatter spelling.
fn op_text(op: &BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::EqEq => "==",
        BinaryOp::NotEq => "!=",
        BinaryOp::Lt => "<",
        BinaryOp::Gt => ">",
        BinaryOp::LtEq => "<=",
        BinaryOp::GtEq => ">=",
        BinaryOp::DivRem => "div",
        BinaryOp::Rem => "rem",
        BinaryOp::And => "and",
        BinaryOp::Or => "or",
        BinaryOp::PipeForward => "|>",
    }
}

/// Formats a case/try clause.
///
/// Inputs: parsed case clause. Output: `pattern [when guard] -> body` text.
/// Transformation: formats the pattern, optional guard, and body expression.
fn format_case_clause(clause: &CaseClause) -> String {
    let mut out = String::new();
    out.push_str(&format_pattern(&clause.pattern));
    if let Some(guard) = &clause.guard {
        out.push(' ');
        out.push_str("when ");
        out.push_str(&format_expr(guard, 0));
    }
    out.push_str(" -> ");
    out.push_str(&format_expr(&clause.body, 2));
    out
}

/// Returns whether a single function clause duplicates the declaration header.
///
/// Inputs: parsed function declaration. Output: `true` when the first clause
/// patterns are exactly the header parameter names. Transformation: compares
/// clause variables with params to decide compact formatting.
fn single_clause_matches_header(function: &FunctionDecl) -> bool {
    let Some(clause) = function.clauses.first() else {
        return false;
    };

    if clause.patterns.len() != function.params.len() {
        return false;
    }

    clause
        .patterns
        .iter()
        .zip(function.params.iter())
        .all(|(pattern, param)| match pattern {
            Pattern::Var(name) => name == &param.name,
            _ => false,
        })
}

#[cfg(test)]
#[path = "formatter_test.rs"]
mod formatter_test;
