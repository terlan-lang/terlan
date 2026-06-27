use std::collections::HashMap;

use crate::terlan_hir::{FunctionSignature, ModuleInterface, ParamSignature};
use crate::terlan_syntax::{
    parse_module_as_syntax_output, SyntaxDeclarationOutput, SyntaxDeclarationPayload,
    SyntaxModuleOutput, SyntaxParamOutput,
};
use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position, Url};

use crate::terlan_lsp::document::{OpenDocument, OpenDocuments};
use crate::terlan_lsp::Backend;

/// Builds hover documentation for a source position.
///
/// Inputs:
/// - `uri`: document URI used to discover packaged interface summaries.
/// - `document`: current open source document.
/// - `position`: cursor position from the editor.
///
/// Output:
/// - Markdown hover content for local or imported symbols.
/// - `None` when the position is not on a documented symbol.
///
/// Transformation:
/// - Extracts the identifier under the cursor, parses the current document,
///   then searches local syntax docs before falling back to visible
///   `.typi`/`.terli` module interfaces packaged with the compiler/stdlib.
pub(crate) fn hover_for_position(
    uri: &Url,
    document: &OpenDocument,
    position: Position,
) -> Option<Hover> {
    let byte_offset = document.byte_offset_from_position(position)?;
    let identifier = Backend::identifier_at_byte_offset(&document.text, byte_offset)?;
    let module = parse_module_as_syntax_output(&document.text).ok()?;
    let interfaces = OpenDocuments::interfaces_for_uri(uri);
    let qualifier = qualifier_before_identifier(&document.text, byte_offset);

    let content = local_hover_markdown(&module, &identifier).or_else(|| {
        interface_hover_markdown(&module, &interfaces, &identifier, qualifier.as_deref())
    })?;

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: content,
        }),
        range: None,
    })
}

/// Builds hover Markdown from declarations in the current source file.
///
/// Inputs:
/// - `module`: parsed syntax-output module.
/// - `identifier`: source identifier under the cursor.
///
/// Output:
/// - Markdown documentation for the matching local module/declaration.
/// - `None` when no local documented symbol matches.
///
/// Transformation:
/// - Converts compiler syntax-output declaration metadata into a compact
///   Markdown hover model shared by structs, types, functions, methods,
///   constructors, traits, and modules.
fn local_hover_markdown(module: &SyntaxModuleOutput, identifier: &str) -> Option<String> {
    if identifier == module.module_name || module.module_name.rsplit('.').next() == Some(identifier)
    {
        return hover_markdown(
            "module",
            &module.module_name,
            &format!("module {}.", module.module_name),
            &module.docs,
        );
    }

    module
        .declarations
        .iter()
        .find_map(|declaration| declaration_hover_markdown(declaration, identifier))
}

/// Builds hover Markdown from one source declaration.
///
/// Inputs:
/// - `declaration`: syntax-output declaration with attached docs.
/// - `identifier`: source identifier under the cursor.
///
/// Output:
/// - Markdown for the declaration if it matches the identifier.
/// - `None` for nonmatching or non-documentable declarations.
///
/// Transformation:
/// - Renders a source-like signature from syntax-output payloads and attaches
///   normalized documentation lines.
fn declaration_hover_markdown(
    declaration: &SyntaxDeclarationOutput,
    identifier: &str,
) -> Option<String> {
    let (kind, name, signature) = match &declaration.payload {
        SyntaxDeclarationPayload::Type {
            name,
            params,
            is_public,
            ..
        } if name == identifier => (
            "type",
            name.as_str(),
            format!(
                "{}type {}{}",
                visibility_prefix(*is_public),
                name,
                type_params_text(params)
            ),
        ),
        SyntaxDeclarationPayload::Struct {
            name, is_public, ..
        } if name == identifier => (
            "struct",
            name.as_str(),
            format!("{}struct {}", visibility_prefix(*is_public), name),
        ),
        SyntaxDeclarationPayload::Constructor {
            name,
            params,
            is_public,
            ..
        } if name == identifier => (
            "constructor",
            name.as_str(),
            format!(
                "{}constructor {}{}",
                visibility_prefix(*is_public),
                name,
                type_params_text(params)
            ),
        ),
        SyntaxDeclarationPayload::Function {
            name,
            params,
            return_type,
            is_public,
            ..
        } if name == identifier => (
            "function",
            name.as_str(),
            format!(
                "{}{}({}): {}",
                visibility_prefix(*is_public),
                name,
                syntax_params_text(params),
                return_type.text
            ),
        ),
        SyntaxDeclarationPayload::Method {
            receiver,
            name,
            params,
            return_type,
            is_public,
            ..
        } if name == identifier => (
            "method",
            name.as_str(),
            format!(
                "{}({}: {}) {}({}): {}",
                visibility_prefix(*is_public),
                receiver.name,
                receiver.annotation.text,
                name,
                syntax_params_text(params),
                return_type.text
            ),
        ),
        SyntaxDeclarationPayload::Trait {
            name,
            params,
            is_public,
            ..
        } if name == identifier => (
            "trait",
            name.as_str(),
            format!(
                "{}trait {}{}",
                visibility_prefix(*is_public),
                name,
                type_params_text(params)
            ),
        ),
        SyntaxDeclarationPayload::Template { name, .. } if name == identifier => {
            ("template", name.as_str(), format!("template {name}"))
        }
        _ => return None,
    };
    hover_markdown(kind, name, &signature, &declaration.docs)
}

/// Builds hover Markdown from packaged module interfaces.
///
/// Inputs:
/// - `module`: current syntax-output module, used to scope imports.
/// - `interfaces`: visible packaged/local interface summaries.
/// - `identifier`: source identifier under the cursor.
/// - `qualifier`: optional dotted prefix immediately before the identifier.
///
/// Output:
/// - Markdown documentation for imported modules, types, functions, and
///   receiver methods.
/// - `None` when no visible interface exposes a matching documented symbol.
///
/// Transformation:
/// - Restricts imported hover candidates to explicit source imports when
///   available, while still allowing fully qualified module names from the
///   packaged interface map.
fn interface_hover_markdown(
    module: &SyntaxModuleOutput,
    interfaces: &HashMap<String, ModuleInterface>,
    identifier: &str,
    qualifier: Option<&str>,
) -> Option<String> {
    let imported_modules = imported_modules(module);
    if let Some(qualifier) = qualifier {
        if let Some((module_name, interface)) = interface_for_qualifier(interfaces, qualifier) {
            return interface_member_hover_markdown(module_name, interface, identifier)
                .or_else(|| interface_module_hover_markdown(module_name, interface, identifier));
        }
    }

    for module_name in &imported_modules {
        if let Some(interface) = interfaces.get(module_name) {
            if let Some(markdown) =
                interface_module_hover_markdown(module_name, interface, identifier)
                    .or_else(|| interface_member_hover_markdown(module_name, interface, identifier))
            {
                return Some(markdown);
            }
        }
    }

    interfaces.iter().find_map(|(module_name, interface)| {
        interface_module_hover_markdown(module_name, interface, identifier)
    })
}

/// Returns module hover Markdown when an identifier matches a module name.
///
/// Inputs:
/// - `module_name`: fully qualified module name.
/// - `interface`: packaged module interface.
/// - `identifier`: source identifier under the cursor.
///
/// Output:
/// - Module hover Markdown, or `None` for nonmatching names.
///
/// Transformation:
/// - Matches either the full module name or the default-export-style leaf.
fn interface_module_hover_markdown(
    module_name: &str,
    interface: &ModuleInterface,
    identifier: &str,
) -> Option<String> {
    if identifier == module_name || module_name.rsplit('.').next() == Some(identifier) {
        return hover_markdown(
            "module",
            module_name,
            &format!("module {module_name}."),
            &interface.docs,
        );
    }
    None
}

/// Returns member hover Markdown from one interface.
///
/// Inputs:
/// - `module_name`: fully qualified module owning the member.
/// - `interface`: packaged module interface.
/// - `identifier`: member identifier under the cursor.
///
/// Output:
/// - Hover docs for public types, constructors, functions, or methods.
/// - `None` when the member is absent.
///
/// Transformation:
/// - Renders interface metadata into source-like signatures suitable for
///   editor hovers without requiring provider source files.
fn interface_member_hover_markdown(
    module_name: &str,
    interface: &ModuleInterface,
    identifier: &str,
) -> Option<String> {
    if let Some(docs) = interface.type_docs.get(identifier) {
        if interface.public_types.contains(identifier)
            || interface.opaque_types.contains(identifier)
        {
            let params = interface
                .type_params
                .get(identifier)
                .map(|params| type_params_text(params))
                .unwrap_or_default();
            let kind = if interface.struct_fields.contains_key(identifier) {
                "struct"
            } else {
                "type"
            };
            return hover_markdown(
                kind,
                &format!("{module_name}.{identifier}"),
                &format!("pub {kind} {identifier}{params}"),
                docs,
            );
        }
    }

    if let Some(constructors) = interface
        .constructors
        .get(identifier)
        .filter(|items| !items.is_empty())
    {
        let constructor = &constructors[0];
        return hover_markdown(
            "constructor",
            &format!("{module_name}.{identifier}"),
            &format!(
                "pub constructor {}{}",
                constructor.name,
                type_params_text(&constructor.type_params)
            ),
            &constructor.docs,
        );
    }

    let signature = interface
        .functions
        .values()
        .filter(|function| function.name == identifier && function.public)
        .min_by_key(|function| function.params.len())?;
    let kind = if signature.receiver_method {
        "method"
    } else {
        "function"
    };
    hover_markdown(
        kind,
        &format!("{module_name}.{identifier}"),
        &interface_function_signature(signature),
        &signature.docs,
    )
}

/// Finds an interface by source qualifier.
///
/// Inputs:
/// - `interfaces`: visible interface map.
/// - `qualifier`: dotted source prefix before a member name.
///
/// Output:
/// - Matching interface entry when the qualifier is a full module name or
///   module leaf.
///
/// Transformation:
/// - Supports both `std.core.Bool.to_string` and imported `Bool.to_string`
///   hover shapes.
fn interface_for_qualifier<'a>(
    interfaces: &'a HashMap<String, ModuleInterface>,
    qualifier: &str,
) -> Option<(&'a str, &'a ModuleInterface)> {
    interfaces
        .iter()
        .find(|(module_name, _)| {
            module_name.as_str() == qualifier || module_name.rsplit('.').next() == Some(qualifier)
        })
        .map(|(name, interface)| (name.as_str(), interface))
}

/// Returns module names imported by a syntax-output module.
///
/// Inputs:
/// - `module`: current syntax-output module.
///
/// Output:
/// - Imported module names in source order.
///
/// Transformation:
/// - Extracts only source import declarations; selective import items are
///   resolved later against the provider interface.
fn imported_modules(module: &SyntaxModuleOutput) -> Vec<String> {
    module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Import { module_name, .. } => Some(module_name.clone()),
            _ => None,
        })
        .collect()
}

/// Returns the dotted qualifier immediately before an identifier.
///
/// Inputs:
/// - `text`: source text.
/// - `byte_offset`: cursor byte offset touching an identifier.
///
/// Output:
/// - Qualifier text before the current identifier, without the trailing dot,
///   or `None` for unqualified identifiers.
///
/// Transformation:
/// - Scans left over identifier and dot bytes to support hover on qualified
///   calls such as `Bool.to_string`.
fn qualifier_before_identifier(text: &str, byte_offset: usize) -> Option<String> {
    let identifier = Backend::identifier_at_byte_offset(text, byte_offset)?;
    let identifier_start = text[..byte_offset.min(text.len())].rfind(&identifier)?;
    let before = text[..identifier_start].trim_end();
    let before = before.strip_suffix('.')?;
    let mut start = before.len();
    let bytes = before.as_bytes();
    while start > 0 {
        let byte = bytes[start - 1];
        if Backend::is_identifier_byte(byte) || byte == b'.' {
            start -= 1;
        } else {
            break;
        }
    }
    let qualifier = before[start..].trim_matches('.');
    (!qualifier.is_empty()).then(|| qualifier.to_string())
}

/// Renders a packaged function signature.
///
/// Inputs:
/// - `signature`: function or receiver-method interface signature.
///
/// Output:
/// - Source-like Terlan signature text.
///
/// Transformation:
/// - Preserves receiver method notation and parameter annotations for editor
///   hover display.
fn interface_function_signature(signature: &FunctionSignature) -> String {
    let params = interface_params_text(&signature.params);
    if signature.receiver_method {
        let receiver = signature.params.first();
        let receiver_text = receiver
            .map(|param| format!("({}: {}) ", param.name, param.annotation))
            .unwrap_or_default();
        let rest = if signature.params.is_empty() {
            String::new()
        } else {
            interface_params_text(&signature.params[1..])
        };
        format!(
            "pub {}{}({}): {}",
            receiver_text, signature.name, rest, signature.return_type
        )
    } else {
        format!(
            "pub {}({}): {}",
            signature.name, params, signature.return_type
        )
    }
}

/// Renders syntax-output parameters.
///
/// Inputs:
/// - `params`: syntax-output parameters.
///
/// Output:
/// - Comma-separated `name: Type` parameter text.
///
/// Transformation:
/// - Drops implementation spans while preserving names and annotations.
fn syntax_params_text(params: &[SyntaxParamOutput]) -> String {
    params
        .iter()
        .map(|param| format!("{}: {}", param.name, param.annotation.text))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Renders HIR interface parameters.
///
/// Inputs:
/// - `params`: interface parameter signatures.
///
/// Output:
/// - Comma-separated `name: Type` parameter text.
///
/// Transformation:
/// - Projects packaged interface metadata into source-like display text.
fn interface_params_text(params: &[ParamSignature]) -> String {
    params
        .iter()
        .map(|param| format!("{}: {}", param.name, param.annotation))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Renders generic type parameter text.
///
/// Inputs:
/// - `params`: type parameter names.
///
/// Output:
/// - `[A, B]` when non-empty; otherwise an empty string.
///
/// Transformation:
/// - Keeps the Terlan generic syntax used by source declarations.
fn type_params_text(params: &[String]) -> String {
    if params.is_empty() {
        String::new()
    } else {
        format!("[{}]", params.join(", "))
    }
}

/// Returns a source visibility prefix.
///
/// Inputs:
/// - `is_public`: declaration visibility flag.
///
/// Output:
/// - `"pub "` for public declarations, otherwise empty.
///
/// Transformation:
/// - Normalizes hover signatures without exposing internal flags.
fn visibility_prefix(is_public: bool) -> &'static str {
    if is_public {
        "pub "
    } else {
        ""
    }
}

/// Builds Markdown hover text.
///
/// Inputs:
/// - `kind`: symbol category label.
/// - `name`: fully qualified or local display name.
/// - `signature`: Terlan source-like signature.
/// - `docs`: documentation lines.
///
/// Output:
/// - Markdown string when docs or signature are present.
///
/// Transformation:
/// - Combines a code fence, title, and normalized documentation lines into
///   LSP `MarkupKind::Markdown` content.
fn hover_markdown(kind: &str, name: &str, signature: &str, docs: &[String]) -> Option<String> {
    if docs.is_empty() && signature.is_empty() {
        return None;
    }
    let mut out = String::new();
    out.push_str(&format!("**{kind} `{name}`**\n\n"));
    if !signature.is_empty() {
        out.push_str("```terlan\n");
        out.push_str(signature);
        out.push_str("\n```\n\n");
    }
    if !docs.is_empty() {
        out.push_str(&docs.join("\n"));
    }
    Some(out.trim_end().to_string())
}

#[cfg(test)]
#[path = "hover_test.rs"]
mod hover_test;
