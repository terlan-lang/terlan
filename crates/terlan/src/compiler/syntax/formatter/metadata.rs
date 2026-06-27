use super::format_expr;
use crate::terlan_syntax::parse_tree::{
    AnnotationKeyOption, AnnotationSchemaDecl, AnnotationSchemaEntry, AnnotationValue, ExportDecl,
    ImportDecl, ImportKind, TemplateDecl,
};

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
pub(super) fn format_annotation_schema_decl(schema: &AnnotationSchemaDecl) -> String {
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
/// Inputs:
/// - `import`: parsed import declaration.
///
/// Output:
/// - Canonical import source text.
///
/// Transformation:
/// - Handles asset imports, type imports, selected imports, and redundant
///   default type import spelling.
pub(super) fn format_import(import: &ImportDecl) -> String {
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

    if import.items.len() == 1 && import.items[0].name == "*" {
        out.push_str("{*}.");
        return out;
    }

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
/// Inputs:
/// - `import`: parsed import declaration.
///
/// Output:
/// - `true` when the import item repeats its module basename.
///
/// Transformation:
/// - Compares the selected item with the module's final segment when no alias
///   is present.
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
/// Inputs:
/// - `item`: parsed import item.
///
/// Output:
/// - Source item text.
///
/// Transformation:
/// - Appends `as Alias` only when an alias is present.
fn format_import_item(item: &crate::terlan_syntax::parse_tree::ImportItem) -> String {
    if item.name == "*" {
        return "*".to_string();
    }
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
/// Inputs:
/// - `template`: parsed template declaration.
///
/// Output:
/// - Canonical template source text.
///
/// Transformation:
/// - Emits source path and ordered props, using an empty block when no props
///   exist.
pub(super) fn format_template_decl(template: &TemplateDecl) -> String {
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
        if let Some(default) = &prop.default {
            out.push_str(" = ");
            out.push_str(&format_expr(default, 0));
        }
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
/// Inputs:
/// - `value`: raw string value.
///
/// Output:
/// - Escaped string payload without outer quotes.
///
/// Transformation:
/// - Escapes backslash, quote, newline, carriage return, and tab.
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
///   interface formatter path; normal source modules must use `pub`
///   declarations instead of export lists.
pub(super) fn format_export(export: &ExportDecl) -> String {
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
