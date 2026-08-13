mod annotations;
mod callables;
mod data;

use super::{
    super::{
        lalrpop_syntax::{
            LalrpopModuleSyntaxOutput, LalrpopSyntaxNode, LalrpopSyntaxNodeKind as Kind,
        },
        lexer::lex,
        parse_tree::{Decl, ExportDecl, ExportItem, Module},
        token::TokenKind,
    },
    LalrpopLoweringContext, LalrpopLoweringResult,
};

pub(super) fn lower_module(
    source: &str,
    output: &LalrpopModuleSyntaxOutput,
    is_interface: bool,
) -> LalrpopLoweringResult<Module> {
    let context = LalrpopLoweringContext::new(source);
    let declarations = output.root.children.get(1..).unwrap_or_default();
    let docs = module_docs(source);
    reject_misplaced_module_docs(source, &output.root.children[0])?;
    let item_docs = declaration_docs(source, &output.root.children[0], declarations);
    let mut lowered = Vec::with_capacity(declarations.len());
    let mut declaration_annotations = Vec::with_capacity(declarations.len());
    for (index, node) in declarations.iter().enumerate() {
        let (annotations, declaration) = split_annotations(node);
        declaration_annotations.push(
            annotations
                .iter()
                .map(|node| annotations::lower_annotation(&context, node))
                .collect::<LalrpopLoweringResult<Vec<_>>>()?,
        );
        let declaration = lower_declaration(
            &context,
            declaration,
            item_docs.get(index).cloned().unwrap_or_default(),
            is_interface,
        )?;
        if let Decl::Function(next) = declaration {
            if let Some(Decl::Function(previous)) = lowered.last_mut() {
                if previous.name == next.name
                    && previous.clauses.is_empty()
                    && !next.clauses.is_empty()
                {
                    previous.clauses = next.clauses;
                    declaration_annotations.pop();
                    continue;
                }
            }
            lowered.push(Decl::Function(next));
        } else {
            lowered.push(declaration);
        }
    }
    Ok(Module {
        name: output.module_name.clone(),
        docs,
        declarations: lowered,
        declaration_annotations,
        span: output.root.span,
    })
}

fn lower_declaration(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
    docs: Vec<String>,
    is_interface: bool,
) -> LalrpopLoweringResult<Decl> {
    match node.kind {
        Kind::ImportDeclaration => data::lower_import(context, node),
        Kind::ExportDeclaration => lower_export(context, node),
        Kind::ConstantDeclaration => data::lower_constant(context, node, docs),
        Kind::TypeDeclaration => data::lower_type(context, node, docs, is_interface),
        Kind::StructDeclaration => data::lower_struct(context, node, docs),
        Kind::ConstructorDeclaration => callables::lower_constructor(context, node, docs),
        Kind::FunctionDeclaration => callables::lower_function(context, node, docs),
        Kind::MethodDeclaration => callables::lower_method(context, node, docs),
        Kind::TraitDeclaration => data::lower_trait(context, node, docs),
        Kind::TraitImplementationDeclaration => data::lower_trait_impl(context, node, docs),
        Kind::TemplateDeclaration => data::lower_template(context, node, docs),
        Kind::ShapeDeclaration => data::lower_shape(context, node, docs),
        Kind::Annotation => data::lower_annotation_schema(context, node, docs),
        Kind::RawMacro | Kind::ConfigDeclaration => data::lower_raw(context, node, docs),
        _ => Err(context.error(
            node,
            format!("generated declaration {:?} has no lowering", node.kind),
        )),
    }
}

fn lower_export(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
) -> LalrpopLoweringResult<Decl> {
    let items = node
        .children
        .iter()
        .map(|item| {
            let text = item.text.as_deref().unwrap_or_default();
            let (name, arity) = text
                .rsplit_once('/')
                .ok_or_else(|| context.error(item, "interface export item is malformed"))?;
            let arity = arity
                .parse()
                .map_err(|_| context.error(item, "expected numeric arity"))?;
            Ok(ExportItem {
                name: name.to_string(),
                arity,
                span: item.span,
            })
        })
        .collect::<LalrpopLoweringResult<Vec<_>>>()?;
    Ok(Decl::Export(ExportDecl {
        items,
        span: node.span,
    }))
}

fn split_annotations(node: &LalrpopSyntaxNode) -> (&[LalrpopSyntaxNode], &LalrpopSyntaxNode) {
    let count = node
        .children
        .iter()
        .take_while(|child| child.kind == Kind::Annotation)
        .count();
    // The grammar attaches annotation nodes to the declaration rather than
    // wrapping it, so the declaration identity remains on `node`.
    (&node.children[..count], node)
}

fn module_docs(source: &str) -> Vec<String> {
    lex(source)
        .unwrap_or_default()
        .into_iter()
        .take_while(|token| {
            matches!(
                token.kind,
                TokenKind::Comment | TokenKind::ModuleDocComment | TokenKind::DocBlockComment
            )
        })
        .filter(|token| {
            matches!(
                token.kind,
                TokenKind::ModuleDocComment | TokenKind::DocBlockComment
            )
        })
        .map(|token| token.text)
        .collect()
}

fn declaration_docs(
    source: &str,
    module: &LalrpopSyntaxNode,
    declarations: &[LalrpopSyntaxNode],
) -> Vec<Vec<String>> {
    let tokens = lex(source).unwrap_or_default();
    let mut previous_end = module.span.end;
    declarations
        .iter()
        .map(|declaration| {
            let docs = tokens
                .iter()
                .filter(|token| {
                    token.start >= previous_end
                        && token.end <= declaration.span.start
                        && matches!(
                            token.kind,
                            TokenKind::DocComment | TokenKind::DocBlockComment
                        )
                        && !token.text.contains("@module")
                })
                .map(|token| token.text.clone())
                .collect();
            previous_end = declaration.span.end;
            docs
        })
        .collect()
}

fn reject_misplaced_module_docs(
    source: &str,
    module: &LalrpopSyntaxNode,
) -> LalrpopLoweringResult<()> {
    if let Some(token) = lex(source).unwrap_or_default().into_iter().find(|token| {
        token.start >= module.span.end
            && (token.kind == TokenKind::ModuleDocComment
                || (token.kind == TokenKind::DocBlockComment && token.text.contains("@module")))
    }) {
        let message = if token.kind == TokenKind::ModuleDocComment {
            "module doc comments (`//!`) must appear before the module declaration"
        } else {
            "module documentation blocks (`/** ... @module ... */`) must appear before the module declaration"
        };
        return Err(super::LalrpopLoweringError {
            message: message.to_string(),
            span: token.span(),
        });
    }
    Ok(())
}

pub(super) fn metadata_count(node: &LalrpopSyntaxNode, key: &str) -> usize {
    node.text
        .as_deref()
        .unwrap_or_default()
        .split(';')
        .find_map(|part| part.strip_prefix(&format!("{key}:")))
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

pub(super) fn metadata_bool(node: &LalrpopSyntaxNode, key: &str) -> bool {
    node.text
        .as_deref()
        .unwrap_or_default()
        .split(';')
        .find_map(|part| part.strip_prefix(&format!("{key}:")))
        == Some("true")
}

pub(super) fn head_constraint_texts(
    context: &LalrpopLoweringContext<'_>,
    nodes: &[LalrpopSyntaxNode],
) -> Vec<String> {
    nodes
        .iter()
        .flat_map(|node| {
            let raw = context.text(node.span);
            let inner = raw
                .strip_prefix('<')
                .and_then(|value| value.strip_suffix('>'))
                .unwrap_or(raw);
            let mut parts = Vec::new();
            let mut start = 0usize;
            let mut depth = 0usize;
            for (offset, character) in inner.char_indices() {
                match character {
                    '[' | '(' | '{' | '<' => depth += 1,
                    ']' | ')' | '}' | '>' => depth = depth.saturating_sub(1),
                    ',' if depth == 0 => {
                        parts.push(inner[start..offset].trim().to_string());
                        start = offset + character.len_utf8();
                    }
                    _ => {}
                }
            }
            parts.push(inner[start..].trim().to_string());
            parts
        })
        .filter(|constraint| !constraint.is_empty())
        .collect()
}

pub(super) fn declaration_identity(node: &LalrpopSyntaxNode) -> (String, bool, bool, bool) {
    let head = node
        .text
        .as_deref()
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default();
    let public = head.starts_with("pub ");
    let head = head.strip_prefix("pub ").unwrap_or(head);
    let is_const = head.starts_with("const ");
    let head = head.strip_prefix("const ").unwrap_or(head);
    let is_macro = head.starts_with("macro ");
    let head = head.strip_prefix("macro ").unwrap_or(head);
    (head.to_string(), public, is_const, is_macro)
}

pub(super) fn without_annotations(node: &LalrpopSyntaxNode) -> &[LalrpopSyntaxNode] {
    let count = node
        .children
        .iter()
        .take_while(|child| child.kind == Kind::Annotation)
        .count();
    &node.children[count..]
}
