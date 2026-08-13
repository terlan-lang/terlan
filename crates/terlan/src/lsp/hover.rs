use std::collections::HashMap;

use crate::terlan_hir::{FunctionSignature, ModuleInterface, ParamSignature};
use crate::terlan_purity::{
    infer_body_available_pure_callables, syntax_declaration_callable_identity, CallableIdentity,
};
use crate::terlan_syntax::{
    SyntaxDeclarationOutput, SyntaxDeclarationPayload, SyntaxModuleOutput, SyntaxParamOutput,
    SyntaxTraitMethodOutput,
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
    let module = document.parse_syntax().ok()?;
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

    let known_pure = infer_body_available_pure_callables(module);
    module.declarations.iter().find_map(|declaration| {
        let markdown = declaration_hover_markdown(declaration, identifier, &known_pure)?;
        let is_target_type = matches!(
            &declaration.payload,
            SyntaxDeclarationPayload::Type { name, .. }
                | SyntaxDeclarationPayload::Struct { name, .. }
                if name == identifier
        );
        if !is_target_type {
            return Some(markdown);
        }
        Some(append_negative_trait_impls(
            markdown,
            local_negative_trait_impl_signatures(module, identifier),
        ))
    })
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
    known_pure: &std::collections::HashSet<CallableIdentity>,
) -> Option<String> {
    if let SyntaxDeclarationPayload::Raw { raw_kind, text } = &declaration.payload {
        let (name, signature) = raw_shape_hover_parts(raw_kind, text, identifier)?;
        return hover_markdown("shape", &name, &signature, &declaration.docs);
    }

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
            name,
            generic_params,
            is_public,
            ..
        } if name == identifier => (
            "struct",
            name.as_str(),
            format!(
                "{}struct {}{}",
                visibility_prefix(*is_public),
                name,
                type_params_text(generic_params)
            ),
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
        SyntaxDeclarationPayload::Constant {
            name,
            annotation,
            value,
            is_public,
        } if name == identifier => (
            "constant",
            name.as_str(),
            format!(
                "{}const {}: {} = {}",
                visibility_prefix(*is_public),
                name,
                annotation.text,
                value
                    .raw
                    .as_deref()
                    .or(value.text.as_deref())
                    .unwrap_or("<const value>")
            ),
        ),
        SyntaxDeclarationPayload::ConstFunction {
            name,
            params,
            return_type,
            is_public,
            ..
        } if name == identifier => (
            "const function",
            name.as_str(),
            format!(
                "{}const {}({}): {}",
                visibility_prefix(*is_public),
                name,
                syntax_params_text(params),
                return_type.text
            ),
        ),
        SyntaxDeclarationPayload::Type {
            name,
            valued_arms,
            representation,
            ..
        } if valued_arms.iter().any(|arm| arm.name == identifier) => {
            let arm = valued_arms.iter().find(|arm| arm.name == identifier)?;
            (
                "valued-union constant",
                arm.name.as_str(),
                format!(
                    "{}.{}: {} = {}",
                    name,
                    arm.name,
                    representation
                        .as_ref()
                        .map(|ty| ty.text.as_str())
                        .unwrap_or(name),
                    arm.value
                        .raw
                        .as_deref()
                        .or(arm.value.text.as_deref())
                        .unwrap_or("<const value>")
                ),
            )
        }
        SyntaxDeclarationPayload::Function {
            name,
            generic_params,
            params,
            return_type,
            is_public,
            ..
        } if name == identifier => (
            "function",
            name.as_str(),
            format!(
                "{}{}{}{}({}): {}",
                declaration_purity_prefix(declaration, known_pure),
                visibility_prefix(*is_public),
                name,
                type_params_text(generic_params),
                syntax_params_text(params),
                return_type.text
            ),
        ),
        SyntaxDeclarationPayload::Method {
            receiver,
            name,
            generic_params,
            params,
            return_type,
            is_public,
            ..
        } if name == identifier => (
            "method",
            name.as_str(),
            format!(
                "{}{}({}: {}) {}{}({}): {}",
                declaration_purity_prefix(declaration, known_pure),
                visibility_prefix(*is_public),
                receiver.name,
                receiver.annotation.text,
                name,
                type_params_text(generic_params),
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
        SyntaxDeclarationPayload::Trait { methods, .. } => {
            let method = methods.iter().find(|method| method.name == identifier)?;
            (
                "trait method",
                method.name.as_str(),
                syntax_trait_method_signature(method),
            )
        }
        SyntaxDeclarationPayload::Template { name, .. } if name == identifier => {
            ("template", name.as_str(), format!("template {name}"))
        }
        _ => return None,
    };
    hover_markdown(kind, name, &signature, &declaration.docs)
}

/// Returns the source-like purity prefix for function and method hovers.
///
/// Inputs:
/// - `declaration`: syntax-output callable declaration.
/// - `known_pure`: compiler-proven body and assertion purity identities.
///
/// Output:
/// - `"@pure\n"` for compiler-proven pure declarations, otherwise empty text.
///
/// Transformation:
/// - Projects the same inferred/asserted metadata emitted into interfaces and
///   public docs into same-document hover signatures.
fn declaration_purity_prefix(
    declaration: &SyntaxDeclarationOutput,
    known_pure: &std::collections::HashSet<CallableIdentity>,
) -> &'static str {
    if syntax_declaration_callable_identity(declaration)
        .is_some_and(|identity| known_pure.contains(&identity))
    {
        "@pure\n"
    } else {
        ""
    }
}

/// Extracts local hover metadata from a raw shape declaration.
///
/// Inputs:
/// - `raw_kind`: raw declaration kind emitted by syntax output.
/// - `text`: original raw declaration text.
/// - `identifier`: source identifier under the cursor.
///
/// Output:
/// - Shape name and source-like signature when the raw declaration is a shape
///   matching the hovered identifier.
///
/// Transformation:
/// - Keeps editor hover useful while shape expansion remains intentionally
///   rejected by typechecking.
fn raw_shape_hover_parts(raw_kind: &str, text: &str, identifier: &str) -> Option<(String, String)> {
    if raw_kind != "shape" {
        return None;
    }

    let trimmed = text.trim();
    let after_visibility =
        if let Some(rest) = trimmed.strip_prefix("pub").and_then(trim_keyword_rest) {
            rest
        } else {
            trimmed
        };
    let after_shape = after_visibility
        .strip_prefix("shape")
        .and_then(trim_keyword_rest)?;
    let name = after_shape
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect::<String>();
    if name != identifier {
        return None;
    }

    let signature = trimmed.strip_suffix('.').unwrap_or(trimmed).trim_end();
    Some((name, signature.to_string()))
}

/// Trims whitespace after a recognized keyword token.
///
/// Inputs:
/// - `rest`: source text immediately after the keyword spelling.
///
/// Output:
/// - Remaining source after required whitespace.
///
/// Transformation:
/// - Prevents prefix matches such as `publisher` or `shapeName` from being
///   treated as keyword-bearing declarations.
fn trim_keyword_rest(rest: &str) -> Option<&str> {
    let mut chars = rest.chars();
    let first = chars.next()?;
    if !first.is_whitespace() {
        return None;
    }
    Some(chars.as_str().trim_start())
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
    if let Some(constant) = interface.constants.get(identifier) {
        return hover_markdown(
            "constant",
            &format!("{module_name}.{identifier}"),
            &format!(
                "pub const {}: {} = {}\n// fingerprint: {}",
                constant.name, constant.annotation, constant.value_text, constant.fingerprint
            ),
            &constant.docs,
        );
    }

    if let Some(function) = interface
        .const_functions
        .values()
        .filter(|function| function.name == identifier)
        .min_by_key(|function| function.params.len())
    {
        return hover_markdown(
            "const function",
            &format!("{module_name}.{identifier}"),
            &format!(
                "pub const {}({}): {}\n// evaluator fingerprint: {}",
                function.name,
                interface_params_text(&function.params),
                function.return_type,
                function.fingerprint
            ),
            &function.docs,
        );
    }

    if let Some((owner, arm)) = interface.valued_unions.iter().find_map(|(owner, union)| {
        union
            .arms
            .iter()
            .find(|arm| arm.name == identifier)
            .map(|arm| (owner, arm))
    }) {
        return hover_markdown(
            "valued-union constant",
            &format!("{module_name}.{owner}.{identifier}"),
            &format!(
                "{owner}.{} = {}\n// fingerprint: {}",
                arm.name, arm.value_text, arm.fingerprint
            ),
            &[],
        );
    }

    if let Some(constant) = interface.associated_constants.values().find(|constant| {
        constant.name == identifier || constant.name.ends_with(&format!(".{identifier}"))
    }) {
        return hover_markdown(
            "trait-associated constant",
            &format!("{module_name}.{}", constant.name),
            &format!(
                "{}: {} = {}\n// fingerprint: {}",
                constant.name, constant.annotation, constant.value_text, constant.fingerprint
            ),
            &constant.docs,
        );
    }

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
            let markdown = hover_markdown(
                kind,
                &format!("{module_name}.{identifier}"),
                &format!("pub {kind} {identifier}{params}"),
                docs,
            )?;
            return Some(append_negative_trait_impls(
                markdown,
                interface_negative_trait_impl_signatures(interface, identifier),
            ));
        }
    }

    if let Some(shape) = interface.shapes.get(identifier) {
        return hover_markdown(
            "shape",
            &format!("{module_name}.{identifier}"),
            &shape.signature,
            &shape.docs,
        );
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

    if let Some((trait_name, method)) = interface.traits.iter().find_map(|(trait_name, trait_)| {
        trait_
            .methods
            .get(identifier)
            .map(|method| (trait_name, method))
    }) {
        let params = interface_params_text(&method.params);
        let pure = if method.pure { "@pure\n" } else { "" };
        return hover_markdown(
            "trait method",
            &format!("{module_name}.{trait_name}.{identifier}"),
            &format!("{pure}{identifier}({params}): {}", method.return_type),
            &method.docs,
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

/// Collects source-like negative impl facts for one local target type.
fn local_negative_trait_impl_signatures(
    module: &SyntaxModuleOutput,
    type_name: &str,
) -> Vec<String> {
    let mut signatures = module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::TraitImpl {
                trait_ref,
                for_type,
                is_negative: true,
                is_public,
                ..
            } if Backend::base_type_name(&for_type.text) == type_name => Some(format!(
                "{}impl not {}[{}].",
                visibility_prefix(*is_public),
                trait_ref.text,
                for_type.text
            )),
            _ => None,
        })
        .collect::<Vec<_>>();
    signatures.sort();
    signatures.dedup();
    signatures
}

/// Collects exported negative impl facts for one imported target type.
fn interface_negative_trait_impl_signatures(
    interface: &ModuleInterface,
    type_name: &str,
) -> Vec<String> {
    let mut signatures = interface
        .trait_conformances
        .iter()
        .filter(|conformance| {
            conformance.is_negative
                && conformance.public
                && Backend::base_type_name(&conformance.for_type) == type_name
        })
        .map(|conformance| {
            format!(
                "pub impl not {}[{}].",
                conformance.trait_ref, conformance.for_type
            )
        })
        .collect::<Vec<_>>();
    signatures.sort();
    signatures.dedup();
    signatures
}

/// Appends visible negative impl metadata to a type hover.
fn append_negative_trait_impls(mut markdown: String, signatures: Vec<String>) -> String {
    if signatures.is_empty() {
        return markdown;
    }
    markdown.push_str("\n\n**Negative trait implementations**\n\n```terlan\n");
    markdown.push_str(&signatures.join("\n"));
    markdown.push_str("\n```");
    markdown
}

/// Renders the source-facing signature for one trait method hover entry.
fn syntax_trait_method_signature(method: &SyntaxTraitMethodOutput) -> String {
    let pure = if method.is_pure { "@pure\n" } else { "" };
    format!(
        "{pure}{}({}): {}",
        method.name,
        syntax_params_text(&method.params),
        method.return_type.text
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
    let generic_params = type_params_text(&signature.generic_params);
    let purity_prefix = if signature.pure { "@pure\n" } else { "" };
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
            "{}pub {}{}{}({}): {}",
            purity_prefix,
            receiver_text,
            signature.name,
            generic_params,
            rest,
            signature.return_type
        )
    } else {
        format!(
            "{}pub {}{}({}): {}",
            purity_prefix, signature.name, generic_params, params, signature.return_type
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
        .map(|param| {
            let display_name = param.pattern_text.as_deref().unwrap_or(&param.name);
            format!("{}: {}", display_name, param.annotation.text)
        })
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
#[cfg(test)]
mod hover_test;
