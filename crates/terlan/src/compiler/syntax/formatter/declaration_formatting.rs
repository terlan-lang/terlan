use super::expression_formatting::*;
use super::reference_rewriting::*;
use super::*;

pub(super) fn rewrite_type_refs_in_expr(expr: &mut Expr, replacements: &BTreeMap<String, String>) {
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
        Expr::BinaryLayout { fields, .. } => {
            for field in fields {
                rewrite_type_text(&mut field.descriptor.text, replacements);
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
            generators,
            guards,
            ..
        } => {
            rewrite_type_refs_in_expr(expr, replacements);
            for generator in generators {
                rewrite_type_refs_in_expr(&mut generator.source, replacements);
            }
            for guard in guards {
                rewrite_type_refs_in_expr(guard, replacements);
            }
        }
        Expr::Let {
            bindings,
            else_clauses,
            body,
        } => {
            for binding in bindings {
                rewrite_type_refs_in_expr(&mut binding.value, replacements);
            }
            for clause in else_clauses {
                rewrite_type_refs_in_case_clause(clause, replacements);
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

pub(super) fn rewrite_type_refs_in_case_clause(
    clause: &mut CaseClause,
    replacements: &BTreeMap<String, String>,
) {
    if let Some(guard) = &mut clause.guard {
        rewrite_type_refs_in_expr(guard, replacements);
    }
    rewrite_type_refs_in_expr(&mut clause.body, replacements);
}

pub(super) fn rewrite_type_refs_in_html_block(
    block: &mut HtmlBlockExpr,
    replacements: &BTreeMap<String, String>,
) {
    for node in &mut block.nodes {
        rewrite_type_refs_in_html_node(node, replacements);
    }
}

pub(super) fn rewrite_type_refs_in_html_node(
    node: &mut HtmlNode,
    replacements: &BTreeMap<String, String>,
) {
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

pub(super) fn rewrite_value_call_refs_in_html_block(
    block: &mut HtmlBlockExpr,
    replacements: &BTreeMap<(String, String), String>,
) {
    for node in &mut block.nodes {
        rewrite_value_call_refs_in_html_node(node, replacements);
    }
}

pub(super) fn rewrite_value_call_refs_in_html_node(
    node: &mut HtmlNode,
    replacements: &BTreeMap<(String, String), String>,
) {
    match node {
        HtmlNode::Element(element) => {
            for attr in &mut element.attrs {
                if let Some(HtmlAttrValue::Expr(expr)) = &mut attr.value {
                    rewrite_value_call_refs_in_expr(expr, replacements);
                }
            }
            for child in &mut element.children {
                rewrite_value_call_refs_in_html_node(child, replacements);
            }
        }
        HtmlNode::Expr(expr) => rewrite_value_call_refs_in_expr(expr, replacements),
        HtmlNode::NamedSlot(slot) => {
            for child in &mut slot.children {
                rewrite_value_call_refs_in_html_node(child, replacements);
            }
        }
        HtmlNode::Text(_) => {}
    }
}

pub(super) fn rewrite_type_text(text: &mut String, replacements: &BTreeMap<String, String>) {
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
pub(super) fn ordered_declarations_for_format(module: &Module) -> Vec<(&Decl, &[Annotation])> {
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
    imports.sort_by_key(|left| import_sort_key(left.0));

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
pub(super) fn import_sort_key(decl: &Decl) -> (u8, String) {
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
pub(super) fn format_decl_docs(decl: &Decl) -> String {
    let docs = match decl {
        Decl::Constant(decl) => &decl.docs,
        Decl::ConstFunction(decl) => &decl.docs,
        Decl::Type(decl) => &decl.docs,
        Decl::Struct(decl) => &decl.docs,
        Decl::Constructor(decl) => &decl.docs,
        Decl::Function(decl) => &decl.docs,
        Decl::Method(decl) => &decl.docs,
        Decl::Trait(decl) => &decl.docs,
        Decl::TraitImpl(decl) => &decl.docs,
        Decl::AnnotationSchema(decl) => &decl.docs,
        Decl::Template(decl) => &decl.docs,
        Decl::Shape(decl) => &decl.docs,
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
pub(super) fn format_decl_annotations(annotations: &[Annotation]) -> String {
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
pub(super) fn format_decl(decl: &Decl) -> String {
    match decl {
        Decl::Import(import) => format_import(import),
        Decl::Export(export) => format_export(export),
        Decl::Constant(constant) => format_constant_decl(constant),
        Decl::ConstFunction(function) => format_const_function_decl(function),
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
        Decl::Shape(shape) => format_shape_decl(shape),
        Decl::Raw(raw) => format_raw_decl(raw),
    }
}

/// Formats a type declaration.
///
/// Inputs: parsed type declaration. Output: canonical type source text.
/// Transformation: emits visibility, opacity, params, implements clauses, and
/// union variants with stable indentation.
pub(crate) fn format_pattern(pattern: &Pattern) -> String {
    match pattern {
        Pattern::Wildcard => "_".to_string(),
        Pattern::Var(name) => name.clone(),
        Pattern::Int(value) => value.to_string(),
        Pattern::Float(value) => format_float_literal(*value),
        Pattern::String(value) => super::quoted_string_literal(value),
        Pattern::StringSegments(segments) => format_string_pattern(segments),
        Pattern::Atom(value) => value.clone(),
        Pattern::AtomLiteral(value) => {
            format!("Atom[{}]", super::quoted_string_literal(value))
        }
        Pattern::NullaryConstructorCall(name) => format!("{name}()"),
        Pattern::Tuple(items) => {
            if let Some(rendered) = format_constructor_pattern(items) {
                return rendered;
            }
            let parts = items
                .iter()
                .map(format_pattern)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{{}}}", parts)
        }
        Pattern::Alias { alias, pattern } => format!("{} = {}", format_pattern(pattern), alias),
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
        Pattern::BinaryLayout { endian, fields } => format_binary_layout(endian, fields),
    }
}
