use serde::{Deserialize, Serialize};

use crate::terlan_syntax::{ebnf::EbnfSourceSpan, parse_tree::ImportKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Import kind tag used by serialized syntax output.
///
/// Inputs:
/// - Parse-tree import classification.
///
/// Outputs:
/// - Stable import kind metadata for module, file, CSS, and markdown imports.
///
/// Transformation:
/// - Serializes import categories without exposing parse-tree-specific types.
pub enum SyntaxImportKind {
    Module,
    File,
    Css,
    Markdown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// One imported symbol or module item in syntax output.
///
/// Inputs:
/// - Parsed import item name, optional alias, and span.
///
/// Outputs:
/// - Serializable import item payload for downstream name resolution.
///
/// Transformation:
/// - Preserves source aliasing and span metadata without retaining parser nodes.
pub struct SyntaxImportItem {
    pub name: String,
    pub as_alias: Option<String>,
    pub span: EbnfSourceSpan,
}

/// Returns the provider module identity for a source module import.
///
/// Inputs:
/// - `module_name`: parser-preserved module prefix.
/// - `items`: selected or default import items.
/// - `is_selected`: whether source used the explicit `{...}` selection form.
///
/// Output:
/// - Fully qualified provider module identity.
///
/// Transformation:
/// - Reconstructs default imports such as `import std.core.Task.` from parser
///   prefix `std.core` plus item `Task`.
/// - Preserves the complete provider stored by selected imports.
pub fn syntax_module_import_identity(
    module_name: &str,
    items: &[SyntaxImportItem],
    is_selected: bool,
) -> String {
    if is_selected {
        return module_name.to_string();
    }
    let Some(item) = items.first() else {
        return module_name.to_string();
    };
    if items.len() == 1
        && item.as_alias.is_none()
        && module_name
            .rsplit('.')
            .next()
            .and_then(|segment| segment.chars().next())
            .is_some_and(|first| first.is_ascii_lowercase())
    {
        format!("{module_name}.{}", item.name)
    } else {
        module_name.to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// One exported function item in interface syntax output.
///
/// Inputs:
/// - Parsed export name, arity, and source span.
///
/// Outputs:
/// - Serializable export item payload for interface processing.
///
/// Transformation:
/// - Records export surface data without carrying source tokens forward.
pub struct SyntaxExportItem {
    pub name: String,
    pub arity: usize,
    pub span: EbnfSourceSpan,
}

impl From<ImportKind> for SyntaxImportKind {
    /// Converts a parse-tree import kind into syntax-output import metadata.
    ///
    /// Inputs:
    /// - `kind`: parse-tree import classification.
    ///
    /// Output:
    /// - Equivalent syntax-output import classification.
    ///
    /// Transformation:
    /// - Performs a one-to-one enum mapping so serialized syntax output does
    ///   not expose parse-tree-specific types.
    fn from(kind: ImportKind) -> Self {
        match kind {
            ImportKind::Module => Self::Module,
            ImportKind::File => Self::File,
            ImportKind::Css => Self::Css,
            ImportKind::Markdown => Self::Markdown,
        }
    }
}
