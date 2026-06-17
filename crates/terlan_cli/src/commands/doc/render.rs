use terlan_syntax::{
    SyntaxConstructorClauseOutput, SyntaxConstructorParamOutput, SyntaxDeclarationOutput,
    SyntaxDeclarationPayload, SyntaxImplMethodOutput, SyntaxModuleOutput, SyntaxParamOutput,
    SyntaxStructFieldOutput, SyntaxTraitMethodOutput, SyntaxTypeOutput,
};

use crate::commands::json::json_string;

/// Renders syntax-output module documentation as Markdown.
///
/// Inputs:
/// - `module`: formal syntax-output module containing documentation metadata.
///
/// Output:
/// - Markdown documentation for the module.
///
/// Transformation:
/// - Groups declarations by documentation section and renders declaration
///   signatures from syntax-output fields.
pub(crate) fn render_syntax_module_docs_markdown(module: &SyntaxModuleOutput) -> String {
    let mut out = String::new();
    out.push_str(&format!("# `{}`\n\n", module.module_name));
    push_markdown_doc_block(&mut out, &module.docs);

    let types: Vec<_> = module
        .declarations
        .iter()
        .filter_map(|decl| match &decl.payload {
            SyntaxDeclarationPayload::Type {
                name,
                params,
                is_public,
                is_opaque,
                variants,
                ..
            } if *is_public => Some((
                decl.docs.as_slice(),
                name,
                params,
                is_public,
                is_opaque,
                variants,
            )),
            _ => None,
        })
        .collect();
    if !types.is_empty() {
        out.push_str("## Types\n\n");
        for (docs, name, params, is_public, is_opaque, variants) in types {
            render_syntax_type_decl_docs_markdown(
                &mut out, docs, name, params, *is_public, *is_opaque, variants,
            );
        }
    }

    let structs: Vec<_> = module
        .declarations
        .iter()
        .filter_map(|decl| match &decl.payload {
            SyntaxDeclarationPayload::Struct {
                name,
                is_public,
                fields,
                ..
            } if *is_public => Some((decl.docs.as_slice(), name, is_public, fields)),
            _ => None,
        })
        .collect();
    if !structs.is_empty() {
        out.push_str("## Structs\n\n");
        for (docs, name, is_public, fields) in structs {
            render_syntax_struct_decl_docs_markdown(&mut out, docs, name, *is_public, fields);
        }
    }

    let constructors: Vec<_> = module
        .declarations
        .iter()
        .filter_map(|decl| match &decl.payload {
            SyntaxDeclarationPayload::Constructor {
                name,
                params,
                is_public,
                clauses,
            } if *is_public => Some((decl.docs.as_slice(), name, params, is_public, clauses)),
            _ => None,
        })
        .collect();
    if !constructors.is_empty() {
        out.push_str("## Constructors\n\n");
        for (docs, name, params, is_public, clauses) in constructors {
            render_syntax_constructor_decl_docs_markdown(
                &mut out, docs, name, params, *is_public, clauses,
            );
        }
    }

    let trait_decls: Vec<_> = module
        .declarations
        .iter()
        .filter_map(|decl| match &decl.payload {
            SyntaxDeclarationPayload::Trait {
                name,
                params,
                super_traits,
                is_public,
                methods,
            } if *is_public => Some((
                decl.docs.as_slice(),
                name,
                params,
                super_traits,
                is_public,
                methods,
            )),
            _ => None,
        })
        .collect();
    if !trait_decls.is_empty() {
        out.push_str("## Traits\n\n");
        for (docs, name, params, super_traits, is_public, methods) in trait_decls {
            render_syntax_trait_decl_docs_markdown(
                &mut out,
                docs,
                name,
                params,
                super_traits,
                *is_public,
                methods,
            );
        }
    }

    let trait_impls: Vec<_> = module
        .declarations
        .iter()
        .filter_map(|decl| match &decl.payload {
            SyntaxDeclarationPayload::TraitImpl {
                trait_ref,
                for_type,
                is_public,
                methods,
            } if *is_public => Some((
                decl.docs.as_slice(),
                trait_ref,
                for_type,
                is_public,
                methods.as_slice(),
            )),
            _ => None,
        })
        .collect();
    if !trait_impls.is_empty() {
        out.push_str("## Trait Implementations\n\n");
        for (docs, trait_ref, for_type, is_public, methods) in trait_impls {
            render_syntax_trait_impl_docs_markdown(
                &mut out, docs, trait_ref, for_type, *is_public, methods,
            );
        }
    }

    let functions: Vec<_> = module
        .declarations
        .iter()
        .filter_map(|decl| match &decl.payload {
            SyntaxDeclarationPayload::Function {
                name,
                params,
                return_type,
                is_public,
                is_macro,
                ..
            } if *is_public => Some((
                decl.docs.as_slice(),
                name,
                params,
                return_type,
                is_public,
                is_macro,
            )),
            _ => None,
        })
        .collect();
    if !functions.is_empty() {
        out.push_str("## Functions\n\n");
        for (docs, name, params, return_type, is_public, is_macro) in functions {
            render_syntax_function_decl_docs_markdown(
                &mut out,
                docs,
                name,
                params,
                return_type,
                *is_public,
                *is_macro,
            );
        }
    }

    let methods: Vec<_> = module
        .declarations
        .iter()
        .filter_map(|decl| match &decl.payload {
            SyntaxDeclarationPayload::Method {
                receiver,
                name,
                params,
                return_type,
                is_public,
                ..
            } if *is_public => Some((
                decl.docs.as_slice(),
                receiver,
                name,
                params,
                return_type,
                is_public,
            )),
            _ => None,
        })
        .collect();
    if !methods.is_empty() {
        out.push_str("## Receiver Methods\n\n");
        for (docs, receiver, name, params, return_type, is_public) in methods {
            render_syntax_method_decl_docs_markdown(
                &mut out,
                docs,
                receiver,
                name,
                params,
                return_type,
                *is_public,
            );
        }
    }

    out
}

/// Renders syntax-output module documentation as a static HTML reference page.
///
/// Inputs:
/// - `module`: formal syntax-output module containing documentation metadata.
///
/// Output:
/// - HTML documentation string.
///
/// Transformation:
/// - Converts public syntax-output declarations into a navigable module page
///   with source-shaped signatures, declaration sections, and rendered
///   documentation blocks.
pub(crate) fn render_syntax_module_docs_html(module: &SyntaxModuleOutput) -> String {
    let title = format!("{} documentation", module.module_name);
    let module_docs = render_doc_lines_html(&module.docs);
    let table_of_contents = render_syntax_module_html_toc(module);
    let sections = render_syntax_module_html_sections(module);
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>{}</title>\n<style>\n{}\n</style>\n</head>\n<body>\n<header class=\"doc-header\"><a class=\"doc-index-link\" href=\"index.html\">Index</a><p class=\"doc-kicker\">Terlan module</p><h1>{}</h1>{}</header>\n<main class=\"doc-layout\"><aside class=\"doc-nav\"><h2>Contents</h2>{}</aside><section class=\"doc-content\">{}</section></main>\n</body>\n</html>\n",
        sanitize_html_text(&title),
        syntax_doc_html_styles(),
        sanitize_html_text(&module.module_name),
        module_docs,
        table_of_contents,
        sections
    )
}

/// Renders module-level declaration links for a documentation page.
///
/// Inputs:
/// - `module`: syntax-output module whose public declarations should appear in
///   the page navigation.
///
/// Output:
/// - HTML navigation list.
///
/// Transformation:
/// - Reads public declarations in source order and emits stable anchor links
///   grouped only by their visible declaration text.
fn render_syntax_module_html_toc(module: &SyntaxModuleOutput) -> String {
    let items = module
        .declarations
        .iter()
        .filter_map(public_doc_item)
        .map(|item| {
            format!(
                "<li><a href=\"#{}\"><span>{}</span><code>{}</code></a></li>",
                sanitize_html_text(&doc_anchor_id(&item.kind, &item.name)),
                sanitize_html_text(&item.kind),
                sanitize_html_text(&item.name),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    if items.is_empty() {
        "<p class=\"doc-empty\">No public API declarations.</p>".to_string()
    } else {
        format!("<ul class=\"doc-nav-list\">\n{}\n</ul>", items)
    }
}

/// Renders all public declaration sections for a documentation page.
///
/// Inputs:
/// - `module`: syntax-output module with declarations and documentation.
///
/// Output:
/// - HTML section content for the module body.
///
/// Transformation:
/// - Groups public declarations by API category while preserving source order
///   inside each category.
fn render_syntax_module_html_sections(module: &SyntaxModuleOutput) -> String {
    let mut out = String::new();
    push_html_section(&mut out, module, "Types", |decl| match &decl.payload {
        SyntaxDeclarationPayload::Type { .. } => render_declaration_html_card(decl),
        _ => None,
    });
    push_html_section(&mut out, module, "Structs", |decl| match &decl.payload {
        SyntaxDeclarationPayload::Struct { .. } => render_declaration_html_card(decl),
        _ => None,
    });
    push_html_section(&mut out, module, "Constructors", |decl| {
        match &decl.payload {
            SyntaxDeclarationPayload::Constructor { .. } => render_declaration_html_card(decl),
            _ => None,
        }
    });
    push_html_section(&mut out, module, "Traits", |decl| match &decl.payload {
        SyntaxDeclarationPayload::Trait { .. } => render_declaration_html_card(decl),
        _ => None,
    });
    push_html_section(
        &mut out,
        module,
        "Trait Implementations",
        |decl| match &decl.payload {
            SyntaxDeclarationPayload::TraitImpl { .. } => render_declaration_html_card(decl),
            _ => None,
        },
    );
    push_html_section(&mut out, module, "Functions", |decl| match &decl.payload {
        SyntaxDeclarationPayload::Function { .. } => render_declaration_html_card(decl),
        _ => None,
    });
    push_html_section(&mut out, module, "Receiver Methods", |decl| {
        match &decl.payload {
            SyntaxDeclarationPayload::Method { .. } => render_declaration_html_card(decl),
            _ => None,
        }
    });
    if out.is_empty() {
        out.push_str("<section class=\"doc-section\"><p class=\"doc-empty\">No public API declarations.</p></section>");
    }
    out
}

/// Appends one declaration category to module HTML output.
///
/// Inputs:
/// - `out`: HTML output buffer.
/// - `module`: source syntax-output module.
/// - `title`: visible section heading.
/// - `render`: category-specific declaration renderer.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Filters declarations with `render` and appends a titled section only
///   when at least one declaration belongs to that category.
fn push_html_section<F>(out: &mut String, module: &SyntaxModuleOutput, title: &str, render: F)
where
    F: Fn(&SyntaxDeclarationOutput) -> Option<String>,
{
    let cards = module
        .declarations
        .iter()
        .filter_map(render)
        .collect::<Vec<_>>();
    if cards.is_empty() {
        return;
    }
    out.push_str("<section class=\"doc-section\">");
    out.push_str(&format!("<h2>{}</h2>", sanitize_html_text(title)));
    out.push_str(&cards.join("\n"));
    out.push_str("</section>");
}

/// Public documentation metadata for one declaration.
///
/// Inputs:
/// - Built from one syntax-output declaration.
///
/// Output:
/// - Kind, name, signature, docs, and optional detail HTML for rendering.
///
/// Transformation:
/// - Normalizes syntax-output variants into one small documentation record.
struct PublicDocItem<'a> {
    kind: String,
    name: String,
    signature: String,
    docs: &'a [String],
    detail_html: String,
}

/// Extracts a renderable documentation item from a declaration.
///
/// Inputs:
/// - `declaration`: syntax-output declaration.
///
/// Output:
/// - `Some(PublicDocItem)` for public API declarations.
/// - `None` for private declarations and non-API declarations.
///
/// Transformation:
/// - Maps each public declaration variant to a visible kind, stable name,
///   source-shaped signature, attached docs, and category-specific details.
fn public_doc_item(declaration: &SyntaxDeclarationOutput) -> Option<PublicDocItem<'_>> {
    match &declaration.payload {
        SyntaxDeclarationPayload::Type {
            name,
            params,
            is_public,
            is_opaque,
            variants,
            ..
        } if *is_public => Some(PublicDocItem {
            kind: "type".to_string(),
            name: name.clone(),
            signature: render_type_signature(name, params, *is_public, *is_opaque, variants),
            docs: &declaration.docs,
            detail_html: String::new(),
        }),
        SyntaxDeclarationPayload::Struct {
            name,
            is_public,
            fields,
            ..
        } if *is_public => Some(PublicDocItem {
            kind: "struct".to_string(),
            name: name.clone(),
            signature: render_struct_signature(name, *is_public, fields),
            docs: &declaration.docs,
            detail_html: render_struct_fields_html(fields),
        }),
        SyntaxDeclarationPayload::Constructor {
            name,
            params,
            is_public,
            clauses,
        } if *is_public => Some(PublicDocItem {
            kind: "constructor".to_string(),
            name: name.clone(),
            signature: render_constructor_signature(name, params, *is_public),
            docs: &declaration.docs,
            detail_html: render_constructor_clauses_html(name, clauses),
        }),
        SyntaxDeclarationPayload::Function {
            name,
            params,
            return_type,
            is_public,
            is_macro,
            ..
        } if *is_public => Some(PublicDocItem {
            kind: "function".to_string(),
            name: format!("{}/{}", name, params.len()),
            signature: render_function_signature(name, params, return_type, *is_public, *is_macro),
            docs: &declaration.docs,
            detail_html: String::new(),
        }),
        SyntaxDeclarationPayload::Method {
            receiver,
            name,
            params,
            return_type,
            is_public,
            ..
        } if *is_public => Some(PublicDocItem {
            kind: "method".to_string(),
            name: format!("{}.{}({})", receiver.annotation.text, name, params.len()),
            signature: render_method_signature(receiver, name, params, return_type, *is_public),
            docs: &declaration.docs,
            detail_html: String::new(),
        }),
        SyntaxDeclarationPayload::Trait {
            name,
            params,
            super_traits,
            is_public,
            methods,
        } if *is_public => Some(PublicDocItem {
            kind: "trait".to_string(),
            name: name.clone(),
            signature: render_trait_signature(name, params, super_traits, *is_public),
            docs: &declaration.docs,
            detail_html: render_trait_methods_html(methods),
        }),
        SyntaxDeclarationPayload::TraitImpl {
            trait_ref,
            for_type,
            is_public,
            methods,
        } if *is_public => Some(PublicDocItem {
            kind: "impl".to_string(),
            name: format!("{} for {}", trait_ref.text, for_type.text),
            signature: render_trait_impl_signature(trait_ref, for_type, *is_public),
            docs: &declaration.docs,
            detail_html: render_impl_methods_html(methods),
        }),
        _ => None,
    }
}

/// Renders one public declaration as an HTML card.
///
/// Inputs:
/// - `declaration`: syntax-output declaration to render.
///
/// Output:
/// - HTML card for public API declarations.
/// - `None` for non-public or non-renderable declarations.
///
/// Transformation:
/// - Converts normalized declaration metadata into stable, linkable HTML.
fn render_declaration_html_card(declaration: &SyntaxDeclarationOutput) -> Option<String> {
    let item = public_doc_item(declaration)?;
    let anchor = doc_anchor_id(&item.kind, &item.name);
    Some(format!(
        "<article class=\"doc-card\" id=\"{}\"><header><span class=\"doc-kind\">{}</span><h3>{}</h3></header>{}<pre class=\"doc-signature\"><code class=\"language-terlan\">{}</code></pre>{}</article>",
        sanitize_html_text(&anchor),
        sanitize_html_text(&item.kind),
        sanitize_html_text(&item.name),
        render_doc_lines_html(item.docs),
        sanitize_html_text(&item.signature),
        item.detail_html,
    ))
}

/// Renders struct field details as HTML.
///
/// Inputs:
/// - `fields`: syntax-output struct fields.
///
/// Output:
/// - HTML field list, or an empty string when no fields exist.
///
/// Transformation:
/// - Emits field names, field types, and field documentation in a compact list.
fn render_struct_fields_html(fields: &[SyntaxStructFieldOutput]) -> String {
    if fields.is_empty() {
        return String::new();
    }
    let items = fields
        .iter()
        .map(|field| {
            format!(
                "<li><code>{}</code>: <code>{}</code>{}</li>",
                sanitize_html_text(&field.name),
                sanitize_html_text(&field.annotation.text),
                render_inline_docs_suffix(&field.docs),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "<div class=\"doc-detail\"><h4>Fields</h4><ul>{}</ul></div>",
        items
    )
}

/// Renders constructor clause details as HTML.
///
/// Inputs:
/// - `name`: constructor owner type name.
/// - `clauses`: syntax-output constructor clauses.
///
/// Output:
/// - HTML clause list, or an empty string when no clauses exist.
///
/// Transformation:
/// - Converts constructor clauses into source-shaped callable signatures.
fn render_constructor_clauses_html(
    name: &str,
    clauses: &[SyntaxConstructorClauseOutput],
) -> String {
    if clauses.is_empty() {
        return String::new();
    }
    let items = clauses
        .iter()
        .map(|clause| {
            format!(
                "<li><code>{}</code></li>",
                sanitize_html_text(&render_constructor_clause_signature(name, clause)),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "<div class=\"doc-detail\"><h4>Clauses</h4><ul>{}</ul></div>",
        items
    )
}

/// Renders one constructor clause as source-shaped text.
///
/// Inputs:
/// - `name`: constructor owner type name.
/// - `clause`: syntax-output constructor clause.
///
/// Output:
/// - `Name(params): Return` signature text.
///
/// Transformation:
/// - Joins constructor parameters and return annotation without rendering the
///   implementation body.
fn render_constructor_clause_signature(
    name: &str,
    clause: &SyntaxConstructorClauseOutput,
) -> String {
    format!(
        "{}({}): {}",
        name,
        clause
            .params
            .iter()
            .map(render_constructor_param_signature)
            .collect::<Vec<_>>()
            .join(", "),
        clause.return_type.text
    )
}

/// Renders one constructor parameter signature.
///
/// Inputs:
/// - `param`: syntax-output constructor parameter.
///
/// Output:
/// - `name: Type` parameter signature text.
///
/// Transformation:
/// - Combines parameter name and type annotation, preserving varargs marker
///   when present.
fn render_constructor_param_signature(param: &SyntaxConstructorParamOutput) -> String {
    format!(
        "{}{}: {}",
        if param.is_varargs { "..." } else { "" },
        param.name,
        param.annotation.text
    )
}

/// Renders trait method details as HTML.
///
/// Inputs:
/// - `methods`: syntax-output trait methods.
///
/// Output:
/// - HTML method list, or an empty string when no methods exist.
///
/// Transformation:
/// - Exposes trait method signatures and method-level docs.
fn render_trait_methods_html(methods: &[SyntaxTraitMethodOutput]) -> String {
    if methods.is_empty() {
        return String::new();
    }
    let items = methods
        .iter()
        .map(|method| {
            format!(
                "<li><code>{}</code>{}</li>",
                sanitize_html_text(&render_syntax_trait_method_signature(method)),
                render_inline_docs_suffix(&method.docs),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "<div class=\"doc-detail\"><h4>Methods</h4><ul>{}</ul></div>",
        items
    )
}

/// Renders implementation method details as HTML.
///
/// Inputs:
/// - `methods`: syntax-output implementation methods.
///
/// Output:
/// - HTML method list, or an empty string when no methods exist.
///
/// Transformation:
/// - Exposes implementation method signatures without implementation bodies.
fn render_impl_methods_html(methods: &[SyntaxImplMethodOutput]) -> String {
    if methods.is_empty() {
        return String::new();
    }
    let items = methods
        .iter()
        .map(|method| {
            format!(
                "<li><code>{}({}): {}.</code></li>",
                sanitize_html_text(&method.name),
                sanitize_html_text(
                    &method
                        .params
                        .iter()
                        .map(render_syntax_param_signature)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                sanitize_html_text(&method.return_type.text),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "<div class=\"doc-detail\"><h4>Methods</h4><ul>{}</ul></div>",
        items
    )
}

/// Renders a documentation suffix for inline detail rows.
///
/// Inputs:
/// - `docs`: documentation lines attached to a field or method.
///
/// Output:
/// - Escaped inline HTML suffix, or an empty string when no docs exist.
///
/// Transformation:
/// - Joins documentation lines into one compact sentence-like fragment.
fn render_inline_docs_suffix(docs: &[String]) -> String {
    if docs.is_empty() {
        return String::new();
    }
    format!(
        " <span class=\"doc-inline-docs\">{}</span>",
        sanitize_html_text(&docs.join(" "))
    )
}

/// Renders documentation comment lines as basic HTML prose.
///
/// Inputs:
/// - `docs`: normalized documentation lines from syntax output.
///
/// Output:
/// - HTML paragraphs and code blocks.
///
/// Transformation:
/// - Preserves fenced code examples, groups plain text into paragraphs, and
///   escapes all source text before embedding it into HTML.
fn render_doc_lines_html(docs: &[String]) -> String {
    let mut out = String::new();
    let mut paragraph = Vec::new();
    let mut in_code = false;

    for line in docs.iter().flat_map(|doc| doc.lines()) {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            flush_doc_paragraph_html(&mut out, &mut paragraph);
            if in_code {
                out.push_str("</code></pre>");
                in_code = false;
            } else {
                let language = trimmed.trim_start_matches("```").trim();
                out.push_str(&format!(
                    "<pre class=\"doc-example\"><code{}>",
                    if language.is_empty() {
                        String::new()
                    } else {
                        format!(" class=\"language-{}\"", sanitize_html_text(language))
                    }
                ));
                in_code = true;
            }
            continue;
        }
        if in_code {
            out.push_str(&sanitize_html_text(line));
            out.push('\n');
            continue;
        }
        if trimmed.is_empty() {
            flush_doc_paragraph_html(&mut out, &mut paragraph);
        } else {
            paragraph.push(trimmed.to_string());
        }
    }
    if in_code {
        out.push_str("</code></pre>");
    }
    flush_doc_paragraph_html(&mut out, &mut paragraph);
    out
}

/// Flushes accumulated prose lines into one HTML paragraph.
///
/// Inputs:
/// - `out`: HTML output buffer.
/// - `paragraph`: mutable paragraph line accumulator.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Joins accumulated lines with spaces, escapes them, writes a paragraph,
///   and clears the accumulator.
fn flush_doc_paragraph_html(out: &mut String, paragraph: &mut Vec<String>) {
    if paragraph.is_empty() {
        return;
    }
    out.push_str("<p>");
    out.push_str(&sanitize_html_text(&paragraph.join(" ")));
    out.push_str("</p>");
    paragraph.clear();
}

/// Builds a stable HTML anchor id for a declaration.
///
/// Inputs:
/// - `kind`: declaration kind label.
/// - `name`: declaration display name.
///
/// Output:
/// - Lowercase ASCII-ish anchor id.
///
/// Transformation:
/// - Replaces non-alphanumeric runs with `-` and trims separators so links
///   remain deterministic across documentation builds.
fn doc_anchor_id(kind: &str, name: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in format!("{kind}-{name}").chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

/// Returns shared CSS for generated module documentation pages.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Static CSS stylesheet text.
///
/// Transformation:
/// - Encodes the minimal public reference layout directly in the compiler so
///   generated docs have no runtime dependency.
fn syntax_doc_html_styles() -> &'static str {
    "body{margin:0;background:#f8fafc;color:#172033;font:16px/1.55 system-ui,-apple-system,BlinkMacSystemFont,\"Segoe UI\",sans-serif}.doc-header{background:#14213d;color:white;padding:2rem max(1.25rem,calc((100vw - 1120px)/2))}.doc-header h1{margin:.2rem 0 0;font-size:2rem}.doc-header p{max-width:760px}.doc-kicker{margin:0;text-transform:uppercase;font-size:.75rem;letter-spacing:.08em;color:#a9c3ff}.doc-index-link{color:#dbeafe;text-decoration:none}.doc-layout{display:grid;grid-template-columns:260px minmax(0,1fr);gap:2rem;max-width:1120px;margin:0 auto;padding:2rem 1.25rem}.doc-nav{position:sticky;top:1rem;align-self:start}.doc-nav h2,.doc-section h2{font-size:1rem;text-transform:uppercase;color:#475569;letter-spacing:.06em}.doc-nav-list{list-style:none;margin:0;padding:0}.doc-nav-list li{margin:.35rem 0}.doc-nav-list a{display:flex;justify-content:space-between;gap:1rem;color:#172033;text-decoration:none;border-left:3px solid transparent;padding:.25rem .5rem}.doc-nav-list a:hover{border-color:#2563eb;background:#eaf1ff}.doc-nav-list span{color:#64748b}.doc-content{min-width:0}.doc-section{margin-bottom:2rem}.doc-card{background:white;border:1px solid #d7dee9;border-radius:8px;margin:1rem 0;padding:1.1rem 1.25rem;box-shadow:0 1px 2px rgba(15,23,42,.04)}.doc-card header{display:flex;align-items:center;gap:.75rem;flex-wrap:wrap}.doc-card h3{margin:.2rem 0;font-size:1.2rem}.doc-kind{border:1px solid #bfdbfe;background:#eff6ff;color:#1d4ed8;border-radius:999px;font-size:.75rem;padding:.1rem .5rem}.doc-signature,.doc-example{background:#0f172a;color:#e5edf7;border-radius:6px;overflow:auto;padding:.85rem}.doc-signature code,.doc-example code{font-family:\"SFMono-Regular\",Consolas,\"Liberation Mono\",monospace;font-size:.9rem}.doc-detail{border-top:1px solid #e2e8f0;margin-top:1rem;padding-top:.75rem}.doc-detail h4{margin:.2rem 0 .5rem}.doc-detail ul{margin:.25rem 0;padding-left:1.25rem}.doc-inline-docs{color:#475569}.doc-empty{color:#64748b}@media(max-width:760px){.doc-layout{display:block}.doc-nav{position:static;margin-bottom:1.5rem}}"
}

/// Renders syntax-output module documentation as a JSON model.
///
/// Inputs:
/// - `module`: formal syntax-output module containing documentation metadata.
///
/// Output:
/// - Deterministic JSON model for documentation tooling.
///
/// Transformation:
/// - Converts module docs and source declarations into a compact
///   compiler-owned documentation model without depending on a target runtime
///   documentation generator.
pub(crate) fn render_syntax_module_docs_json(module: &SyntaxModuleOutput) -> String {
    let declarations = module
        .declarations
        .iter()
        .filter_map(render_syntax_declaration_doc_json)
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"schema\":\"terlan-doc-module-v1\",\"module\":{},\"docs\":{},\"declarations\":[{}]}}\n",
        json_string(&module.module_name),
        render_json_string_array(&module.docs),
        declarations,
    )
}

/// Renders one declaration into the JSON documentation model.
///
/// Inputs:
/// - `declaration`: syntax-output declaration to render.
///
/// Output:
/// - JSON object for renderable declarations.
/// - `None` for imports and exports, which are not public API docs.
///
/// Transformation:
/// - Classifies declaration kind, source-visible name, visibility, signature,
///   and attached docs into stable JSON fields.
fn render_syntax_declaration_doc_json(declaration: &SyntaxDeclarationOutput) -> Option<String> {
    let (kind, name, is_public, signature) = match &declaration.payload {
        SyntaxDeclarationPayload::Type {
            name,
            params,
            is_public,
            is_opaque,
            variants,
            ..
        } if *is_public => (
            "type",
            name.as_str(),
            *is_public,
            render_type_signature(name, params, *is_public, *is_opaque, variants),
        ),
        SyntaxDeclarationPayload::Struct {
            name,
            is_public,
            fields,
            ..
        } if *is_public => (
            "struct",
            name.as_str(),
            *is_public,
            render_struct_signature(name, *is_public, fields),
        ),
        SyntaxDeclarationPayload::Constructor {
            name,
            params,
            is_public,
            ..
        } if *is_public => (
            "constructor",
            name.as_str(),
            *is_public,
            render_constructor_signature(name, params, *is_public),
        ),
        SyntaxDeclarationPayload::Function {
            name,
            params,
            return_type,
            is_public,
            is_macro,
            ..
        } if *is_public => (
            "function",
            name.as_str(),
            *is_public,
            render_function_signature(name, params, return_type, *is_public, *is_macro),
        ),
        SyntaxDeclarationPayload::Method {
            receiver,
            name,
            params,
            return_type,
            is_public,
            ..
        } if *is_public => (
            "method",
            name.as_str(),
            *is_public,
            render_method_signature(receiver, name, params, return_type, *is_public),
        ),
        SyntaxDeclarationPayload::Trait {
            name,
            params,
            super_traits,
            is_public,
            ..
        } if *is_public => (
            "trait",
            name.as_str(),
            *is_public,
            render_trait_signature(name, params, super_traits, *is_public),
        ),
        SyntaxDeclarationPayload::TraitImpl {
            trait_ref,
            for_type,
            is_public,
            ..
        } if *is_public => (
            "impl",
            trait_ref.text.as_str(),
            *is_public,
            render_trait_impl_signature(trait_ref, for_type, *is_public),
        ),
        SyntaxDeclarationPayload::Import { .. }
        | SyntaxDeclarationPayload::Export { .. }
        | SyntaxDeclarationPayload::Type { .. }
        | SyntaxDeclarationPayload::Struct { .. }
        | SyntaxDeclarationPayload::Constructor { .. }
        | SyntaxDeclarationPayload::Function { .. }
        | SyntaxDeclarationPayload::Method { .. }
        | SyntaxDeclarationPayload::Trait { .. }
        | SyntaxDeclarationPayload::TraitImpl { .. }
        | SyntaxDeclarationPayload::AnnotationSchema { .. }
        | SyntaxDeclarationPayload::Template { .. }
        | SyntaxDeclarationPayload::Config { .. }
        | SyntaxDeclarationPayload::Raw { .. } => return None,
    };

    Some(format!(
        "{{\"kind\":{},\"name\":{},\"public\":{},\"signature\":{},\"docs\":{}}}",
        json_string(kind),
        json_string(name),
        is_public,
        json_string(&signature),
        render_json_string_array(&declaration.docs),
    ))
}

/// Renders a type declaration signature for documentation JSON.
///
/// Inputs:
/// - `name`: type name.
/// - `params`: type parameter names.
/// - `is_public`: whether the type is public.
/// - `is_opaque`: whether the type uses opaque visibility.
/// - `variants`: rendered type expression variants.
///
/// Output:
/// - Source-shaped type declaration signature.
///
/// Transformation:
/// - Combines visibility, opacity, parameters, and variants into one line.
fn render_type_signature(
    name: &str,
    params: &[String],
    is_public: bool,
    is_opaque: bool,
    variants: &[SyntaxTypeOutput],
) -> String {
    let mut out = String::new();
    out.push_str(if is_public { "pub " } else { "" });
    out.push_str(if is_opaque { "opaque " } else { "type " });
    out.push_str(name);
    if !params.is_empty() {
        out.push('[');
        out.push_str(&params.join(", "));
        out.push(']');
    }
    if !variants.is_empty() {
        out.push_str(" = ");
        out.push_str(
            &variants
                .iter()
                .map(|variant| variant.text.as_str())
                .collect::<Vec<_>>()
                .join(" | "),
        );
    }
    out.push('.');
    out
}

/// Renders a struct declaration signature for documentation JSON.
///
/// Inputs:
/// - `name`: struct name.
/// - `is_public`: whether the struct is public.
/// - `fields`: struct fields.
///
/// Output:
/// - Compact source-shaped struct signature.
///
/// Transformation:
/// - Joins field declarations into a single-line signature for machine
///   consumers that do not need Markdown formatting.
fn render_struct_signature(
    name: &str,
    is_public: bool,
    fields: &[SyntaxStructFieldOutput],
) -> String {
    let fields = fields
        .iter()
        .map(|field| format!("{}: {}", field.name, field.annotation.text))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{}struct {} {{{}}}.",
        if is_public { "pub " } else { "" },
        name,
        fields
    )
}

/// Renders a constructor declaration signature for documentation JSON.
///
/// Inputs:
/// - `name`: constructor owner type name.
/// - `params`: type parameter names.
/// - `is_public`: whether the constructor declaration is public.
///
/// Output:
/// - Source-shaped constructor declaration header.
///
/// Transformation:
/// - Renders the declaration header because constructor clauses are represented
///   separately in syntax output.
fn render_constructor_signature(name: &str, params: &[String], is_public: bool) -> String {
    let mut out = String::new();
    out.push_str(if is_public { "pub " } else { "" });
    out.push_str("constructor ");
    out.push_str(name);
    if !params.is_empty() {
        out.push('[');
        out.push_str(&params.join(", "));
        out.push(']');
    }
    out.push('.');
    out
}

/// Renders a function declaration signature for documentation JSON.
///
/// Inputs:
/// - `name`: function name.
/// - `params`: function parameters.
/// - `return_type`: return type.
/// - `is_public`: whether the function is public.
/// - `is_macro`: whether the function is a macro.
///
/// Output:
/// - Source-shaped function signature.
///
/// Transformation:
/// - Joins parameters and return annotation into a declaration signature.
fn render_function_signature(
    name: &str,
    params: &[SyntaxParamOutput],
    return_type: &SyntaxTypeOutput,
    is_public: bool,
    is_macro: bool,
) -> String {
    format!(
        "{}{}{}({}): {}.",
        if is_public { "pub " } else { "" },
        if is_macro { "macro " } else { "" },
        name,
        params
            .iter()
            .map(render_syntax_param_signature)
            .collect::<Vec<_>>()
            .join(", "),
        return_type.text
    )
}

/// Renders a receiver method signature for documentation JSON.
///
/// Inputs:
/// - `receiver`: method receiver parameter.
/// - `name`: method name.
/// - `params`: method call parameters.
/// - `return_type`: return type.
/// - `is_public`: whether the method is public.
///
/// Output:
/// - Source-shaped receiver method signature.
///
/// Transformation:
/// - Places the receiver before the method name, matching Terlan source syntax.
fn render_method_signature(
    receiver: &SyntaxParamOutput,
    name: &str,
    params: &[SyntaxParamOutput],
    return_type: &SyntaxTypeOutput,
    is_public: bool,
) -> String {
    format!(
        "{}({}) {}({}): {}.",
        if is_public { "pub " } else { "" },
        render_syntax_param_signature(receiver),
        name,
        params
            .iter()
            .map(render_syntax_param_signature)
            .collect::<Vec<_>>()
            .join(", "),
        return_type.text
    )
}

/// Renders a trait declaration signature for documentation JSON.
///
/// Inputs:
/// - `name`: trait name.
/// - `params`: trait type parameters.
/// - `super_traits`: inherited traits.
/// - `is_public`: whether the trait is public.
///
/// Output:
/// - Source-shaped trait declaration header.
///
/// Transformation:
/// - Renders only the trait header for compact JSON documentation.
fn render_trait_signature(
    name: &str,
    params: &[String],
    super_traits: &[String],
    is_public: bool,
) -> String {
    let mut out = String::new();
    out.push_str(if is_public { "pub " } else { "" });
    out.push_str("trait ");
    out.push_str(name);
    if !params.is_empty() {
        out.push('[');
        out.push_str(&params.join(", "));
        out.push(']');
    }
    if !super_traits.is_empty() {
        out.push_str(" extends ");
        out.push_str(&super_traits.join(", "));
    }
    out.push('.');
    out
}

/// Renders a trait implementation signature for documentation JSON.
///
/// Inputs:
/// - `trait_ref`: implemented trait.
/// - `for_type`: implementation target type.
/// - `is_public`: whether the implementation is public.
///
/// Output:
/// - Source-shaped implementation header.
///
/// Transformation:
/// - Renders the trait/type pair without implementation method bodies.
fn render_trait_impl_signature(
    trait_ref: &SyntaxTypeOutput,
    for_type: &SyntaxTypeOutput,
    is_public: bool,
) -> String {
    format!(
        "{}impl {} for {}.",
        if is_public { "pub " } else { "" },
        trait_ref.text,
        for_type.text
    )
}

/// Renders a JSON string array.
///
/// Inputs:
/// - `values`: ordered string values.
///
/// Output:
/// - JSON array text.
///
/// Transformation:
/// - Escapes each value with the shared CLI JSON string helper.
fn render_json_string_array(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| json_string(value))
            .collect::<Vec<_>>()
            .join(",")
    )
}

/// Appends documentation lines to a Markdown output buffer.
///
/// Inputs:
/// - `out`: Markdown output buffer.
/// - `docs`: documentation lines from syntax output.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Appends lines with newlines and adds one blank line after non-empty docs.
fn push_markdown_doc_block(out: &mut String, docs: &[String]) {
    for line in docs {
        out.push_str(line);
        out.push('\n');
    }
    if !docs.is_empty() {
        out.push('\n');
    }
}

/// Appends a type declaration documentation section.
///
/// Inputs:
/// - `out`: Markdown output buffer.
/// - `docs`: documentation lines for the type.
/// - `name`: type name.
/// - `params`: type parameter names.
/// - `is_public`: whether the type is public.
/// - `is_opaque`: whether the type is opaque.
/// - `variants`: type expression variants.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Renders docs and a Terlan type signature fence.
fn render_syntax_type_decl_docs_markdown(
    out: &mut String,
    docs: &[String],
    name: &str,
    params: &[String],
    is_public: bool,
    is_opaque: bool,
    variants: &[SyntaxTypeOutput],
) {
    out.push_str(&format!("### `{}`\n\n", name));
    push_markdown_doc_block(out, docs);
    out.push_str("```terlan\n");
    out.push_str(if is_public { "pub " } else { "" });
    out.push_str(if is_opaque { "opaque " } else { "type " });
    out.push_str(name);
    if !params.is_empty() {
        out.push('[');
        out.push_str(&params.join(", "));
        out.push(']');
    }
    if !variants.is_empty() {
        out.push_str(" = ");
        out.push_str(
            &variants
                .iter()
                .map(|variant| variant.text.as_str())
                .collect::<Vec<_>>()
                .join(" | "),
        );
    }
    out.push_str(".\n```\n\n");
}

/// Appends a struct declaration documentation section.
///
/// Inputs:
/// - `out`: Markdown output buffer.
/// - `docs`: documentation lines for the struct.
/// - `name`: struct name.
/// - `is_public`: whether the struct is public.
/// - `fields`: struct field syntax-output data.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Renders docs, a Terlan struct signature fence, and field docs when
///   present.
fn render_syntax_struct_decl_docs_markdown(
    out: &mut String,
    docs: &[String],
    name: &str,
    is_public: bool,
    fields: &[SyntaxStructFieldOutput],
) {
    out.push_str(&format!("### `{}`\n\n", name));
    push_markdown_doc_block(out, docs);
    out.push_str("```terlan\n");
    out.push_str(if is_public { "pub " } else { "" });
    out.push_str(&format!("struct {} {{\n", name));
    for field in fields {
        out.push_str(&format!("    {}: {}", field.name, field.annotation.text));
        if field.has_default {
            out.push_str(" = ...");
        }
        out.push_str(",\n");
    }
    out.push_str("}.\n```\n\n");

    if fields.iter().any(|field| !field.docs.is_empty()) {
        out.push_str("#### Fields\n\n");
        for field in fields {
            out.push_str(&format!("- `{}`: `{}`", field.name, field.annotation.text));
            if !field.docs.is_empty() {
                out.push_str(" - ");
                out.push_str(&field.docs.join(" "));
            }
            out.push('\n');
        }
        out.push('\n');
    }
}

/// Appends a constructor declaration documentation section.
///
/// Inputs:
/// - `out`: Markdown output buffer.
/// - `docs`: documentation lines for the constructor declaration.
/// - `name`: constructor owner type name.
/// - `params`: constructor type parameter names.
/// - `is_public`: whether the constructor is public.
/// - `clauses`: constructor clause signatures.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Renders docs, the constructor header, and public constructor clauses as a
///   Terlan signature fence.
fn render_syntax_constructor_decl_docs_markdown(
    out: &mut String,
    docs: &[String],
    name: &str,
    params: &[String],
    is_public: bool,
    clauses: &[SyntaxConstructorClauseOutput],
) {
    out.push_str(&format!("### `{}`\n\n", name));
    push_markdown_doc_block(out, docs);
    out.push_str("```terlan\n");
    out.push_str(&render_constructor_signature(name, params, is_public));
    if !clauses.is_empty() {
        out.push('\n');
        for clause in clauses {
            out.push_str(&render_constructor_clause_signature(name, clause));
            out.push_str(".\n");
        }
    } else {
        out.push('\n');
    }
    out.push_str("```\n\n");
}

/// Appends a trait declaration documentation section.
///
/// Inputs:
/// - `out`: Markdown output buffer.
/// - `docs`: documentation lines for the trait.
/// - `name`: trait name.
/// - `params`: trait type parameters.
/// - `super_traits`: inherited trait names.
/// - `is_public`: whether the trait is public.
/// - `methods`: trait method declarations.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Renders docs and a Terlan trait signature fence.
fn render_syntax_trait_decl_docs_markdown(
    out: &mut String,
    docs: &[String],
    name: &str,
    params: &[String],
    super_traits: &[String],
    is_public: bool,
    methods: &[SyntaxTraitMethodOutput],
) {
    out.push_str(&format!("### `{}`\n\n", name));
    push_markdown_doc_block(out, docs);
    out.push_str("```terlan\n");
    out.push_str(if is_public { "pub " } else { "" });
    out.push_str("trait ");
    out.push_str(name);
    if !params.is_empty() {
        out.push('[');
        out.push_str(&params.join(", "));
        out.push(']');
    }
    if !super_traits.is_empty() {
        out.push_str(" extends ");
        out.push_str(&super_traits.join(", "));
    }
    out.push_str(" {\n");
    out.push_str(
        &methods
            .iter()
            .map(render_syntax_trait_method_signature)
            .collect::<Vec<_>>()
            .join("\n"),
    );
    out.push_str("\n}.\n```\n\n");
}

/// Renders one trait method signature for documentation.
///
/// Inputs:
/// - `method`: syntax-output trait method.
///
/// Output:
/// - Indented Terlan method signature text.
///
/// Transformation:
/// - Joins rendered parameter signatures and appends return annotation text.
fn render_syntax_trait_method_signature(method: &SyntaxTraitMethodOutput) -> String {
    let mut out = String::new();
    out.push_str("    ");
    out.push_str(&method.name);
    out.push('(');
    out.push_str(
        &method
            .params
            .iter()
            .map(render_syntax_param_signature)
            .collect::<Vec<_>>()
            .join(", "),
    );
    out.push_str("): ");
    out.push_str(&method.return_type.text);
    out.push('.');
    out
}

/// Appends an explicit trait implementation documentation section.
///
/// Inputs:
/// - `out`: Markdown output buffer.
/// - `docs`: documentation lines for the implementation declaration.
/// - `trait_ref`: trait reference being implemented.
/// - `for_type`: concrete or generic target type.
/// - `is_public`: whether the conformance is public.
/// - `methods`: implementation method declarations.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Renders a source-shaped `impl Trait for Type` block containing method
///   signatures only, because docs should expose API shape rather than method
///   bodies.
fn render_syntax_trait_impl_docs_markdown(
    out: &mut String,
    docs: &[String],
    trait_ref: &SyntaxTypeOutput,
    for_type: &SyntaxTypeOutput,
    is_public: bool,
    methods: &[SyntaxImplMethodOutput],
) {
    out.push_str(&format!(
        "### `{} for {}`\n\n",
        trait_ref.text, for_type.text
    ));
    push_markdown_doc_block(out, docs);
    out.push_str("```terlan\n");
    out.push_str(if is_public { "pub " } else { "" });
    out.push_str("impl ");
    out.push_str(&trait_ref.text);
    out.push_str(" for ");
    out.push_str(&for_type.text);
    out.push_str(" {\n");
    for method in methods {
        out.push_str("    ");
        out.push_str(&method.name);
        out.push('(');
        out.push_str(
            &method
                .params
                .iter()
                .map(render_syntax_param_signature)
                .collect::<Vec<_>>()
                .join(", "),
        );
        out.push_str("): ");
        out.push_str(&method.return_type.text);
        out.push_str(".\n");
    }
    out.push_str("}.\n```\n\n");
}

/// Appends a function declaration documentation section.
///
/// Inputs:
/// - `out`: Markdown output buffer.
/// - `docs`: documentation lines for the function.
/// - `name`: function name.
/// - `params`: syntax-output parameters.
/// - `return_type`: syntax-output return type.
/// - `is_public`: whether the function is public.
/// - `is_macro`: whether the function is a macro.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Renders docs and a Terlan function signature fence.
fn render_syntax_function_decl_docs_markdown(
    out: &mut String,
    docs: &[String],
    name: &str,
    params: &[SyntaxParamOutput],
    return_type: &SyntaxTypeOutput,
    is_public: bool,
    is_macro: bool,
) {
    out.push_str(&format!("### `{}/{}`\n\n", name, params.len()));
    push_markdown_doc_block(out, docs);
    out.push_str("```terlan\n");
    out.push_str(if is_public { "pub " } else { "" });
    if is_macro {
        out.push_str("macro ");
    }
    out.push_str(name);
    out.push('(');
    out.push_str(
        &params
            .iter()
            .map(render_syntax_param_signature)
            .collect::<Vec<_>>()
            .join(", "),
    );
    out.push_str(&format!("): {}.\n```\n\n", return_type.text));
}

/// Appends a receiver method documentation section.
///
/// Inputs:
/// - `out`: Markdown output buffer.
/// - `docs`: documentation lines for the method.
/// - `receiver`: method receiver parameter.
/// - `name`: method name.
/// - `params`: method call parameters.
/// - `return_type`: return type.
/// - `is_public`: whether the method is public.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Renders docs and a Terlan receiver method signature fence.
fn render_syntax_method_decl_docs_markdown(
    out: &mut String,
    docs: &[String],
    receiver: &SyntaxParamOutput,
    name: &str,
    params: &[SyntaxParamOutput],
    return_type: &SyntaxTypeOutput,
    is_public: bool,
) {
    out.push_str(&format!(
        "### `{}.{}({})`\n\n",
        receiver.annotation.text,
        name,
        params.len()
    ));
    push_markdown_doc_block(out, docs);
    out.push_str("```terlan\n");
    out.push_str(&render_method_signature(
        receiver,
        name,
        params,
        return_type,
        is_public,
    ));
    out.push_str("\n```\n\n");
}

/// Renders one typed parameter signature for documentation.
///
/// Inputs:
/// - `param`: syntax-output parameter.
///
/// Output:
/// - `name: Type` parameter signature text.
///
/// Transformation:
/// - Combines parameter name and annotation text.
fn render_syntax_param_signature(param: &SyntaxParamOutput) -> String {
    format!("{}: {}", param.name, param.annotation.text)
}

/// Escapes text before embedding it into generated documentation HTML.
///
/// Inputs:
/// - `input`: raw text.
///
/// Output:
/// - HTML-safe text.
///
/// Transformation:
/// - Escapes ampersands and angle brackets.
fn sanitize_html_text(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        render_syntax_module_docs_html, render_syntax_module_docs_json,
        render_syntax_module_docs_markdown,
    };

    /// Verifies the JSON documentation renderer emits a parseable module model.
    ///
    /// Inputs:
    /// - One parsed Terlan module with module and function docs.
    ///
    /// Output:
    /// - JSON object containing schema, module name, docs, and declaration
    ///   signature fields.
    ///
    /// Transformation:
    /// - Renders syntax output into the compiler-owned JSON documentation model
    ///   and parses it back through `serde_json`.
    #[test]
    fn renders_syntax_module_docs_json_model() {
        let source = r#"/**
 * Math docs.
 *
 * @module mathx
 */
module mathx.

/**
 * Adds one.
 */
pub add(x: Int): Int ->
    x + 1.
"#;
        let module = terlan_syntax::parse_module_as_syntax_output(source).expect("parse module");

        let json = render_syntax_module_docs_json(&module);
        let value: serde_json::Value = serde_json::from_str(&json).expect("parse docs json");

        assert_eq!(value["schema"], "terlan-doc-module-v1");
        assert_eq!(value["module"], "mathx");
        assert_eq!(value["docs"][0], "Math docs.\n\n@module mathx");
        assert_eq!(value["declarations"][0]["kind"], "function");
        assert_eq!(value["declarations"][0]["name"], "add");
        assert_eq!(value["declarations"][0]["public"], true);
        assert_eq!(
            value["declarations"][0]["signature"],
            "pub add(x: Int): Int."
        );
    }

    /// Verifies documentation rendering excludes private declarations.
    ///
    /// Inputs:
    /// - One parsed Terlan module with public and private functions.
    ///
    /// Output:
    /// - Markdown and JSON outputs containing only the public function.
    ///
    /// Transformation:
    /// - Renders through both public docs formats and checks the 0.0.3
    ///   public-API-only documentation rule.
    #[test]
    fn renders_only_public_declarations() {
        let source = r#"module mathx.

/**
 * Adds one.
 */
pub add(x: Int): Int ->
    x + 1.

/**
 * Internal helper.
 */
hidden(x: Int): Int ->
    x.

/**
 * Receiver helper.
 */
pub (value: Int) to_string(): String ->
    "1".
"#;
        let module = terlan_syntax::parse_module_as_syntax_output(source).expect("parse module");

        let markdown = render_syntax_module_docs_markdown(&module);
        assert!(markdown.contains("add/1"));
        assert!(markdown.contains("Receiver Methods"));
        assert!(markdown.contains("Int.to_string(0)"));
        assert!(!markdown.contains("hidden"));

        let json = render_syntax_module_docs_json(&module);
        let value: serde_json::Value = serde_json::from_str(&json).expect("parse docs json");
        let names = value["declarations"]
            .as_array()
            .expect("decls")
            .iter()
            .map(|decl| decl["name"].as_str().expect("declaration name"))
            .collect::<Vec<_>>();
        assert!(names.contains(&"add"));
        assert!(names.contains(&"to_string"));
        assert!(!names.contains(&"hidden"));
    }

    /// Verifies HTML documentation renders a usable public module reference.
    ///
    /// Inputs:
    /// - One parsed module containing module docs, a struct, and a receiver
    ///   method.
    ///
    /// Output:
    /// - HTML containing the module shell, declaration navigation, field
    ///   details, method section, and Terlan signature code.
    ///
    /// Transformation:
    /// - Renders formal syntax output to static HTML without going through a
    ///   Markdown validation artifact.
    #[test]
    fn renders_syntax_module_docs_html_reference_page() {
        let source = r#"/**
 * User module docs.
 */
module std.core.User.

/**
 * User record.
 */
pub struct User {
    name: String
}.

/**
 * Returns the display name.
 *
 * ```terlan
 * user.display_name().
 * ```
 */
pub (user: User) display_name(): String ->
    user.name.
"#;
        let module = terlan_syntax::parse_module_as_syntax_output(source).expect("parse module");

        let html = render_syntax_module_docs_html(&module);

        assert!(html.contains("<h1>std.core.User</h1>"));
        assert!(html.contains("User module docs."));
        assert!(html.contains("Structs"));
        assert!(html.contains("Receiver Methods"));
        assert!(html.contains("pub struct User"));
        assert!(html.contains("pub (user: User) display_name(): String."));
        assert!(html.contains("user.display_name()."));
    }
}
