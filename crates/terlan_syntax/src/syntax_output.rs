use serde::{Deserialize, Serialize};

mod annotations;
mod config;
mod declarations;
mod html;

use annotations::{
    annotation_output, annotation_schema_entry_output, validate_builtin_annotation_schemas,
};
use config::{config_declaration_target, is_config_declaration_kind, parse_config_entries};
use declarations::{declaration_docs, declaration_payload};
use html::html_node_output;

use crate::{
    ebnf::{EbnfCompileError, EbnfCompileResult, EbnfGrammarContract, EbnfSourceSpan},
    lexer::lex,
    parse_tree::{
        Annotation, AnnotationEntry, AnnotationKeyOption, AnnotationSchemaEntry, AnnotationValue,
        BinaryOp, CaseClause, ConstructorClause, ConstructorParam, Decl, Expr, FunctionClause,
        FunctionDecl, HtmlAttr, HtmlAttrValue, HtmlElement, HtmlNamedSlot, HtmlNode, IfClause,
        ImportKind, MapExprField, MapField, Module, Param, Pattern, TraitMethodDecl, TypeExpr,
        UnaryOp,
    },
    parser::{parse_interface_module, parse_module},
    parser_contract::{contract_decl_class, decl_span, module_as_contract},
    syntax_contract::{cached_canonical_terlan_syntax_contract_identity, SyntaxContractIdentity},
    token::{Token, TokenKind},
};

pub const SYNTAX_MODULE_OUTPUT_SCHEMA: &str = "terlan-syntax-module-output-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyntaxSourceKind {
    Module,
    Interface,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxModuleOutput {
    pub schema: String,
    pub source_kind: SyntaxSourceKind,
    pub syntax_contract: SyntaxContractIdentity,
    pub module_name: String,
    pub docs: Vec<String>,
    pub span: EbnfSourceSpan,
    pub declarations: Vec<SyntaxDeclarationOutput>,
    pub contract: EbnfGrammarContract,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxDeclarationOutput {
    pub index: usize,
    pub class: String,
    pub span: EbnfSourceSpan,
    pub docs: Vec<String>,
    pub annotations: Vec<SyntaxAnnotationOutput>,
    pub payload: SyntaxDeclarationPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxAnnotationOutput {
    pub path: Vec<String>,
    pub args: Option<String>,
    pub entries: Vec<SyntaxAnnotationEntryOutput>,
    pub values: Vec<SyntaxAnnotationValueOutput>,
    pub span: EbnfSourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxAnnotationEntryOutput {
    pub key: Vec<String>,
    pub value: SyntaxAnnotationValueOutput,
    pub span: EbnfSourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SyntaxAnnotationValueOutput {
    Name {
        segments: Vec<String>,
    },
    Bool {
        value: bool,
    },
    Int {
        text: String,
    },
    Float {
        text: String,
    },
    String {
        text: String,
    },
    List {
        values: Vec<SyntaxAnnotationValueOutput>,
    },
    Object {
        entries: Vec<SyntaxAnnotationEntryOutput>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SyntaxDeclarationPayload {
    Import {
        import_kind: SyntaxImportKind,
        module_name: String,
        items: Vec<SyntaxImportItem>,
        is_type: bool,
        source_path: Option<String>,
    },
    Export {
        items: Vec<SyntaxExportItem>,
    },
    Type {
        name: String,
        params: Vec<String>,
        is_public: bool,
        is_opaque: bool,
        implements: Vec<SyntaxTypeOutput>,
        variants: Vec<SyntaxTypeOutput>,
    },
    Struct {
        name: String,
        derives: Vec<String>,
        implements: Vec<SyntaxTypeOutput>,
        is_public: bool,
        fields: Vec<SyntaxStructFieldOutput>,
    },
    Constructor {
        name: String,
        params: Vec<String>,
        is_public: bool,
        clauses: Vec<SyntaxConstructorClauseOutput>,
    },
    Function {
        name: String,
        params: Vec<SyntaxParamOutput>,
        return_type: SyntaxTypeOutput,
        is_public: bool,
        is_macro: bool,
        generic_bounds: Vec<String>,
        clauses: Vec<SyntaxFunctionClauseOutput>,
    },
    Method {
        receiver: SyntaxParamOutput,
        name: String,
        params: Vec<SyntaxParamOutput>,
        return_type: SyntaxTypeOutput,
        is_public: bool,
        generic_bounds: Vec<String>,
        clauses: Vec<SyntaxFunctionClauseOutput>,
    },
    Trait {
        name: String,
        params: Vec<String>,
        super_traits: Vec<String>,
        is_public: bool,
        methods: Vec<SyntaxTraitMethodOutput>,
    },
    TraitImpl {
        trait_ref: SyntaxTypeOutput,
        for_type: SyntaxTypeOutput,
        is_public: bool,
        methods: Vec<SyntaxImplMethodOutput>,
    },
    AnnotationSchema {
        path: Vec<String>,
        is_public: bool,
        entries: Vec<SyntaxAnnotationSchemaEntryOutput>,
    },
    Template {
        name: String,
        source_path: String,
        props: Vec<SyntaxTemplatePropOutput>,
    },
    Config {
        name: String,
        target: String,
        text: String,
        entries: Vec<SyntaxConfigEntryOutput>,
    },
    Raw {
        raw_kind: String,
        text: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SyntaxAnnotationSchemaEntryOutput {
    AppliesTo {
        targets: Vec<String>,
        span: EbnfSourceSpan,
    },
    Key {
        key: Vec<String>,
        value_type: String,
        options: Vec<SyntaxAnnotationKeyOptionOutput>,
        span: EbnfSourceSpan,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SyntaxAnnotationKeyOptionOutput {
    Required {
        value: bool,
        span: EbnfSourceSpan,
    },
    Repeatable {
        value: bool,
        span: EbnfSourceSpan,
    },
    Default {
        value: SyntaxAnnotationValueOutput,
        span: EbnfSourceSpan,
    },
    AppliesTo {
        targets: Vec<String>,
        span: EbnfSourceSpan,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxConfigEntryOutput {
    pub key: String,
    pub value: SyntaxConfigValueOutput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SyntaxConfigValueOutput {
    Bool {
        value: bool,
    },
    Symbol {
        value: String,
    },
    Int {
        value: String,
    },
    Float {
        value: String,
    },
    String {
        value: String,
    },
    List {
        values: Vec<SyntaxConfigValueOutput>,
    },
    Map {
        entries: Vec<SyntaxConfigEntryOutput>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyntaxImportKind {
    Module,
    File,
    Css,
    Markdown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxImportItem {
    pub name: String,
    pub as_alias: Option<String>,
    pub span: EbnfSourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxExportItem {
    pub name: String,
    pub arity: usize,
    pub span: EbnfSourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxTypeOutput {
    pub text: String,
    pub span: EbnfSourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxParamOutput {
    pub name: String,
    pub annotation: SyntaxTypeOutput,
    #[serde(default, rename = "mutable", skip_serializing_if = "is_false")]
    pub is_mutable: bool,
    pub span: EbnfSourceSpan,
}

/// Returns whether a boolean value is false for compact syntax JSON output.
///
/// Inputs:
/// - `value`: boolean metadata value being considered for serialization.
///
/// Output:
/// - `true` when the value is false and can be omitted from serialized output.
///
/// Transformation:
/// - Performs a direct boolean negation with no side effects.
fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxStructFieldOutput {
    pub name: String,
    pub annotation: SyntaxTypeOutput,
    #[serde(default)]
    pub docs: Vec<String>,
    pub has_default: bool,
    #[serde(default)]
    pub default: Option<SyntaxExprOutput>,
    pub span: EbnfSourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxConstructorParamOutput {
    pub name: String,
    pub annotation: SyntaxTypeOutput,
    pub has_default: bool,
    #[serde(default)]
    pub default: Option<SyntaxExprOutput>,
    pub is_varargs: bool,
    pub span: EbnfSourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxConstructorClauseOutput {
    pub params: Vec<SyntaxConstructorParamOutput>,
    pub return_type: SyntaxTypeOutput,
    pub body: SyntaxExprOutput,
    pub body_text: String,
    pub span: EbnfSourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxFunctionClauseOutput {
    pub patterns: Vec<SyntaxPatternOutput>,
    pub guard: Option<SyntaxExprOutput>,
    pub body: SyntaxExprOutput,
    pub has_guard: bool,
    pub span: EbnfSourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxPatternOutput {
    pub kind: SyntaxPatternKind,
    pub arity: usize,
    pub text: Option<String>,
    pub children: Vec<SyntaxPatternOutput>,
    pub fields: Vec<SyntaxPatternFieldOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxPatternFieldOutput {
    pub key: String,
    pub required: bool,
    pub value: Box<SyntaxPatternOutput>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyntaxPatternKind {
    Wildcard,
    Var,
    Int,
    Float,
    Atom,
    Tuple,
    List,
    ListCons,
    Constructor,
    Map,
    MapField,
    Ignore,
    Placeholder,
    Record,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxExprOutput {
    pub kind: SyntaxExprKind,
    pub arity: usize,
    pub text: Option<String>,
    #[serde(default)]
    pub span: EbnfSourceSpan,
    #[serde(default)]
    pub raw: Option<String>,
    pub operator: Option<String>,
    pub remote: Option<String>,
    pub children: Vec<SyntaxExprOutput>,
    pub patterns: Vec<SyntaxPatternOutput>,
    pub fields: Vec<SyntaxExprFieldOutput>,
    pub clauses: Vec<SyntaxClauseOutput>,
    #[serde(default)]
    pub catch_clauses: Vec<SyntaxClauseOutput>,
    #[serde(default)]
    pub try_after: Option<SyntaxTryAfterOutput>,
    #[serde(default)]
    pub html_nodes: Vec<SyntaxHtmlNodeOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxExprFieldOutput {
    pub key: String,
    pub required: bool,
    pub value: Box<SyntaxExprOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxClauseOutput {
    pub patterns: Vec<SyntaxPatternOutput>,
    pub guard: Option<Box<SyntaxExprOutput>>,
    pub body: Box<SyntaxExprOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxTryAfterOutput {
    pub trigger: Box<SyntaxExprOutput>,
    pub body: Box<SyntaxExprOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SyntaxHtmlNodeOutput {
    Text { text: String },
    Expr { expr: Box<SyntaxExprOutput> },
    Element { element: SyntaxHtmlElementOutput },
    NamedSlot { slot: SyntaxHtmlNamedSlotOutput },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxHtmlElementOutput {
    pub name: String,
    pub attrs: Vec<SyntaxHtmlAttrOutput>,
    pub children: Vec<SyntaxHtmlNodeOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxHtmlNamedSlotOutput {
    pub name: String,
    pub children: Vec<SyntaxHtmlNodeOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxHtmlAttrOutput {
    pub name: String,
    pub value: Option<SyntaxHtmlAttrValueOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SyntaxHtmlAttrValueOutput {
    Text { text: String },
    Expr { expr: Box<SyntaxExprOutput> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyntaxExprKind {
    Int,
    Float,
    Atom,
    Binary,
    Var,
    Tuple,
    List,
    ListCons,
    FixedArray,
    Index,
    IndexAssign,
    Map,
    ListComprehension,
    Let,
    Call,
    Case,
    Try,
    If,
    Fun,
    FunctionCall,
    RemoteFunRef,
    Macro,
    RawMacro,
    HtmlBlock,
    RecordAccess,
    FieldAccess,
    RecordUpdate,
    RecordConstruct,
    TemplateInstantiate,
    ConstructorChain,
    UnaryOp,
    Cast,
    BinaryOp,
    Quote,
    Unquote,
    Sequence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxTraitMethodOutput {
    pub name: String,
    pub params: Vec<SyntaxParamOutput>,
    pub return_type: SyntaxTypeOutput,
    pub generic_bounds: Vec<String>,
    #[serde(default)]
    pub default_body: Option<SyntaxExprOutput>,
    pub is_public: bool,
    pub docs: Vec<String>,
    pub span: EbnfSourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxImplMethodOutput {
    pub name: String,
    pub params: Vec<SyntaxParamOutput>,
    pub return_type: SyntaxTypeOutput,
    pub generic_bounds: Vec<String>,
    pub clauses: Vec<SyntaxFunctionClauseOutput>,
    pub span: EbnfSourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxTemplatePropOutput {
    pub name: String,
    pub annotation: SyntaxTypeOutput,
    pub span: EbnfSourceSpan,
}

pub fn parse_module_as_syntax_output(input: &str) -> EbnfCompileResult<SyntaxModuleOutput> {
    let module =
        parse_module(input).map_err(|err| EbnfCompileError::Parse(err.message, err.span))?;
    module_as_syntax_output(&module, SyntaxSourceKind::Module)
}

pub fn parse_interface_module_as_syntax_output(
    input: &str,
) -> EbnfCompileResult<SyntaxModuleOutput> {
    let module = parse_interface_module(input)
        .map_err(|err| EbnfCompileError::Parse(err.message, err.span))?;
    module_as_syntax_output(&module, SyntaxSourceKind::Interface)
}

pub fn parse_expr_as_syntax_output(input: &str) -> EbnfCompileResult<SyntaxExprOutput> {
    let expr = crate::parser::parse_terlan_expr(input)
        .map_err(|err| EbnfCompileError::Parse(err.message, err.span))?;
    Ok(expr_output(&expr))
}

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

fn pattern_output(pattern: &Pattern) -> SyntaxPatternOutput {
    match pattern {
        Pattern::Wildcard => pattern_leaf(SyntaxPatternKind::Wildcard, None),
        Pattern::Var(name) => pattern_leaf(SyntaxPatternKind::Var, Some(name.clone())),
        Pattern::Int(value) => pattern_leaf(SyntaxPatternKind::Int, Some(value.to_string())),
        Pattern::Float(value) => pattern_leaf(SyntaxPatternKind::Float, Some(value.to_string())),
        Pattern::Atom(name) => pattern_leaf(SyntaxPatternKind::Atom, Some(name.clone())),
        Pattern::Tuple(items) if is_constructor_pattern_tuple(items) => {
            let Pattern::Atom(name) = &items[0] else {
                unreachable!("constructor pattern tuple starts with atom");
            };
            pattern_node(
                SyntaxPatternKind::Constructor,
                Some(name.clone()),
                items.iter().skip(1).map(pattern_output).collect(),
                Vec::new(),
            )
        }
        Pattern::Tuple(items) => pattern_node(
            SyntaxPatternKind::Tuple,
            None,
            items.iter().map(pattern_output).collect(),
            Vec::new(),
        ),
        Pattern::List(items) => pattern_node(
            SyntaxPatternKind::List,
            None,
            items.iter().map(pattern_output).collect(),
            Vec::new(),
        ),
        Pattern::ListCons(head, tail) => pattern_node(
            SyntaxPatternKind::ListCons,
            None,
            vec![pattern_output(head), pattern_output(tail)],
            Vec::new(),
        ),
        Pattern::Map(fields) => pattern_node(
            SyntaxPatternKind::Map,
            None,
            Vec::new(),
            fields.iter().map(pattern_field_output).collect(),
        ),
        Pattern::Record { name, fields } => pattern_node(
            SyntaxPatternKind::Record,
            Some(name.clone()),
            Vec::new(),
            fields.iter().map(pattern_field_output).collect(),
        ),
    }
}

fn expr_output(expr: &Expr) -> SyntaxExprOutput {
    expr_output_with_span(expr, EbnfSourceSpan::default())
}

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
        Expr::Tuple(items) => expr_node(
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
        Expr::List(items) => expr_node(
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
        Expr::ListCons(head, tail) => expr_node(
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
        Expr::FixedArray(items) => expr_node(
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
        Expr::Index(value, index) => expr_node(
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
        } => expr_node(
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
        Expr::Map(fields) => expr_node(
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
            pattern,
            source,
            guard,
        } => {
            let mut children = vec![
                expr_output_with_span(expr, span),
                expr_output_with_span(source, span),
            ];
            if let Some(guard) = guard {
                children.push(expr_output_with_span(guard, span));
            }
            expr_node(
                SyntaxExprKind::ListComprehension,
                None,
                None,
                None,
                children,
                vec![pattern_output(pattern)],
                Vec::new(),
                Vec::new(),
                span,
            )
            .with_arity(3 + usize::from(guard.is_some()))
        }
        Expr::Let { bindings, body } => {
            let mut children = bindings
                .iter()
                .map(|binding| expr_output_with_span(&binding.value, span))
                .collect::<Vec<_>>();
            if let Some(body) = body {
                children.push(expr_output_with_span(body, span));
            }

            expr_node(
                SyntaxExprKind::Let,
                None,
                None,
                None,
                children,
                bindings
                    .iter()
                    .map(|binding| pattern_leaf(SyntaxPatternKind::Var, Some(binding.name.clone())))
                    .collect(),
                Vec::new(),
                Vec::new(),
                span,
            )
            .with_arity(bindings.len())
        }
        Expr::Call {
            callee,
            args,
            remote,
            is_fun_value,
        } => {
            let mut children = Vec::with_capacity(args.len() + 1);
            children.push(expr_output_with_span(callee, span));
            children.extend(args.iter().map(|arg| expr_output_with_span(arg, span)));
            expr_node(
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
            .with_arity(args.len())
        }
        Expr::Case { scrutinee, clauses } => expr_node(
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
            let mut output = expr_node(
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
        Expr::If { clauses } => expr_node(
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
        Expr::Fun { clauses } => expr_node(
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
        Expr::MacroCall { name, args } => expr_node(
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
        Expr::RawMacro { name, raw } => {
            let mut output = expr_node(
                SyntaxExprKind::RawMacro,
                Some(name.clone()),
                None,
                None,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                span,
            );
            output.raw = Some(raw.clone());
            output
        }
        Expr::HtmlBlock(block) => {
            let html_nodes = block.nodes.iter().map(html_node_output).collect::<Vec<_>>();
            let mut output = expr_node(
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
        Expr::RecordAccess { value, name, field } => expr_node(
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
        Expr::FieldAccess { value, field } => expr_node(
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
        } => expr_node(
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
        Expr::RecordConstruct { name, fields } => expr_node(
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
        Expr::TemplateInstantiate { name, fields } => expr_node(
            SyntaxExprKind::TemplateInstantiate,
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
        Expr::ConstructorChain { base, record } => expr_node(
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
        Expr::UnaryOp { op, expr } => expr_node(
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
        Expr::Cast { expr, target_type } => expr_node(
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
        Expr::BinaryOp { op, left, right } => expr_node(
            SyntaxExprKind::BinaryOp,
            None,
            Some(binary_op_text(op).to_string()),
            None,
            vec![
                expr_output_with_span(left, span),
                expr_output_with_span(right, span),
            ],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            span,
        ),
        Expr::Quote(inner) => expr_node(
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
        Expr::Unquote(inner) => expr_node(
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
        Expr::Sequence(expressions) => expr_node(
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

fn pattern_leaf(kind: SyntaxPatternKind, text: Option<String>) -> SyntaxPatternOutput {
    pattern_node(kind, text, Vec::new(), Vec::new())
}

fn pattern_node(
    kind: SyntaxPatternKind,
    text: Option<String>,
    children: Vec<SyntaxPatternOutput>,
    fields: Vec<SyntaxPatternFieldOutput>,
) -> SyntaxPatternOutput {
    SyntaxPatternOutput {
        kind,
        arity: if fields.is_empty() {
            children.len()
        } else {
            fields.len()
        },
        text,
        children,
        fields,
    }
}

fn pattern_field_output(field: &MapField) -> SyntaxPatternFieldOutput {
    SyntaxPatternFieldOutput {
        key: field.key.clone(),
        required: field.required,
        value: Box::new(pattern_output(&field.value)),
    }
}

fn expr_leaf_with_span(
    kind: SyntaxExprKind,
    text: Option<String>,
    span: EbnfSourceSpan,
) -> SyntaxExprOutput {
    expr_node(
        kind,
        text,
        None,
        None,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        span,
    )
}

/// Builds a leaf expression node while preserving raw source spelling.
///
/// Inputs:
/// - `kind`: syntax-output expression kind.
/// - `text`: normalized expression payload.
/// - `raw`: canonical source spelling that should survive the syntax boundary.
/// - `span`: source span for diagnostics.
///
/// Output:
/// - A `SyntaxExprOutput` leaf with no children and the supplied raw payload.
///
/// Transformation:
/// - Starts from the standard expression-node shape and overrides only `raw`
///   so downstream phases can distinguish explicit source forms that share the
///   same semantic kind.
fn expr_leaf_with_span_and_raw(
    kind: SyntaxExprKind,
    text: Option<String>,
    raw: Option<String>,
    span: EbnfSourceSpan,
) -> SyntaxExprOutput {
    let mut output = expr_leaf_with_span(kind, text, span);
    output.raw = raw;
    output
}

/// Renders the canonical raw syntax for an atom literal expression.
///
/// Inputs:
/// - `payload`: unescaped atom payload text.
///
/// Output:
/// - Canonical `Atom["..."]` source spelling.
///
/// Transformation:
/// - Escapes only the characters that need stable representation inside a
///   normal Terlan string literal.
fn format_canonical_atom_literal_raw(payload: &str) -> String {
    let escaped = payload
        .chars()
        .flat_map(|ch| match ch {
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '\n' => "\\n".chars().collect::<Vec<_>>(),
            '\r' => "\\r".chars().collect::<Vec<_>>(),
            '\t' => "\\t".chars().collect::<Vec<_>>(),
            other => vec![other],
        })
        .collect::<String>();
    format!("Atom[\"{escaped}\"]")
}

fn expr_node(
    kind: SyntaxExprKind,
    text: Option<String>,
    operator: Option<String>,
    remote: Option<String>,
    children: Vec<SyntaxExprOutput>,
    patterns: Vec<SyntaxPatternOutput>,
    fields: Vec<SyntaxExprFieldOutput>,
    clauses: Vec<SyntaxClauseOutput>,
    span: EbnfSourceSpan,
) -> SyntaxExprOutput {
    SyntaxExprOutput {
        kind,
        arity: node_arity(&children, &patterns, &fields, &clauses),
        text,
        span,
        raw: None,
        operator,
        remote,
        children,
        patterns,
        fields,
        clauses,
        catch_clauses: Vec::new(),
        try_after: None,
        html_nodes: Vec::new(),
    }
}

fn node_arity(
    children: &[SyntaxExprOutput],
    patterns: &[SyntaxPatternOutput],
    fields: &[SyntaxExprFieldOutput],
    clauses: &[SyntaxClauseOutput],
) -> usize {
    fields
        .len()
        .max(clauses.len())
        .max(patterns.len())
        .max(children.len())
}

trait SyntaxExprArity {
    fn with_arity(self, arity: usize) -> Self;
}

impl SyntaxExprArity for SyntaxExprOutput {
    fn with_arity(mut self, arity: usize) -> Self {
        self.arity = arity;
        self
    }
}

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

fn if_clause_output(clause: &IfClause, span: EbnfSourceSpan) -> SyntaxClauseOutput {
    SyntaxClauseOutput {
        patterns: Vec::new(),
        guard: Some(Box::new(expr_output_with_span(&clause.condition, span))),
        body: Box::new(expr_output_with_span(&clause.body, span)),
    }
}

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

fn expr_to_output_text(expr: &Expr) -> String {
    match expr {
        Expr::Int(value) => value.to_string(),
        Expr::Float(value) => value.to_string(),
        Expr::Atom(name) | Expr::AtomLiteral(name) | Expr::Var(name) => name.clone(),
        Expr::Binary(value) => value.clone(),
        Expr::Tuple(items) => format!(
            "{{{}}}",
            items
                .iter()
                .map(expr_to_output_text)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Expr::List(items) => format!(
            "[{}]",
            items
                .iter()
                .map(expr_to_output_text)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Expr::ListCons(head, tail) => format!(
            "[{} | {}]",
            expr_to_output_text(head),
            expr_to_output_text(tail)
        ),
        Expr::IndexAssign {
            collection,
            index,
            value,
        } => format!(
            "{}[{}] = {}",
            expr_to_output_text(collection),
            expr_to_output_text(index),
            expr_to_output_text(value)
        ),
        Expr::Let { bindings, body } => {
            let mut parts = bindings
                .iter()
                .map(|binding| {
                    format!("{} = {}", binding.name, expr_to_output_text(&binding.value))
                })
                .collect::<Vec<_>>();
            if let Some(body) = body {
                parts.push(expr_to_output_text(body));
            }
            format!("let {}", parts.join("; "))
        }
        Expr::Call {
            callee,
            args,
            remote,
            is_fun_value,
        } => {
            let args = args
                .iter()
                .map(expr_to_output_text)
                .collect::<Vec<_>>()
                .join(", ");
            match remote {
                Some(module) => format!("{}.{}({})", module, expr_to_output_text(callee), args),
                None if *is_fun_value => format!("{}.({})", expr_to_output_text(callee), args),
                None => format!("{}({})", expr_to_output_text(callee), args),
            }
        }
        Expr::FieldAccess { value, field } => {
            format!("{}.{}", expr_to_output_text(value), field)
        }
        Expr::TemplateInstantiate { name, fields } => {
            let body = fields
                .iter()
                .map(|field| format!("{} = {}", field.key, expr_to_output_text(&field.value)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{} {{{}}}", name, body)
        }
        Expr::ConstructorChain { base, record } => {
            format!(
                "{} with {}",
                expr_to_output_text(base),
                expr_to_output_text(record)
            )
        }
        Expr::UnaryOp { op, expr } => {
            format!("{} {}", unary_op_text(op), expr_to_output_text(expr))
        }
        Expr::Cast { expr, target_type } => {
            format!("{} as {}", expr_to_output_text(expr), target_type.text)
        }
        Expr::BinaryOp { op, left, right } => format!(
            "{} {} {}",
            expr_to_output_text(left),
            binary_op_text(op),
            expr_to_output_text(right)
        ),
        Expr::MacroCall { name, args } if args.is_empty() => format!("?{}", name),
        Expr::MacroCall { name, args } => format!(
            "?{}({})",
            name,
            args.iter()
                .map(expr_to_output_text)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Expr::RawMacro { name, raw } => format!("{} {{{}}}", name, raw),
        _ => "terlan_interface_constructor".to_string(),
    }
}

fn unary_op_text(op: &UnaryOp) -> &'static str {
    match op {
        UnaryOp::Neg => "-",
        UnaryOp::Not => "not",
        UnaryOp::Bang => "!",
    }
}

fn binary_op_text(op: &BinaryOp) -> &'static str {
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

fn is_constructor_pattern_tuple(items: &[Pattern]) -> bool {
    matches!(
        items.first(),
        Some(Pattern::Atom(name)) if name.chars().next().is_some_and(|ch| ch.is_ascii_uppercase())
    )
}

impl From<ImportKind> for SyntaxImportKind {
    fn from(kind: ImportKind) -> Self {
        match kind {
            ImportKind::Module => Self::Module,
            ImportKind::File => Self::File,
            ImportKind::Css => Self::Css,
            ImportKind::Markdown => Self::Markdown,
        }
    }
}

#[cfg(test)]
#[path = "syntax_output_decl_test.rs"]
mod syntax_output_decl_test;

#[cfg(test)]
#[path = "syntax_output_expr_test.rs"]
mod syntax_output_expr_test;

#[cfg(test)]
#[path = "syntax_output_test.rs"]
mod syntax_output_test;
