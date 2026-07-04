use std::collections::{BTreeMap, BTreeSet};

use crate::terlan_syntax::parse_tree::{
    Annotation, CaseClause, ConstructorClause, Decl, Expr, HtmlAttrValue, HtmlBlockExpr, HtmlNode,
    ImportDecl, ImportItem, ImportKind, MapExprField, MapField, Module, Param, Pattern, TypeExpr,
    UnaryOp,
};
use crate::terlan_syntax::parser::{parse_interface_module, parse_module, ParseError};
use crate::terlan_syntax::syntax_output::binary_op_text;

mod declarations;
mod html;
mod metadata;

use declarations::*;
use html::format_html_block;
use metadata::*;

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
/// - Renders imports first in canonical alphabetical order, then walks the
///   remaining declarations in source order. Normal `.terl` parsing rejects
///   `Decl::Export`; if export declarations appear here they are interface
///   summaries from `.terli` parsing.
pub(crate) fn format_module(module: &Module) -> String {
    let promoted_module = promote_repeated_direct_type_imports(module);
    format_module_inner(&promoted_module)
}

/// Formats a parsed module after formatter-owned normalization passes.
fn format_module_inner(module: &Module) -> String {
    let mut out = String::new();
    if !module.docs.is_empty() {
        out.push_str(&format_docs(&module.docs, 0));
        out.push('\n');
    }
    out.push_str("module ");
    out.push_str(&module.name);
    out.push_str(".\n\n");

    for (i, (decl, annotations)) in ordered_declarations_for_format(module).iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let docs = format_decl_docs(decl);
        if !docs.is_empty() {
            out.push_str(&docs);
            out.push('\n');
        }
        let annotations = format_decl_annotations(annotations);
        if !annotations.is_empty() {
            out.push_str(&annotations);
            out.push('\n');
        }
        out.push_str(&format_decl(decl));
        out.push('\n');
    }

    out
}

/// Promotes repeated fully-qualified type references into selected type imports.
///
/// Inputs:
/// - `module`: parsed source or interface module.
///
/// Output:
/// - A cloned module with repeated references such as
///   `std.js.Number.JsNumber` shortened to `JsNumber`, plus a synthesized
///   `import type std.js.Number.{JsNumber}.` when needed.
///
/// Transformation:
/// - Counts formatter-visible type expressions, keeps only repeated
///   fully-qualified type names, avoids local/import name conflicts, rewrites
///   matching type text, and appends synthetic imports for the normal import
///   ordering pass to place.
fn promote_repeated_direct_type_imports(module: &Module) -> Module {
    let mut counts = BTreeMap::new();
    collect_type_refs_from_module(module, &mut counts);
    let blocked_names = blocked_type_promotion_names(module);
    let existing_type_imports = existing_type_imports(module);
    let mut replacements = BTreeMap::new();
    let mut imports = BTreeSet::new();

    for (qualified, count) in counts {
        if count < 2 {
            continue;
        }
        let Some((module_name, type_name)) = split_qualified_type_ref(&qualified) else {
            continue;
        };
        if blocked_names.contains(&type_name) {
            continue;
        }
        match existing_type_imports.get(&type_name) {
            Some(existing_module) if existing_module == &module_name => {
                replacements.insert(qualified, type_name);
            }
            Some(_) => {}
            None => {
                replacements.insert(qualified, type_name.clone());
                imports.insert((module_name, type_name));
            }
        }
    }

    if replacements.is_empty() {
        return module.clone();
    }

    let mut promoted = module.clone();
    rewrite_type_refs_in_module(&mut promoted, &replacements);
    for (module_name, type_name) in imports {
        promoted.declarations.push(Decl::Import(ImportDecl {
            kind: ImportKind::Module,
            module_name,
            items: vec![ImportItem {
                name: type_name,
                as_alias: None,
                span: promoted.span,
            }],
            is_type: true,
            is_selected: true,
            source_path: None,
            span: promoted.span,
        }));
        promoted.declaration_annotations.push(Vec::new());
    }
    promoted
}

fn collect_type_refs_from_module(module: &Module, counts: &mut BTreeMap<String, usize>) {
    for decl in &module.declarations {
        collect_type_refs_from_decl(decl, counts);
    }
}

fn collect_type_refs_from_decl(decl: &Decl, counts: &mut BTreeMap<String, usize>) {
    match decl {
        Decl::Type(decl) => {
            for ty in decl.variants.iter().chain(decl.implements.iter()) {
                collect_type_refs_from_type_text(&ty.text, counts);
            }
        }
        Decl::Struct(decl) => {
            for include in &decl.includes {
                collect_type_refs_from_type_text(include, counts);
            }
            for implement in &decl.implements {
                collect_type_refs_from_type_text(&implement.text, counts);
            }
            for field in &decl.fields {
                collect_type_refs_from_type_text(&field.annotation.text, counts);
                if let Some(default) = &field.default {
                    collect_type_refs_from_expr(default, counts);
                }
            }
        }
        Decl::Constructor(decl) => {
            for clause in &decl.clauses {
                collect_type_refs_from_constructor_clause(clause, counts);
            }
        }
        Decl::Function(decl) => {
            for bound in &decl.generic_bounds {
                collect_type_refs_from_type_text(bound, counts);
            }
            for param in &decl.params {
                collect_type_refs_from_param(param, counts);
            }
            collect_type_refs_from_type_text(&decl.return_type.text, counts);
            for clause in &decl.clauses {
                collect_type_refs_from_function_clause(clause, counts);
            }
        }
        Decl::Method(decl) => {
            for bound in &decl.generic_bounds {
                collect_type_refs_from_type_text(bound, counts);
            }
            collect_type_refs_from_param(&decl.receiver, counts);
            for param in &decl.params {
                collect_type_refs_from_param(param, counts);
            }
            collect_type_refs_from_type_text(&decl.return_type.text, counts);
            for clause in &decl.clauses {
                collect_type_refs_from_function_clause(clause, counts);
            }
        }
        Decl::Trait(decl) => {
            for super_trait in &decl.super_traits {
                collect_type_refs_from_type_text(super_trait, counts);
            }
            for method in &decl.methods {
                for bound in &method.generic_bounds {
                    collect_type_refs_from_type_text(bound, counts);
                }
                for param in &method.params {
                    collect_type_refs_from_param(param, counts);
                }
                collect_type_refs_from_type_text(&method.return_type.text, counts);
                if let Some(default_body) = &method.default_body {
                    collect_type_refs_from_expr(default_body, counts);
                }
            }
        }
        Decl::TraitImpl(decl) => {
            collect_type_refs_from_type_text(&decl.trait_ref.text, counts);
            collect_type_refs_from_type_text(&decl.for_type.text, counts);
            for method in &decl.methods {
                collect_type_refs_from_decl(&Decl::Function(method.clone()), counts);
            }
        }
        Decl::Template(decl) => {
            for prop in &decl.props {
                collect_type_refs_from_type_text(&prop.annotation.text, counts);
                if let Some(default) = &prop.default {
                    collect_type_refs_from_expr(default, counts);
                }
            }
        }
        Decl::Import(_) | Decl::Export(_) | Decl::AnnotationSchema(_) | Decl::Raw(_) => {}
    }
}

fn collect_type_refs_from_constructor_clause(
    clause: &ConstructorClause,
    counts: &mut BTreeMap<String, usize>,
) {
    for param in &clause.params {
        collect_type_refs_from_type_text(&param.annotation.text, counts);
        if let Some(default) = &param.default {
            collect_type_refs_from_expr(default, counts);
        }
    }
    collect_type_refs_from_type_text(&clause.return_type.text, counts);
    collect_type_refs_from_expr(&clause.body, counts);
}

fn collect_type_refs_from_function_clause(
    clause: &crate::terlan_syntax::parse_tree::FunctionClause,
    counts: &mut BTreeMap<String, usize>,
) {
    if let Some(guard) = &clause.guard {
        collect_type_refs_from_expr(guard, counts);
    }
    collect_type_refs_from_expr(&clause.body, counts);
}

fn collect_type_refs_from_param(param: &Param, counts: &mut BTreeMap<String, usize>) {
    collect_type_refs_from_type_text(&param.annotation.text, counts);
    if let Some(default) = &param.default {
        collect_type_refs_from_expr(default, counts);
    }
}

fn collect_type_refs_from_expr(expr: &Expr, counts: &mut BTreeMap<String, usize>) {
    match expr {
        Expr::Tuple(items)
        | Expr::List(items)
        | Expr::FixedArray(items)
        | Expr::Sequence(items) => {
            for item in items {
                collect_type_refs_from_expr(item, counts);
            }
        }
        Expr::ListCons(left, right)
        | Expr::Index(left, right)
        | Expr::BinaryOp { left, right, .. } => {
            collect_type_refs_from_expr(left, counts);
            collect_type_refs_from_expr(right, counts);
        }
        Expr::IndexAssign {
            collection,
            index,
            value,
        } => {
            collect_type_refs_from_expr(collection, counts);
            collect_type_refs_from_expr(index, counts);
            collect_type_refs_from_expr(value, counts);
        }
        Expr::Map(fields)
        | Expr::RecordUpdate { fields, .. }
        | Expr::RecordConstruct { fields, .. } => {
            for field in fields {
                collect_type_refs_from_expr(&field.value, counts);
            }
            if let Expr::RecordUpdate { value, .. } = expr {
                collect_type_refs_from_expr(value, counts);
            }
        }
        Expr::ListComprehension {
            expr,
            source,
            guard,
            ..
        } => {
            collect_type_refs_from_expr(expr, counts);
            collect_type_refs_from_expr(source, counts);
            if let Some(guard) = guard {
                collect_type_refs_from_expr(guard, counts);
            }
        }
        Expr::Let { bindings, body } => {
            for binding in bindings {
                collect_type_refs_from_expr(&binding.value, counts);
            }
            if let Some(body) = body {
                collect_type_refs_from_expr(body, counts);
            }
        }
        Expr::Call {
            callee,
            type_args,
            args,
            ..
        } => {
            collect_type_refs_from_expr(callee, counts);
            for ty in type_args {
                collect_type_refs_from_type_text(&ty.text, counts);
            }
            for arg in args {
                collect_type_refs_from_expr(arg, counts);
            }
        }
        Expr::Case { scrutinee, clauses } => {
            collect_type_refs_from_expr(scrutinee, counts);
            for clause in clauses {
                collect_type_refs_from_case_clause(clause, counts);
            }
        }
        Expr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            collect_type_refs_from_expr(body, counts);
            for clause in of_clauses.iter().chain(catch_clauses.iter()) {
                collect_type_refs_from_case_clause(clause, counts);
            }
            if let Some(after) = after_clause {
                collect_type_refs_from_expr(&after.trigger, counts);
                collect_type_refs_from_expr(&after.body, counts);
            }
        }
        Expr::If { clauses } => {
            for clause in clauses {
                collect_type_refs_from_expr(&clause.condition, counts);
                collect_type_refs_from_expr(&clause.body, counts);
            }
        }
        Expr::Fun { clauses } => {
            for clause in clauses {
                collect_type_refs_from_function_clause(clause, counts);
            }
        }
        Expr::MacroCall { args, .. } => {
            for arg in args {
                collect_type_refs_from_expr(arg, counts);
            }
        }
        Expr::RawMacro {
            type_args,
            interpolations,
            ..
        } => {
            for ty in type_args {
                collect_type_refs_from_type_text(&ty.text, counts);
            }
            for interpolation in interpolations {
                collect_type_refs_from_expr(interpolation, counts);
            }
        }
        Expr::HtmlBlock(block) => collect_type_refs_from_html_block(block, counts),
        Expr::RecordAccess { value, .. } | Expr::FieldAccess { value, .. } => {
            collect_type_refs_from_expr(value, counts);
        }
        Expr::ConstructorChain { base, record } => {
            collect_type_refs_from_expr(base, counts);
            collect_type_refs_from_expr(record, counts);
        }
        Expr::UnaryOp { expr, .. } | Expr::Quote(expr) | Expr::Unquote(expr) => {
            collect_type_refs_from_expr(expr, counts);
        }
        Expr::Cast { expr, target_type } => {
            collect_type_refs_from_expr(expr, counts);
            collect_type_refs_from_type_text(&target_type.text, counts);
        }
        Expr::Int(_)
        | Expr::Float(_)
        | Expr::Atom(_)
        | Expr::AtomLiteral(_)
        | Expr::Binary(_)
        | Expr::Var(_) => {}
    }
}

fn collect_type_refs_from_case_clause(clause: &CaseClause, counts: &mut BTreeMap<String, usize>) {
    if let Some(guard) = &clause.guard {
        collect_type_refs_from_expr(guard, counts);
    }
    collect_type_refs_from_expr(&clause.body, counts);
}

fn collect_type_refs_from_html_block(block: &HtmlBlockExpr, counts: &mut BTreeMap<String, usize>) {
    for node in &block.nodes {
        collect_type_refs_from_html_node(node, counts);
    }
}

fn collect_type_refs_from_html_node(node: &HtmlNode, counts: &mut BTreeMap<String, usize>) {
    match node {
        HtmlNode::Element(element) => {
            for attr in &element.attrs {
                if let Some(HtmlAttrValue::Expr(expr)) = &attr.value {
                    collect_type_refs_from_expr(expr, counts);
                }
            }
            for child in &element.children {
                collect_type_refs_from_html_node(child, counts);
            }
        }
        HtmlNode::Expr(expr) => collect_type_refs_from_expr(expr, counts),
        HtmlNode::NamedSlot(slot) => {
            for child in &slot.children {
                collect_type_refs_from_html_node(child, counts);
            }
        }
        HtmlNode::Text(_) => {}
    }
}

fn collect_type_refs_from_type_text(text: &str, counts: &mut BTreeMap<String, usize>) {
    for token in type_ref_tokens(text) {
        if is_qualified_type_ref_candidate(token) {
            *counts.entry(token.to_string()).or_insert(0) += 1;
        }
    }
}

fn blocked_type_promotion_names(module: &Module) -> BTreeSet<String> {
    let mut blocked = BTreeSet::new();
    for decl in &module.declarations {
        match decl {
            Decl::Type(decl) => {
                blocked.insert(decl.name.clone());
            }
            Decl::Struct(decl) => {
                blocked.insert(decl.name.clone());
            }
            Decl::Trait(decl) => {
                blocked.insert(decl.name.clone());
            }
            Decl::Import(import) if !import.is_type => {
                for item in &import.items {
                    blocked.insert(import_local_name(item));
                }
            }
            _ => {}
        }
    }
    blocked
}

fn existing_type_imports(module: &Module) -> BTreeMap<String, String> {
    let mut imports = BTreeMap::new();
    for decl in &module.declarations {
        let Decl::Import(import) = decl else {
            continue;
        };
        if !import.is_type {
            continue;
        }
        for item in &import.items {
            imports.insert(import_local_name(item), import.module_name.clone());
        }
    }
    imports
}

fn import_local_name(item: &ImportItem) -> String {
    item.as_alias.clone().unwrap_or_else(|| item.name.clone())
}

fn split_qualified_type_ref(text: &str) -> Option<(String, String)> {
    if !is_qualified_type_ref_candidate(text) {
        return None;
    }
    let (module_name, type_name) = text.rsplit_once('.')?;
    Some((module_name.to_string(), type_name.to_string()))
}

fn is_qualified_type_ref_candidate(text: &str) -> bool {
    let Some((module_name, type_name)) = text.rsplit_once('.') else {
        return false;
    };
    if module_name.is_empty() || type_name.is_empty() {
        return false;
    }
    if !text
        .split('.')
        .all(|segment| !segment.is_empty() && is_identifier_segment(segment))
    {
        return false;
    }
    type_name
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
        && module_name
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_lowercase())
}

fn is_identifier_segment(segment: &str) -> bool {
    let mut chars = segment.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn type_ref_tokens(text: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut start = None;
    for (index, ch) in text.char_indices() {
        if is_type_ref_token_char(ch) {
            start.get_or_insert(index);
        } else if let Some(token_start) = start.take() {
            tokens.push(&text[token_start..index]);
        }
    }
    if let Some(token_start) = start {
        tokens.push(&text[token_start..]);
    }
    tokens
}

fn is_type_ref_token_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '.'
}

fn rewrite_type_refs_in_module(module: &mut Module, replacements: &BTreeMap<String, String>) {
    for decl in &mut module.declarations {
        rewrite_type_refs_in_decl(decl, replacements);
    }
}

fn rewrite_type_refs_in_decl(decl: &mut Decl, replacements: &BTreeMap<String, String>) {
    match decl {
        Decl::Type(decl) => {
            for ty in decl.variants.iter_mut().chain(decl.implements.iter_mut()) {
                rewrite_type_text(&mut ty.text, replacements);
            }
        }
        Decl::Struct(decl) => {
            for include in &mut decl.includes {
                rewrite_type_text(include, replacements);
            }
            for implement in &mut decl.implements {
                rewrite_type_text(&mut implement.text, replacements);
            }
            for field in &mut decl.fields {
                rewrite_type_text(&mut field.annotation.text, replacements);
                if let Some(default) = &mut field.default {
                    rewrite_type_refs_in_expr(default, replacements);
                }
            }
        }
        Decl::Constructor(decl) => {
            for clause in &mut decl.clauses {
                rewrite_type_refs_in_constructor_clause(clause, replacements);
            }
        }
        Decl::Function(decl) => {
            for bound in &mut decl.generic_bounds {
                rewrite_type_text(bound, replacements);
            }
            for param in &mut decl.params {
                rewrite_type_refs_in_param(param, replacements);
            }
            rewrite_type_text(&mut decl.return_type.text, replacements);
            for clause in &mut decl.clauses {
                rewrite_type_refs_in_function_clause(clause, replacements);
            }
        }
        Decl::Method(decl) => {
            for bound in &mut decl.generic_bounds {
                rewrite_type_text(bound, replacements);
            }
            rewrite_type_refs_in_param(&mut decl.receiver, replacements);
            for param in &mut decl.params {
                rewrite_type_refs_in_param(param, replacements);
            }
            rewrite_type_text(&mut decl.return_type.text, replacements);
            for clause in &mut decl.clauses {
                rewrite_type_refs_in_function_clause(clause, replacements);
            }
        }
        Decl::Trait(decl) => {
            for super_trait in &mut decl.super_traits {
                rewrite_type_text(super_trait, replacements);
            }
            for method in &mut decl.methods {
                for bound in &mut method.generic_bounds {
                    rewrite_type_text(bound, replacements);
                }
                for param in &mut method.params {
                    rewrite_type_refs_in_param(param, replacements);
                }
                rewrite_type_text(&mut method.return_type.text, replacements);
                if let Some(default_body) = &mut method.default_body {
                    rewrite_type_refs_in_expr(default_body, replacements);
                }
            }
        }
        Decl::TraitImpl(decl) => {
            rewrite_type_text(&mut decl.trait_ref.text, replacements);
            rewrite_type_text(&mut decl.for_type.text, replacements);
            for method in &mut decl.methods {
                rewrite_type_refs_in_decl(&mut Decl::Function(method.clone()), replacements);
                for bound in &mut method.generic_bounds {
                    rewrite_type_text(bound, replacements);
                }
                for param in &mut method.params {
                    rewrite_type_refs_in_param(param, replacements);
                }
                rewrite_type_text(&mut method.return_type.text, replacements);
                for clause in &mut method.clauses {
                    rewrite_type_refs_in_function_clause(clause, replacements);
                }
            }
        }
        Decl::Template(decl) => {
            for prop in &mut decl.props {
                rewrite_type_text(&mut prop.annotation.text, replacements);
                if let Some(default) = &mut prop.default {
                    rewrite_type_refs_in_expr(default, replacements);
                }
            }
        }
        Decl::Import(_) | Decl::Export(_) | Decl::AnnotationSchema(_) | Decl::Raw(_) => {}
    }
}

fn rewrite_type_refs_in_constructor_clause(
    clause: &mut ConstructorClause,
    replacements: &BTreeMap<String, String>,
) {
    for param in &mut clause.params {
        rewrite_type_text(&mut param.annotation.text, replacements);
        if let Some(default) = &mut param.default {
            rewrite_type_refs_in_expr(default, replacements);
        }
    }
    rewrite_type_text(&mut clause.return_type.text, replacements);
    rewrite_type_refs_in_expr(&mut clause.body, replacements);
}

fn rewrite_type_refs_in_function_clause(
    clause: &mut crate::terlan_syntax::parse_tree::FunctionClause,
    replacements: &BTreeMap<String, String>,
) {
    if let Some(guard) = &mut clause.guard {
        rewrite_type_refs_in_expr(guard, replacements);
    }
    rewrite_type_refs_in_expr(&mut clause.body, replacements);
}

fn rewrite_type_refs_in_param(param: &mut Param, replacements: &BTreeMap<String, String>) {
    rewrite_type_text(&mut param.annotation.text, replacements);
    if let Some(default) = &mut param.default {
        rewrite_type_refs_in_expr(default, replacements);
    }
}

fn rewrite_type_refs_in_expr(expr: &mut Expr, replacements: &BTreeMap<String, String>) {
    match expr {
        Expr::Tuple(items)
        | Expr::List(items)
        | Expr::FixedArray(items)
        | Expr::Sequence(items) => {
            for item in items {
                rewrite_type_refs_in_expr(item, replacements);
            }
        }
        Expr::ListCons(left, right)
        | Expr::Index(left, right)
        | Expr::BinaryOp { left, right, .. } => {
            rewrite_type_refs_in_expr(left, replacements);
            rewrite_type_refs_in_expr(right, replacements);
        }
        Expr::IndexAssign {
            collection,
            index,
            value,
        } => {
            rewrite_type_refs_in_expr(collection, replacements);
            rewrite_type_refs_in_expr(index, replacements);
            rewrite_type_refs_in_expr(value, replacements);
        }
        Expr::Map(fields) | Expr::RecordConstruct { fields, .. } => {
            for field in fields {
                rewrite_type_refs_in_expr(&mut field.value, replacements);
            }
        }
        Expr::RecordUpdate { value, fields, .. } => {
            rewrite_type_refs_in_expr(value, replacements);
            for field in fields {
                rewrite_type_refs_in_expr(&mut field.value, replacements);
            }
        }
        Expr::ListComprehension {
            expr,
            source,
            guard,
            ..
        } => {
            rewrite_type_refs_in_expr(expr, replacements);
            rewrite_type_refs_in_expr(source, replacements);
            if let Some(guard) = guard {
                rewrite_type_refs_in_expr(guard, replacements);
            }
        }
        Expr::Let { bindings, body } => {
            for binding in bindings {
                rewrite_type_refs_in_expr(&mut binding.value, replacements);
            }
            if let Some(body) = body {
                rewrite_type_refs_in_expr(body, replacements);
            }
        }
        Expr::Call {
            callee,
            type_args,
            args,
            ..
        } => {
            rewrite_type_refs_in_expr(callee, replacements);
            for ty in type_args {
                rewrite_type_text(&mut ty.text, replacements);
            }
            for arg in args {
                rewrite_type_refs_in_expr(arg, replacements);
            }
        }
        Expr::Case { scrutinee, clauses } => {
            rewrite_type_refs_in_expr(scrutinee, replacements);
            for clause in clauses {
                rewrite_type_refs_in_case_clause(clause, replacements);
            }
        }
        Expr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            rewrite_type_refs_in_expr(body, replacements);
            for clause in of_clauses.iter_mut().chain(catch_clauses.iter_mut()) {
                rewrite_type_refs_in_case_clause(clause, replacements);
            }
            if let Some(after) = after_clause {
                rewrite_type_refs_in_expr(&mut after.trigger, replacements);
                rewrite_type_refs_in_expr(&mut after.body, replacements);
            }
        }
        Expr::If { clauses } => {
            for clause in clauses {
                rewrite_type_refs_in_expr(&mut clause.condition, replacements);
                rewrite_type_refs_in_expr(&mut clause.body, replacements);
            }
        }
        Expr::Fun { clauses } => {
            for clause in clauses {
                rewrite_type_refs_in_function_clause(clause, replacements);
            }
        }
        Expr::MacroCall { args, .. } => {
            for arg in args {
                rewrite_type_refs_in_expr(arg, replacements);
            }
        }
        Expr::RawMacro {
            type_args,
            interpolations,
            ..
        } => {
            for ty in type_args {
                rewrite_type_text(&mut ty.text, replacements);
            }
            for interpolation in interpolations {
                rewrite_type_refs_in_expr(interpolation, replacements);
            }
        }
        Expr::HtmlBlock(block) => rewrite_type_refs_in_html_block(block, replacements),
        Expr::RecordAccess { value, .. } | Expr::FieldAccess { value, .. } => {
            rewrite_type_refs_in_expr(value, replacements);
        }
        Expr::ConstructorChain { base, record } => {
            rewrite_type_refs_in_expr(base, replacements);
            rewrite_type_refs_in_expr(record, replacements);
        }
        Expr::UnaryOp { expr, .. } | Expr::Quote(expr) | Expr::Unquote(expr) => {
            rewrite_type_refs_in_expr(expr, replacements);
        }
        Expr::Cast { expr, target_type } => {
            rewrite_type_refs_in_expr(expr, replacements);
            rewrite_type_text(&mut target_type.text, replacements);
        }
        Expr::Int(_)
        | Expr::Float(_)
        | Expr::Atom(_)
        | Expr::AtomLiteral(_)
        | Expr::Binary(_)
        | Expr::Var(_) => {}
    }
}

fn rewrite_type_refs_in_case_clause(
    clause: &mut CaseClause,
    replacements: &BTreeMap<String, String>,
) {
    if let Some(guard) = &mut clause.guard {
        rewrite_type_refs_in_expr(guard, replacements);
    }
    rewrite_type_refs_in_expr(&mut clause.body, replacements);
}

fn rewrite_type_refs_in_html_block(
    block: &mut HtmlBlockExpr,
    replacements: &BTreeMap<String, String>,
) {
    for node in &mut block.nodes {
        rewrite_type_refs_in_html_node(node, replacements);
    }
}

fn rewrite_type_refs_in_html_node(node: &mut HtmlNode, replacements: &BTreeMap<String, String>) {
    match node {
        HtmlNode::Element(element) => {
            for attr in &mut element.attrs {
                if let Some(HtmlAttrValue::Expr(expr)) = &mut attr.value {
                    rewrite_type_refs_in_expr(expr, replacements);
                }
            }
            for child in &mut element.children {
                rewrite_type_refs_in_html_node(child, replacements);
            }
        }
        HtmlNode::Expr(expr) => rewrite_type_refs_in_expr(expr, replacements),
        HtmlNode::NamedSlot(slot) => {
            for child in &mut slot.children {
                rewrite_type_refs_in_html_node(child, replacements);
            }
        }
        HtmlNode::Text(_) => {}
    }
}

fn rewrite_type_text(text: &mut String, replacements: &BTreeMap<String, String>) {
    let mut rewritten = String::with_capacity(text.len());
    let mut start = None;
    for (index, ch) in text.char_indices() {
        if is_type_ref_token_char(ch) {
            start.get_or_insert(index);
            continue;
        }
        if let Some(token_start) = start.take() {
            let token = &text[token_start..index];
            rewritten.push_str(replacements.get(token).map(String::as_str).unwrap_or(token));
        }
        rewritten.push(ch);
    }
    if let Some(token_start) = start {
        let token = &text[token_start..];
        rewritten.push_str(replacements.get(token).map(String::as_str).unwrap_or(token));
    }
    *text = rewritten;
}

/// Returns declarations in formatter output order.
///
/// Inputs:
/// - `module`: parsed Terlan module or interface.
///
/// Output:
/// - Declaration references ordered for canonical rendering.
///
/// Transformation:
/// - Extracts import declarations, sorts regular imports before type imports,
///   alphabetizes each group by formatted source text, and places them before
///   non-import declarations. Non-import declarations preserve source order to
///   avoid reordering code with semantic bodies.
fn ordered_declarations_for_format(module: &Module) -> Vec<(&Decl, &[Annotation])> {
    let declarations = module
        .declarations
        .iter()
        .enumerate()
        .map(|(index, decl)| {
            let annotations = module
                .declaration_annotations
                .get(index)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            (decl, annotations)
        })
        .collect::<Vec<_>>();
    let mut imports = declarations
        .iter()
        .copied()
        .filter(|(decl, _)| matches!(decl, Decl::Import(_)))
        .collect::<Vec<_>>();
    imports.sort_by(|left, right| import_sort_key(left.0).cmp(&import_sort_key(right.0)));

    let mut ordered = imports;
    ordered.extend(
        declarations
            .into_iter()
            .filter(|(decl, _)| !matches!(decl, Decl::Import(_))),
    );
    ordered
}

/// Returns the canonical key used to order import declarations.
///
/// Inputs:
/// - `decl`: declaration known to be an import.
///
/// Output:
/// - Pair of import group and formatted import text.
///
/// Transformation:
/// - Groups non-type imports before type imports, then reuses the import
///   formatter so sorting follows the same canonical spelling that will be
///   written to disk.
fn import_sort_key(decl: &Decl) -> (u8, String) {
    match decl {
        Decl::Import(import) => (u8::from(import.is_type), format_import(import)),
        _ => (0, String::new()),
    }
}

/// Formats parsed TypeDoc-style documentation as canonical block comments.
///
/// Inputs:
/// - `docs`: normalized documentation text captured by the lexer.
/// - `indent`: indentation depth measured in formatter levels of four spaces.
///
/// Output:
/// - A canonical `/** ... */` documentation block, or an empty string when no
///   documentation exists.
///
/// Transformation:
/// - Joins adjacent parsed doc tokens into one block and emits every body line
///   as ` * text`, ensuring the marker has a separating space before content.
pub(super) fn format_docs(docs: &[String], indent: usize) -> String {
    if docs.is_empty() {
        return String::new();
    }

    let padding = "    ".repeat(indent);
    let mut out = String::new();
    out.push_str(&padding);
    out.push_str("/**");
    for line in docs.iter().flat_map(|doc| doc.lines()) {
        out.push('\n');
        out.push_str(&padding);
        if line.is_empty() {
            out.push_str(" *");
        } else {
            out.push_str(" * ");
            out.push_str(line);
        }
    }
    out.push('\n');
    out.push_str(&padding);
    out.push_str(" */");
    out
}

/// Formats documentation attached to a declaration.
///
/// Inputs:
/// - `decl`: parsed declaration with optional documentation metadata.
///
/// Output:
/// - Canonical documentation block for the declaration, or an empty string.
///
/// Transformation:
/// - Selects the documentation-bearing field for declarations that support
///   docs and ignores imports/exports, which currently carry no source docs.
fn format_decl_docs(decl: &Decl) -> String {
    let docs = match decl {
        Decl::Type(decl) => &decl.docs,
        Decl::Struct(decl) => &decl.docs,
        Decl::Constructor(decl) => &decl.docs,
        Decl::Function(decl) => &decl.docs,
        Decl::Method(decl) => &decl.docs,
        Decl::Trait(decl) => &decl.docs,
        Decl::TraitImpl(decl) => &decl.docs,
        Decl::AnnotationSchema(decl) => &decl.docs,
        Decl::Template(decl) => &decl.docs,
        Decl::Raw(decl) => &decl.docs,
        Decl::Import(_) | Decl::Export(_) => return String::new(),
    };
    format_docs(docs, 0)
}

/// Formats declaration-leading annotations.
///
/// Inputs:
/// - `annotations`: parsed annotations attached to one declaration.
///
/// Output:
/// - One annotation per line, or an empty string when the declaration has none.
///
/// Transformation:
/// - Preserves marker annotations such as `@test` and raw metadata blocks such
///   as `@target.vm {process_mailbox: true}` before rendering the declaration.
fn format_decl_annotations(annotations: &[Annotation]) -> String {
    annotations
        .iter()
        .map(|annotation| {
            let mut out = format!("@{}", annotation.path.join("."));
            if let Some(args) = &annotation.args {
                out.push(' ');
                out.push_str(args);
            }
            out
        })
        .collect::<Vec<_>>()
        .join("\n")
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

/// Formats a type declaration.
///
/// Inputs: parsed type declaration. Output: canonical type source text.
/// Transformation: emits visibility, opacity, params, implements clauses, and
/// union variants with stable indentation.
pub(super) fn format_pattern(pattern: &Pattern) -> String {
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
                "{}".to_string()
            } else {
                let body = fields
                    .iter()
                    .map(format_map_field)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{{}}}", body)
            }
        }
        Pattern::Record { name, fields } => {
            let body = fields
                .iter()
                .map(format_record_pattern_field)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{} {{{}}}", name, body)
        }
    }
}

/// Formats a record pattern field.
///
/// Inputs: parsed pattern field. Output: `key: pattern` text. Transformation:
/// recursively formats the field pattern value.
fn format_record_pattern_field(field: &MapField) -> String {
    format!("{}: {}", field.key, format_pattern(&field.value))
}

/// Formats a map pattern field.
///
/// Inputs: parsed map pattern field. Output: `key: pattern` text.
/// Transformation: recursively formats the field pattern value.
fn format_map_field(field: &MapField) -> String {
    format!("{}: {}", field.key, format_pattern(&field.value))
}

/// Formats a map expression field.
///
/// Inputs: parsed map expression field. Output: `key: expr` text.
/// Transformation: recursively formats the value expression.
fn format_map_expr_field(field: &MapExprField) -> String {
    format!("{}: {}", field.key, format_expr(&field.value, 0))
}

/// Formats a template or record construction field.
///
/// Inputs: parsed expression field. Output: `key: expr` text. Transformation:
/// recursively formats the value expression.
fn format_template_expr_field(field: &MapExprField) -> String {
    format!("{}: {}", field.key, format_expr(&field.value, 0))
}

/// Formats a type expression.
///
/// Inputs: parsed type expression. Output: source type text. Transformation:
/// trims whitespace and substitutes `Dynamic` for empty type text.
pub(super) fn format_type_expr(ty: &TypeExpr) -> String {
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
pub(super) fn format_expr(expr: &Expr, indent: usize) -> String {
    let spacing = "    ".repeat(indent);
    match expr {
        Expr::Int(value) => value.to_string(),
        Expr::Float(value) => value.to_string(),
        Expr::Atom(value) => value.clone(),
        Expr::AtomLiteral(value) => format!("Atom[{}]", super::quoted_string_literal(value)),
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
                "{}".to_string()
            } else {
                let body = fields
                    .iter()
                    .map(format_map_expr_field)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{{}}}", body)
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
                .map(format_template_expr_field)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}#{}{{{}}}", format_expr(value, 0), name, body)
        }
        Expr::RecordConstruct { name, fields } => {
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
                .map(|binding| {
                    format!(
                        "{} = {}",
                        format_pattern(&binding.pattern),
                        format_expr(&binding.value, 0)
                    )
                })
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
            type_args,
            args,
            arg_names,
            remote,
            is_fun_value,
        } => {
            let args_text = args
                .iter()
                .enumerate()
                .map(
                    |(index, arg)| match arg_names.get(index).and_then(Option::as_ref) {
                        Some(name) => format!("{name} = {}", format_expr(arg, 0)),
                        None => format_expr(arg, 0),
                    },
                )
                .collect::<Vec<_>>()
                .join(", ");
            let rendered_type_args = if type_args.is_empty() {
                String::new()
            } else {
                format!(
                    "[{}]",
                    type_args
                        .iter()
                        .map(|type_arg| type_arg.text.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            if let Some(remote) = remote {
                format!(
                    "{}.{}{}({})",
                    remote,
                    format_expr(callee, 0),
                    rendered_type_args,
                    args_text
                )
            } else if *is_fun_value {
                format!("{}.({})", format_expr(callee, 0), args_text)
            } else {
                format!(
                    "{}{}({})",
                    format_expr(callee, 0),
                    rendered_type_args,
                    args_text
                )
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
        Expr::RawMacro {
            name,
            type_args,
            interpolations: _,
            raw,
        } => {
            let rendered_type_args = if type_args.is_empty() {
                String::new()
            } else {
                format!(
                    "[{}]",
                    type_args
                        .iter()
                        .map(|ty| ty.text.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            format!("{}{} {{{}}}", name, rendered_type_args, raw)
        }
        Expr::BinaryOp { op, left, right } => {
            format!(
                "{} {} {}",
                format_expr(left, 0),
                binary_op_text(op),
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

#[cfg(test)]
#[path = "formatter_test.rs"]
mod formatter_test;
