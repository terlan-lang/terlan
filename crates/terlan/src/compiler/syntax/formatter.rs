use crate::terlan_syntax::parse_tree::{
    BinaryOp, CaseClause, Decl, Expr, MapExprField, MapField, Module, Pattern, TypeExpr, UnaryOp,
};
use crate::terlan_syntax::parser::{parse_interface_module, parse_module, ParseError};

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
    let mut out = String::new();
    if !module.docs.is_empty() {
        out.push_str(&format_docs(&module.docs, 0));
        out.push('\n');
    }
    out.push_str("module ");
    out.push_str(&module.name);
    out.push_str(".\n\n");

    for (i, decl) in ordered_declarations_for_format(module).iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let docs = format_decl_docs(decl);
        if !docs.is_empty() {
            out.push_str(&docs);
            out.push('\n');
        }
        out.push_str(&format_decl(decl));
        out.push('\n');
    }

    out
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
/// - Extracts import declarations, sorts them by their formatted source text,
///   and places them before non-import declarations. Non-import declarations
///   preserve source order to avoid reordering code with semantic bodies.
fn ordered_declarations_for_format(module: &Module) -> Vec<&Decl> {
    let mut imports = module
        .declarations
        .iter()
        .filter(|decl| matches!(decl, Decl::Import(_)))
        .collect::<Vec<_>>();
    imports.sort_by(|left, right| import_sort_text(left).cmp(&import_sort_text(right)));

    let mut ordered = imports;
    ordered.extend(
        module
            .declarations
            .iter()
            .filter(|decl| !matches!(decl, Decl::Import(_))),
    );
    ordered
}

/// Returns the canonical text used to order import declarations.
///
/// Inputs:
/// - `decl`: declaration known to be an import.
///
/// Output:
/// - Formatted import text, or an empty string for non-import callers.
///
/// Transformation:
/// - Reuses the import formatter so sorting follows the same canonical spelling
///   that will be written to disk.
fn import_sort_text(decl: &Decl) -> String {
    match decl {
        Decl::Import(import) => format_import(import),
        _ => String::new(),
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

#[cfg(test)]
#[path = "formatter_test.rs"]
mod formatter_test;
