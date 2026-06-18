use terlan_syntax::{
    SyntaxConstructorClauseOutput, SyntaxConstructorParamOutput, SyntaxDeclarationOutput,
    SyntaxDeclarationPayload, SyntaxImplMethodOutput, SyntaxModuleOutput, SyntaxStructFieldOutput,
    SyntaxTraitMethodOutput,
};

use super::{
    render_constructor_signature, render_function_signature, render_method_signature,
    render_struct_signature, render_syntax_param_signature, render_syntax_trait_method_signature,
    render_trait_impl_signature, render_trait_signature, render_type_signature, sanitize_html_text,
};

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
pub(super) fn render_constructor_clause_signature(
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
