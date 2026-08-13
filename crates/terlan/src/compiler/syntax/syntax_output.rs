macro_rules! expr_node {
    (
        $kind:expr,
        $text:expr,
        $operator:expr,
        $remote:expr,
        $children:expr,
        $patterns:expr,
        $fields:expr,
        $clauses:expr,
        $span:expr $(,)?
    ) => {{
        let children: Vec<SyntaxExprOutput> = $children;
        let patterns: Vec<SyntaxPatternOutput> = $patterns;
        let fields: Vec<SyntaxExprFieldOutput> = $fields;
        let clauses: Vec<SyntaxClauseOutput> = $clauses;
        SyntaxExprOutput {
            kind: $kind,
            arity: $crate::compiler::syntax::syntax_output::node_arity(
                &children, &patterns, &fields, &clauses,
            ),
            text: $text,
            span: $span,
            raw: None,
            comprehension_lift: None,
            type_args: Vec::new(),
            operator: $operator,
            remote: $remote,
            arg_names: Vec::new(),
            children,
            patterns,
            let_guards: Vec::new(),
            fields,
            clauses,
            catch_clauses: Vec::new(),
            try_after: None,
            html_nodes: Vec::new(),
        }
    }};
}

mod annotation_schemas;
mod annotations;
mod config;
mod declarations;
mod expressions;
mod html;
mod imports;
mod model;
mod modules;
mod patterns;
mod shapes;
mod text;
mod types;

use annotations::{
    annotation_output, annotation_schema_entry_output, validate_builtin_annotation_schemas,
};
use config::{config_declaration_target, is_config_declaration_kind, parse_config_entries};
pub use config::{SyntaxConfigEntryOutput, SyntaxConfigValueOutput};
use declarations::{declaration_docs, declaration_payload};
pub use expressions::{
    SyntaxClauseOutput, SyntaxExprFieldOutput, SyntaxExprKind, SyntaxExprOutput,
    SyntaxTryAfterOutput,
};
use html::html_node_output;
pub use html::{
    SyntaxHtmlAttrOutput, SyntaxHtmlAttrValueOutput, SyntaxHtmlElementOutput,
    SyntaxHtmlNamedSlotOutput, SyntaxHtmlNodeOutput,
};
pub use imports::{
    syntax_module_import_identity, SyntaxExportItem, SyntaxImportItem, SyntaxImportKind,
};
pub use model::{
    SyntaxAnnotationEntryOutput, SyntaxAnnotationKeyOptionOutput, SyntaxAnnotationOutput,
    SyntaxAnnotationSchemaEntryOutput, SyntaxAnnotationValueOutput, SyntaxConstructorClauseOutput,
    SyntaxConstructorParamOutput, SyntaxDeclarationOutput, SyntaxDeclarationPayload,
    SyntaxFunctionClauseOutput, SyntaxImplConstOutput, SyntaxImplMethodOutput,
    SyntaxStructFieldOutput, SyntaxTemplatePropOutput, SyntaxTraitConstOutput,
    SyntaxTraitMethodOutput, SyntaxValuedUnionArmOutput,
};
pub use modules::{SyntaxModuleOutput, SyntaxSourceKind, SYNTAX_MODULE_OUTPUT_SCHEMA};
use patterns::pattern_output;
pub use patterns::{SyntaxPatternFieldOutput, SyntaxPatternKind, SyntaxPatternOutput};
use shapes::expand_local_shape_synonyms;
pub use shapes::{expand_shape_imports, SyntaxShapeImport};
pub(crate) use text::binary_op_text;
use text::{expr_to_output_text, type_expr_output, unary_op_text};
pub use types::{SyntaxParamOutput, SyntaxTypeOutput};

use crate::terlan_syntax::{
    ebnf::{EbnfCompileError, EbnfCompileResult, EbnfSourceSpan},
    lexer::lex,
    parse_tree::{
        Annotation, AnnotationEntry, AnnotationKeyOption, AnnotationSchemaEntry, AnnotationValue,
        BinaryLayoutField, CaseClause, ConstructorClause, ConstructorParam, Decl, Expr,
        FunctionClause, FunctionDecl, IfClause, MapExprField, Module, Param, Pattern,
        TraitMethodDecl, TypeExpr,
    },
    parser::{parse_interface_module, parse_module},
    parser_contract::{contract_decl_class, decl_span, module_as_contract},
    syntax_contract::cached_canonical_terlan_syntax_contract_identity,
    token::{Token, TokenKind},
};

/// Parses a Terlan module into syntax-output form.
///
/// Inputs: module source text. Output: syntax-output module or compile error.
/// Transformation: parses source, attaches contract identity, validates
/// annotation schemas, and serializes declarations into formal output records.
pub fn parse_module_as_syntax_output(input: &str) -> EbnfCompileResult<SyntaxModuleOutput> {
    let module =
        parse_module(input).map_err(|err| EbnfCompileError::Parse(err.message, err.span))?;
    module_as_validated_syntax_output(&module, SyntaxSourceKind::Module)
}

/// Parses a headerless executable script into ordinary module syntax output.
///
/// The parser synthesizes public `main/0` while preserving source-authored
/// expression spans. Downstream resolution, typing, Core lowering, and AOT
/// compilation therefore use the exact maintained module pipeline.
pub fn parse_script_as_syntax_output(
    input: &str,
    module_name: &str,
) -> EbnfCompileResult<SyntaxModuleOutput> {
    let module = crate::terlan_syntax::parser::parse_script(input, module_name)
        .map_err(|err| EbnfCompileError::Parse(err.message, err.span))?;
    module_as_validated_syntax_output(&module, SyntaxSourceKind::Module)
}

/// Collects canonical provider identities for module imports.
///
/// Inputs:
/// - `module`: parsed syntax-output module.
///
/// Output:
/// - Sorted, deduplicated provider module identities.
///
/// Transformation:
/// - Excludes asset imports and canonicalizes both default and selected module
///   imports through `syntax_module_import_identity`.
pub fn syntax_module_import_identities(module: &SyntaxModuleOutput) -> Vec<String> {
    let mut imports = module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Import {
                import_kind: SyntaxImportKind::Module,
                module_name,
                items,
                is_selected,
                ..
            } => Some(syntax_module_import_identity(
                module_name,
                items,
                *is_selected,
            )),
            _ => None,
        })
        .collect::<Vec<_>>();
    imports.sort_unstable();
    imports.dedup();
    imports
}

/// Parses a Terlan interface module into syntax-output form.
///
/// Inputs: interface source text. Output: syntax-output module or compile
/// error. Transformation: uses the interface parser and marks the output source
/// kind as `Interface`.
pub fn parse_interface_module_as_syntax_output(
    input: &str,
) -> EbnfCompileResult<SyntaxModuleOutput> {
    let module = parse_interface_module(input)
        .map_err(|err| EbnfCompileError::Parse(err.message, err.span))?;
    module_as_validated_syntax_output(&module, SyntaxSourceKind::Interface)
}

/// Builds and validates syntax output from an already parsed module.
///
/// This boundary lets consumers that also need the parse tree avoid parsing the
/// same source twice while retaining syntax-output schema, annotation, and shape
/// validation.
pub(crate) fn module_as_validated_syntax_output(
    module: &Module,
    source_kind: SyntaxSourceKind,
) -> EbnfCompileResult<SyntaxModuleOutput> {
    let mut output = module_as_syntax_output(module, source_kind)?;
    if source_kind == SyntaxSourceKind::Module {
        expand_local_shape_synonyms(module, &mut output.declarations)?;
    }
    Ok(output)
}

/// Parses one Terlan expression into syntax-output form.
///
/// Inputs: expression source text. Output: syntax-output expression or compile
/// error. Transformation: parses the expression and lowers it to the formal
/// expression-output tree.
pub fn parse_expr_as_syntax_output(input: &str) -> EbnfCompileResult<SyntaxExprOutput> {
    let expr = crate::terlan_syntax::parser::parse_terlan_expr(input)
        .map_err(|err| EbnfCompileError::Parse(err.message, err.span))?;
    Ok(expr_output(&expr))
}

/// Converts a parsed module to syntax-output form.
///
/// Inputs: parsed `module` and source kind. Output: syntax-output module or
/// serialization error. Transformation: attaches canonical syntax contract
/// identity, declaration payloads, docs, annotations, and parser contract text.
fn module_as_syntax_output(
    module: &Module,
    source_kind: SyntaxSourceKind,
) -> EbnfCompileResult<SyntaxModuleOutput> {
    let syntax_contract = cached_canonical_terlan_syntax_contract_identity()
        .map_err(|error| EbnfCompileError::Serialize(format!("{error:?}")))?;
    let declarations = module
        .declarations
        .iter()
        .enumerate()
        .map(|(index, declaration)| SyntaxDeclarationOutput {
            index,
            class: contract_decl_class(declaration).to_string(),
            span: decl_span(declaration).into(),
            docs: declaration_docs(declaration),
            annotations: module
                .declaration_annotations
                .get(index)
                .map(|annotations| annotations.iter().map(annotation_output).collect())
                .unwrap_or_default(),
            payload: declaration_payload(declaration),
        })
        .collect::<Vec<_>>();
    validate_builtin_annotation_schemas(&declarations)?;

    Ok(SyntaxModuleOutput {
        schema: SYNTAX_MODULE_OUTPUT_SCHEMA.to_string(),
        source_kind,
        syntax_contract,
        module_name: module.name.clone(),
        docs: module.docs.clone(),
        span: module.span.into(),
        declarations,
        contract: module_as_contract(module)?,
    })
}

/// Converts an expression to syntax output with a default span.
///
/// Inputs: parsed expression. Output: syntax-output expression. Transformation:
/// delegates to span-aware lowering with an empty span.
fn expr_output(expr: &Expr) -> SyntaxExprOutput {
    expr_output_with_span(expr, EbnfSourceSpan::default())
}

/// Converts an expression to syntax output with a source span.
///
/// Inputs: parsed expression and span. Output: syntax-output expression tree.
/// Transformation: recursively maps parser expression variants into stable
/// syntax-output kinds, children, fields, clauses, and metadata.
fn expr_output_with_span(expr: &Expr, span: EbnfSourceSpan) -> SyntaxExprOutput {
    match expr {
        Expr::Int(value) => expr_leaf_with_span(SyntaxExprKind::Int, Some(value.to_string()), span),
        Expr::Float(value) => {
            expr_leaf_with_span(SyntaxExprKind::Float, Some(value.to_string()), span)
        }
        Expr::Atom(name) => expr_leaf_with_span(SyntaxExprKind::Atom, Some(name.clone()), span),
        Expr::AtomLiteral(name) => expr_leaf_with_span_and_raw(
            SyntaxExprKind::Atom,
            Some(name.clone()),
            Some(format_canonical_atom_literal_raw(name)),
            span,
        ),
        Expr::Binary(value) => {
            expr_leaf_with_span(SyntaxExprKind::Binary, Some(value.clone()), span)
        }
        Expr::Var(name) => expr_leaf_with_span(SyntaxExprKind::Var, Some(name.clone()), span),
        Expr::Tuple(items) => expr_node!(
            SyntaxExprKind::Tuple,
            None,
            None,
            None,
            items
                .iter()
                .map(|item| expr_output_with_span(item, span))
                .collect(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            span,
        ),
        Expr::List(items) => expr_node!(
            SyntaxExprKind::List,
            None,
            None,
            None,
            items
                .iter()
                .map(|item| expr_output_with_span(item, span))
                .collect(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            span,
        ),
        Expr::ListCons(head, tail) => expr_node!(
            SyntaxExprKind::ListCons,
            None,
            None,
            None,
            vec![
                expr_output_with_span(head, span),
                expr_output_with_span(tail, span),
            ],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            span,
        ),
        Expr::FixedArray(items) => expr_node!(
            SyntaxExprKind::FixedArray,
            None,
            None,
            None,
            items
                .iter()
                .map(|item| expr_output_with_span(item, span))
                .collect(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            span,
        ),
        Expr::Index(value, index) => expr_node!(
            SyntaxExprKind::Index,
            None,
            None,
            None,
            vec![
                expr_output_with_span(value, span),
                expr_output_with_span(index, span),
            ],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            span,
        ),
        Expr::IndexAssign {
            collection,
            index,
            value,
        } => expr_node!(
            SyntaxExprKind::IndexAssign,
            None,
            None,
            None,
            vec![
                expr_output_with_span(collection, span),
                expr_output_with_span(index, span),
                expr_output_with_span(value, span),
            ],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            span,
        ),
        Expr::Map(fields) => expr_node!(
            SyntaxExprKind::Map,
            None,
            None,
            None,
            Vec::new(),
            Vec::new(),
            fields
                .iter()
                .map(|field| expr_field_output_with_span(field, span))
                .collect(),
            Vec::new(),
            span,
        ),
        Expr::ListComprehension {
            expr,
            generators,
            guards,
        } => {
            let mut children = vec![expr_output_with_span(expr, span)];
            children.extend(
                generators
                    .iter()
                    .map(|generator| expr_output_with_span(&generator.source, span)),
            );
            children.extend(
                guards
                    .iter()
                    .map(|guard| expr_output_with_span(guard, span)),
            );
            expr_node!(
                SyntaxExprKind::ListComprehension,
                None,
                None,
                None,
                children,
                generators
                    .iter()
                    .map(|generator| pattern_output(&generator.pattern))
                    .collect(),
                Vec::new(),
                Vec::new(),
                span,
            )
            .with_arity(1 + generators.len() + guards.len())
        }
        Expr::Let {
            bindings,
            else_clauses,
            body,
        } => {
            let mut children = bindings
                .iter()
                .map(|binding| expr_output_with_span(&binding.value, span))
                .collect::<Vec<_>>();
            if let Some(body) = body {
                children.push(expr_output_with_span(body, span));
            }

            expr_node!(
                SyntaxExprKind::Let,
                None,
                None,
                None,
                children,
                bindings
                    .iter()
                    .map(|binding| pattern_output(&binding.pattern))
                    .collect(),
                Vec::new(),
                else_clauses
                    .iter()
                    .map(|clause| case_clause_output(clause, span))
                    .collect(),
                span,
            )
            .with_arity(bindings.len())
        }
        Expr::Call {
            callee,
            type_args,
            args,
            arg_names,
            remote,
            is_fun_value,
        } => {
            let mut children = Vec::with_capacity(args.len() + 1);
            children.push(expr_output_with_span(callee, span));
            children.extend(args.iter().map(|arg| expr_output_with_span(arg, span)));
            let mut output = expr_node!(
                if *is_fun_value {
                    SyntaxExprKind::FunctionCall
                } else {
                    SyntaxExprKind::Call
                },
                None,
                None,
                remote.clone(),
                children,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                span,
            )
            .with_arity(args.len());
            output.arg_names = arg_names.clone();
            output.type_args = type_args.iter().map(type_expr_output).collect();
            output
        }
        Expr::Case { scrutinee, clauses } => expr_node!(
            SyntaxExprKind::Case,
            None,
            None,
            None,
            vec![expr_output_with_span(scrutinee, span)],
            Vec::new(),
            Vec::new(),
            clauses
                .iter()
                .map(|clause| case_clause_output(clause, span))
                .collect(),
            span,
        ),
        Expr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            let mut output = expr_node!(
                SyntaxExprKind::Try,
                None,
                None,
                None,
                vec![expr_output_with_span(body, span)],
                Vec::new(),
                Vec::new(),
                of_clauses
                    .iter()
                    .map(|clause| case_clause_output(clause, span))
                    .collect(),
                span,
            );
            output.catch_clauses = catch_clauses
                .iter()
                .map(|clause| case_clause_output(clause, span))
                .collect();
            output.try_after = after_clause.as_ref().map(|after| SyntaxTryAfterOutput {
                trigger: Box::new(expr_output_with_span(&after.trigger, span)),
                body: Box::new(expr_output_with_span(&after.body, span)),
            });
            output
        }
        Expr::If { clauses } => expr_node!(
            SyntaxExprKind::If,
            None,
            None,
            None,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            clauses
                .iter()
                .map(|clause| if_clause_output(clause, span))
                .collect(),
            span,
        ),
        Expr::Fun { clauses } => expr_node!(
            SyntaxExprKind::Fun,
            None,
            None,
            None,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            clauses
                .iter()
                .map(|clause| function_clause_output_with_span(clause, span))
                .collect(),
            span,
        ),
        Expr::MacroCall { name, args } => expr_node!(
            SyntaxExprKind::Macro,
            Some(name.clone()),
            None,
            None,
            args.iter()
                .map(|arg| expr_output_with_span(arg, span))
                .collect(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            span,
        )
        .with_arity(args.len()),
        Expr::RawMacro {
            name,
            type_args,
            interpolations,
            raw,
        } => {
            let mut output = expr_node!(
                SyntaxExprKind::RawMacro,
                Some(name.clone()),
                None,
                None,
                interpolations
                    .iter()
                    .map(|expr| expr_output_with_span(expr, span))
                    .collect(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                span,
            );
            output.raw = Some(raw.clone());
            output.type_args = type_args.iter().map(type_expr_output).collect();
            output
        }
        Expr::HtmlBlock(block) => {
            let html_nodes = block.nodes.iter().map(html_node_output).collect::<Vec<_>>();
            let mut output = expr_node!(
                SyntaxExprKind::HtmlBlock,
                Some(block.raw.clone()),
                None,
                None,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                span,
            )
            .with_arity(block.nodes.len());
            output.html_nodes = html_nodes;
            output
        }
        Expr::RecordAccess { value, name, field } => expr_node!(
            SyntaxExprKind::RecordAccess,
            Some(format!("{name}.{field}")),
            None,
            None,
            vec![expr_output_with_span(value, span)],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            span,
        ),
        Expr::FieldAccess { value, field } => expr_node!(
            SyntaxExprKind::FieldAccess,
            Some(field.clone()),
            None,
            None,
            vec![expr_output_with_span(value, span)],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            span,
        ),
        Expr::RecordUpdate {
            value,
            name,
            fields,
        } => expr_node!(
            SyntaxExprKind::RecordUpdate,
            Some(name.clone()),
            None,
            None,
            vec![expr_output_with_span(value, span)],
            Vec::new(),
            fields
                .iter()
                .map(|field| expr_field_output_with_span(field, span))
                .collect(),
            Vec::new(),
            span,
        ),
        Expr::RecordConstruct { name, fields } => expr_node!(
            SyntaxExprKind::RecordConstruct,
            Some(name.clone()),
            None,
            None,
            Vec::new(),
            Vec::new(),
            fields
                .iter()
                .map(|field| expr_field_output_with_span(field, span))
                .collect(),
            Vec::new(),
            span,
        ),
        Expr::BinaryLayout { endian, fields } => expr_node!(
            SyntaxExprKind::BinaryLayout,
            Some(endian.clone()),
            None,
            None,
            Vec::new(),
            Vec::new(),
            fields
                .iter()
                .map(|field| binary_layout_expr_field_output(field, span))
                .collect(),
            Vec::new(),
            span,
        ),
        Expr::ConstructorChain { base, record } => expr_node!(
            SyntaxExprKind::ConstructorChain,
            None,
            None,
            None,
            vec![
                expr_output_with_span(base, span),
                expr_output_with_span(record, span),
            ],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            span,
        ),
        Expr::UnaryOp { op, expr } => expr_node!(
            SyntaxExprKind::UnaryOp,
            None,
            Some(unary_op_text(op).to_string()),
            None,
            vec![expr_output_with_span(expr, span)],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            span,
        )
        .with_arity(1),
        Expr::Cast { expr, target_type } => expr_node!(
            SyntaxExprKind::Cast,
            Some(target_type.text.clone()),
            Some("as".to_string()),
            None,
            vec![expr_output_with_span(expr, span)],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            span,
        )
        .with_arity(1),
        Expr::BinaryOp { .. } => binary_expr_output_with_span(expr, span),
        Expr::Quote(inner) => expr_node!(
            SyntaxExprKind::Quote,
            None,
            None,
            None,
            vec![expr_output_with_span(inner, span)],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            span,
        ),
        Expr::Unquote(inner) => expr_node!(
            SyntaxExprKind::Unquote,
            None,
            None,
            None,
            vec![expr_output_with_span(inner, span)],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            span,
        ),
        Expr::Sequence(expressions) => expr_node!(
            SyntaxExprKind::Sequence,
            None,
            None,
            None,
            expressions
                .iter()
                .map(|expr| expr_output_with_span(expr, span))
                .collect(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            span,
        ),
    }
}

mod expression_nodes;

pub(crate) use expression_nodes::node_arity;
use expression_nodes::{
    binary_expr_output_with_span, expr_leaf_with_span, expr_leaf_with_span_and_raw,
    format_canonical_atom_literal_raw, SyntaxExprArity,
};

/// Converts a map/record expression field to syntax output.
///
/// Inputs: parsed field and span. Output: syntax-output field. Transformation:
/// preserves key/operator intent and recursively lowers the field value.
fn expr_field_output_with_span(
    field: &MapExprField,
    span: EbnfSourceSpan,
) -> SyntaxExprFieldOutput {
    SyntaxExprFieldOutput {
        key: field.key.clone(),
        required: field.required,
        value: Box::new(expr_output_with_span(&field.value, span)),
    }
}

/// Converts a binary layout descriptor field to syntax output.
fn binary_layout_expr_field_output(
    field: &BinaryLayoutField,
    span: EbnfSourceSpan,
) -> SyntaxExprFieldOutput {
    SyntaxExprFieldOutput {
        key: field.name.clone(),
        required: true,
        value: Box::new(expr_leaf_with_span(
            SyntaxExprKind::Var,
            Some(field.descriptor.text.clone()),
            span,
        )),
    }
}

/// Converts a case clause to syntax output.
///
/// Inputs: parsed case clause and span. Output: syntax-output clause.
/// Transformation: lowers the pattern, optional guard, and body expression.
fn case_clause_output(clause: &CaseClause, span: EbnfSourceSpan) -> SyntaxClauseOutput {
    SyntaxClauseOutput {
        patterns: vec![pattern_output(&clause.pattern)],
        guard: clause
            .guard
            .as_deref()
            .map(|guard| Box::new(expr_output_with_span(guard, span))),
        body: Box::new(expr_output_with_span(&clause.body, span)),
    }
}

/// Converts an if clause to syntax output.
///
/// Inputs: parsed if clause and span. Output: syntax-output clause.
/// Transformation: stores the condition as the clause guard and lowers the
/// branch body.
fn if_clause_output(clause: &IfClause, span: EbnfSourceSpan) -> SyntaxClauseOutput {
    SyntaxClauseOutput {
        patterns: Vec::new(),
        guard: Some(Box::new(expr_output_with_span(&clause.condition, span))),
        body: Box::new(expr_output_with_span(&clause.body, span)),
    }
}

/// Converts a function or lambda clause to syntax output.
///
/// Inputs: parsed function clause and span. Output: syntax-output clause.
/// Transformation: lowers patterns, optional guard, and body expression.
fn function_clause_output_with_span(
    clause: &FunctionClause,
    span: EbnfSourceSpan,
) -> SyntaxClauseOutput {
    SyntaxClauseOutput {
        patterns: clause.patterns.iter().map(pattern_output).collect(),
        guard: clause
            .guard
            .as_deref()
            .map(|guard| Box::new(expr_output_with_span(guard, span))),
        body: Box::new(expr_output_with_span(&clause.body, span)),
    }
}

#[cfg(test)]
#[path = "syntax_output_decl_test.rs"]
#[cfg(test)]
mod syntax_output_decl_test;

#[cfg(test)]
#[path = "syntax_output_expr_test.rs"]
#[cfg(test)]
mod syntax_output_expr_test;

#[cfg(test)]
#[path = "syntax_output_html_test.rs"]
#[cfg(test)]
mod syntax_output_html_test;

#[cfg(test)]
#[path = "syntax_output_pattern_test.rs"]
#[cfg(test)]
mod syntax_output_pattern_test;

#[cfg(test)]
#[path = "syntax_output/shapes_test.rs"]
#[cfg(test)]
mod shapes_test;

#[cfg(test)]
#[path = "syntax_output/shapes/overlap_predicate_implication_test.rs"]
#[cfg(test)]
mod shape_overlap_predicate_implication_test;

#[cfg(test)]
#[path = "syntax_output/shapes/overlap_relation_transitive_test.rs"]
#[cfg(test)]
mod shape_overlap_relation_transitive_test;

#[cfg(test)]
#[path = "syntax_output/shapes/overlap_test.rs"]
#[cfg(test)]
mod shape_overlap_test;
