use serde::{Deserialize, Serialize};

use crate::{
    ast::{
        Annotation, BinaryOp, CaseClause, ConstructorClause, ConstructorParam, Decl, Expr,
        FunctionClause, FunctionDecl, HtmlAttr, HtmlAttrValue, HtmlElement, HtmlNamedSlot,
        HtmlNode, IfClause, ImportKind, MapExprField, MapField, Module, Param, Pattern,
        TraitMethodDecl, TypeExpr, UnaryOp,
    },
    ebnf::{EbnfCompileError, EbnfCompileResult, EbnfGrammarContract, EbnfSourceSpan},
    lexer::lex,
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
    pub span: EbnfSourceSpan,
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
    pub receive_after: Option<SyntaxReceiveAfterOutput>,
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
pub struct SyntaxReceiveAfterOutput {
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
    Map,
    ListComprehension,
    Let,
    Call,
    Case,
    Receive,
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
        .collect();

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

/// Converts parser annotation metadata into serializable syntax output.
///
/// Inputs:
/// - `annotation`: parsed declaration-leading annotation metadata.
///
/// Output:
/// - Syntax-output annotation payload with path, optional raw args, and span.
///
/// Transformation:
/// - Clones parser-owned annotation fields into the formal output schema so
///   downstream phases can inspect annotations without reading source text.
fn annotation_output(annotation: &Annotation) -> SyntaxAnnotationOutput {
    SyntaxAnnotationOutput {
        path: annotation.path.clone(),
        args: annotation.args.clone(),
        span: annotation.span.into(),
    }
}

fn declaration_payload(declaration: &Decl) -> SyntaxDeclarationPayload {
    match declaration {
        Decl::Import(decl) => SyntaxDeclarationPayload::Import {
            import_kind: decl.kind.into(),
            module_name: decl.module_name.clone(),
            items: decl
                .items
                .iter()
                .map(|item| SyntaxImportItem {
                    name: item.name.clone(),
                    as_alias: item.as_alias.clone(),
                    span: item.span.into(),
                })
                .collect(),
            is_type: decl.is_type,
            source_path: decl.source_path.clone(),
        },
        Decl::Export(decl) => SyntaxDeclarationPayload::Export {
            items: decl
                .items
                .iter()
                .map(|item| SyntaxExportItem {
                    name: item.name.clone(),
                    arity: item.arity,
                    span: item.span.into(),
                })
                .collect(),
        },
        Decl::Type(decl) => SyntaxDeclarationPayload::Type {
            name: decl.name.clone(),
            params: decl.params.clone(),
            is_public: decl.is_public,
            is_opaque: decl.is_opaque,
            implements: decl.implements.iter().map(type_output).collect(),
            variants: decl.variants.iter().map(type_output).collect(),
        },
        Decl::Struct(decl) => SyntaxDeclarationPayload::Struct {
            name: decl.name.clone(),
            derives: decl.derives.clone(),
            implements: decl.implements.iter().map(type_output).collect(),
            is_public: decl.is_public,
            fields: decl
                .fields
                .iter()
                .map(|field| SyntaxStructFieldOutput {
                    name: field.name.clone(),
                    annotation: type_output(&field.annotation),
                    docs: field.docs.clone(),
                    has_default: field.default.is_some(),
                    default: field
                        .default
                        .as_ref()
                        .map(|expr| expr_output_with_span(expr, field.span.into())),
                    span: field.span.into(),
                })
                .collect(),
        },
        Decl::Constructor(decl) => SyntaxDeclarationPayload::Constructor {
            name: decl.name.clone(),
            params: decl.params.clone(),
            is_public: decl.is_public,
            clauses: decl.clauses.iter().map(constructor_clause_output).collect(),
        },
        Decl::Function(decl) => SyntaxDeclarationPayload::Function {
            name: decl.name.clone(),
            params: decl.params.iter().map(param_output).collect(),
            return_type: type_output(&decl.return_type),
            is_public: decl.is_public,
            is_macro: decl.is_macro,
            generic_bounds: decl.generic_bounds.clone(),
            clauses: decl.clauses.iter().map(function_clause_output).collect(),
        },
        Decl::Method(decl) => SyntaxDeclarationPayload::Method {
            receiver: param_output(&decl.receiver),
            name: decl.name.clone(),
            params: decl.params.iter().map(param_output).collect(),
            return_type: type_output(&decl.return_type),
            is_public: decl.is_public,
            generic_bounds: decl.generic_bounds.clone(),
            clauses: decl.clauses.iter().map(function_clause_output).collect(),
        },
        Decl::Trait(decl) => SyntaxDeclarationPayload::Trait {
            name: decl.name.clone(),
            params: decl.params.clone(),
            super_traits: decl.super_traits.clone(),
            is_public: decl.is_public,
            methods: decl.methods.iter().map(trait_method_output).collect(),
        },
        Decl::TraitImpl(decl) => SyntaxDeclarationPayload::TraitImpl {
            trait_ref: type_output(&decl.trait_ref),
            for_type: type_output(&decl.for_type),
            is_public: decl.is_public,
            methods: decl.methods.iter().map(impl_method_output).collect(),
        },
        Decl::Template(decl) => SyntaxDeclarationPayload::Template {
            name: decl.name.clone(),
            source_path: decl.source_path.clone(),
            props: decl
                .props
                .iter()
                .map(|prop| SyntaxTemplatePropOutput {
                    name: prop.name.clone(),
                    annotation: type_output(&prop.annotation),
                    span: prop.span.into(),
                })
                .collect(),
        },
        Decl::Raw(decl) if is_config_declaration_kind(&decl.kind) => {
            SyntaxDeclarationPayload::Config {
                name: decl.kind.clone(),
                target: config_declaration_target(&decl.text),
                text: decl.text.clone(),
                entries: parse_config_entries(&decl.text),
            }
        }
        Decl::Raw(decl) => SyntaxDeclarationPayload::Raw {
            raw_kind: decl.kind.clone(),
            text: decl.text.clone(),
        },
    }
}

/// Returns whether a preserved raw declaration is a canonical config declaration.
///
/// Inputs:
/// - `kind`: parser-preserved declaration head.
///
/// Output:
/// - `true` when the declaration belongs to the EBNF `ConfigDecl` family.
///
/// Transformation:
/// - Keeps the parser AST unchanged while allowing syntax output to expose the
///   formal config declaration payload instead of a raw placeholder.
fn is_config_declaration_kind(kind: &str) -> bool {
    matches!(kind, "target" | "native" | "machine" | "static")
}

/// Extracts the config declaration target from preserved declaration text.
///
/// Inputs:
/// - `text`: parser-preserved config declaration text, such as
///   `target erlang` or `target js { module: true }`.
///
/// Output:
/// - The first target segment after the config declaration name, or an empty
///   string when preserved declaration text is malformed.
///
/// Transformation:
/// - Reads only the declaration head and leaves the full metadata body in
///   `text`, so later structured metadata parsing can replace this shim without
///   changing consumers that only need the target.
fn config_declaration_target(text: &str) -> String {
    text.split_whitespace()
        .nth(1)
        .map(|part| {
            part.trim_matches(|ch: char| matches!(ch, '{' | '}' | '.' | ',' | ';'))
                .to_string()
        })
        .unwrap_or_default()
}

/// Parses structured config entries from preserved config declaration text.
///
/// Inputs:
/// - `text`: parser-preserved config declaration text.
///
/// Output:
/// - Structured config entries when the text follows `ConfigDecl` metadata
///   block syntax.
/// - An empty entry list for empty blocks, blockless declarations, lexer
///   errors, or raw declarations outside the formal config shape.
///
/// Transformation:
/// - Re-lexes the preserved text, skips the config name and target path, and
///   parses a metadata block only when it appears immediately after the target.
///   This keeps target-specific semantics explicit while making the syntax
///   contract structured enough for validators and phase manifests.
fn parse_config_entries(text: &str) -> Vec<SyntaxConfigEntryOutput> {
    let Ok(tokens) = lex(text) else {
        return Vec::new();
    };
    let mut parser = ConfigEntryParser {
        tokens: &tokens,
        pos: 0,
    };
    parser.parse_entries().unwrap_or_default()
}

struct ConfigEntryParser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl ConfigEntryParser<'_> {
    /// Parses a complete config declaration into metadata entries.
    ///
    /// Inputs:
    /// - `self`: parser cursor over tokens from preserved config text.
    ///
    /// Output:
    /// - Parsed metadata entries, or `None` when the text is not the formal
    ///   `ConfigName ConfigTarget MetadataBlock` shape.
    ///
    /// Transformation:
    /// - Consumes the declaration head, target path, and immediate metadata
    ///   block. Blockless declarations and non-immediate blocks are treated as
    ///   entryless preserved config text.
    fn parse_entries(&mut self) -> Option<Vec<SyntaxConfigEntryOutput>> {
        self.expect_identifier()?;
        self.parse_config_path()?;
        if !self.consume(TokenKind::LBrace) {
            return Some(Vec::new());
        }
        self.parse_entry_list(TokenKind::RBrace, TokenKind::Semicolon)
    }

    /// Parses a dot-qualified config path.
    ///
    /// Inputs:
    /// - Cursor positioned at the first path segment.
    ///
    /// Output:
    /// - `Some(())` when at least one identifier segment was consumed.
    ///
    /// Transformation:
    /// - Consumes `LowerIdent { "." LowerIdent }` in the token stream used by
    ///   config targets and keys.
    fn parse_config_path(&mut self) -> Option<()> {
        self.expect_identifier()?;
        while self.consume(TokenKind::Dot) {
            self.expect_identifier()?;
        }
        Some(())
    }

    /// Parses metadata entries until a closing delimiter.
    ///
    /// Inputs:
    /// - `close`: token that terminates the current metadata container.
    /// - `separator`: token separating entries.
    ///
    /// Output:
    /// - Entry list when every entry parses and the closing token is present.
    ///
    /// Transformation:
    /// - Accepts optional trailing separators and delegates value parsing to
    ///   `parse_value`.
    fn parse_entry_list(
        &mut self,
        close: TokenKind,
        separator: TokenKind,
    ) -> Option<Vec<SyntaxConfigEntryOutput>> {
        let mut entries = Vec::new();
        if self.consume(close.clone()) {
            return Some(entries);
        }
        loop {
            entries.push(self.parse_entry()?);
            if self.consume(separator.clone()) {
                if self.consume(close.clone()) {
                    break;
                }
                continue;
            }
            self.expect(close.clone())?;
            break;
        }
        Some(entries)
    }

    /// Parses one key/value metadata entry.
    ///
    /// Inputs:
    /// - Cursor positioned at `ConfigKey`.
    ///
    /// Output:
    /// - Parsed entry with dotted key text and typed value.
    ///
    /// Transformation:
    /// - Reads the key, consumes `:`, and parses the value using the formal
    ///   config value grammar instead of Terlan runtime expression parsing.
    fn parse_entry(&mut self) -> Option<SyntaxConfigEntryOutput> {
        let key = self.parse_key()?;
        self.expect(TokenKind::Colon)?;
        let value = self.parse_value()?;
        Some(SyntaxConfigEntryOutput { key, value })
    }

    /// Parses a dotted config key.
    ///
    /// Inputs:
    /// - Cursor positioned at the first key segment.
    ///
    /// Output:
    /// - Dotted key string.
    ///
    /// Transformation:
    /// - Reconstructs `LowerIdent { "." LowerIdent }` without preserving
    ///   whitespace because config keys are semantic identifiers.
    fn parse_key(&mut self) -> Option<String> {
        let mut key = self.expect_identifier()?;
        while self.consume(TokenKind::Dot) {
            key.push('.');
            key.push_str(&self.expect_identifier()?);
        }
        Some(key)
    }

    /// Parses one typed config value.
    ///
    /// Inputs:
    /// - Cursor positioned at the value token.
    ///
    /// Output:
    /// - Structured config value when the token sequence matches the config
    ///   value grammar.
    ///
    /// Transformation:
    /// - Classifies booleans, symbols, numbers, strings, lists, and maps
    ///   independently from runtime Terlan expressions.
    fn parse_value(&mut self) -> Option<SyntaxConfigValueOutput> {
        let token = self.current()?;
        let kind = token.kind.clone();
        let text = token.text.clone();
        match kind {
            TokenKind::Atom if text == "true" => {
                self.pos += 1;
                Some(SyntaxConfigValueOutput::Bool { value: true })
            }
            TokenKind::Atom if text == "false" => {
                self.pos += 1;
                Some(SyntaxConfigValueOutput::Bool { value: false })
            }
            TokenKind::Atom => {
                self.pos += 1;
                Some(SyntaxConfigValueOutput::Symbol { value: text })
            }
            TokenKind::Int => {
                self.pos += 1;
                Some(SyntaxConfigValueOutput::Int { value: text })
            }
            TokenKind::Float => {
                self.pos += 1;
                Some(SyntaxConfigValueOutput::Float { value: text })
            }
            TokenKind::String => {
                self.pos += 1;
                Some(SyntaxConfigValueOutput::String { value: text })
            }
            TokenKind::LBracket => self.parse_list(),
            TokenKind::Hash => self.parse_map(),
            _ => None,
        }
    }

    /// Parses a config list value.
    ///
    /// Inputs:
    /// - Cursor positioned at `[`.
    ///
    /// Output:
    /// - Structured list value.
    ///
    /// Transformation:
    /// - Parses comma-separated config values and accepts an empty list or a
    ///   trailing comma.
    fn parse_list(&mut self) -> Option<SyntaxConfigValueOutput> {
        self.expect(TokenKind::LBracket)?;
        let mut values = Vec::new();
        if self.consume(TokenKind::RBracket) {
            return Some(SyntaxConfigValueOutput::List { values });
        }
        loop {
            values.push(self.parse_value()?);
            if self.consume(TokenKind::Comma) {
                if self.consume(TokenKind::RBracket) {
                    break;
                }
                continue;
            }
            self.expect(TokenKind::RBracket)?;
            break;
        }
        Some(SyntaxConfigValueOutput::List { values })
    }

    /// Parses a config map value.
    ///
    /// Inputs:
    /// - Cursor positioned at `#`.
    ///
    /// Output:
    /// - Structured map value.
    ///
    /// Transformation:
    /// - Consumes `#{ ... }` and parses comma-separated config map entries
    ///   using the same entry shape as top-level metadata blocks.
    fn parse_map(&mut self) -> Option<SyntaxConfigValueOutput> {
        self.expect(TokenKind::Hash)?;
        self.expect(TokenKind::LBrace)?;
        let entries = self.parse_entry_list(TokenKind::RBrace, TokenKind::Comma)?;
        Some(SyntaxConfigValueOutput::Map { entries })
    }

    /// Consumes the current token when it matches `kind`.
    ///
    /// Inputs:
    /// - `kind`: expected token kind.
    ///
    /// Output:
    /// - `true` if a token was consumed.
    ///
    /// Transformation:
    /// - Advances the parser cursor only for exact kind matches.
    fn consume(&mut self, kind: TokenKind) -> bool {
        if self.check(kind) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// Requires the current token to match `kind`.
    ///
    /// Inputs:
    /// - `kind`: required token kind.
    ///
    /// Output:
    /// - `Some(())` when the token matched and was consumed.
    ///
    /// Transformation:
    /// - Advances the cursor on success and returns `None` on mismatch.
    fn expect(&mut self, kind: TokenKind) -> Option<()> {
        self.consume(kind).then_some(())
    }

    /// Consumes one config identifier token.
    ///
    /// Inputs:
    /// - Cursor positioned at a possible lower identifier.
    ///
    /// Output:
    /// - Identifier text when the current token is a lower identifier.
    ///
    /// Transformation:
    /// - Accepts lexer `Atom` tokens only. Uppercase identifiers are excluded
    ///   from config paths and symbols to match the EBNF.
    fn expect_identifier(&mut self) -> Option<String> {
        let token = self.current()?;
        let kind = token.kind.clone();
        let text = token.text.clone();
        if kind == TokenKind::Atom {
            self.pos += 1;
            Some(text)
        } else {
            None
        }
    }

    /// Checks the current token kind.
    ///
    /// Inputs:
    /// - `kind`: token kind to compare.
    ///
    /// Output:
    /// - `true` when the current token has the requested kind.
    ///
    /// Transformation:
    /// - Reads without advancing the parser cursor.
    fn check(&self, kind: TokenKind) -> bool {
        self.current()
            .map(|token| token.kind == kind)
            .unwrap_or(false)
    }

    /// Returns the current token unless it is EOF.
    ///
    /// Inputs:
    /// - `self`: parser cursor.
    ///
    /// Output:
    /// - Current non-EOF token, or `None` at the end of the token stream.
    ///
    /// Transformation:
    /// - Treats EOF as absence so grammar helpers can use `Option` for
    ///   conservative parse failure.
    fn current(&self) -> Option<&Token> {
        self.tokens
            .get(self.pos)
            .filter(|token| token.kind != TokenKind::EOF)
    }
}

fn declaration_docs(declaration: &Decl) -> Vec<String> {
    match declaration {
        Decl::Type(decl) => decl.docs.clone(),
        Decl::Struct(decl) => decl.docs.clone(),
        Decl::Constructor(decl) => decl.docs.clone(),
        Decl::Function(decl) => decl.docs.clone(),
        Decl::Method(decl) => decl.docs.clone(),
        Decl::Trait(decl) => decl.docs.clone(),
        Decl::TraitImpl(decl) => decl.docs.clone(),
        Decl::Template(decl) => decl.docs.clone(),
        Decl::Raw(decl) => decl.docs.clone(),
        Decl::Import(_) | Decl::Export(_) => Vec::new(),
    }
}

fn type_output(ty: &TypeExpr) -> SyntaxTypeOutput {
    SyntaxTypeOutput {
        text: ty.text.clone(),
        span: ty.span.into(),
    }
}

fn param_output(param: &Param) -> SyntaxParamOutput {
    SyntaxParamOutput {
        name: param.name.clone(),
        annotation: type_output(&param.annotation),
        is_mutable: param.is_mutable,
        span: param.span.into(),
    }
}

fn constructor_param_output(param: &ConstructorParam) -> SyntaxConstructorParamOutput {
    let span: EbnfSourceSpan = param.span.into();
    SyntaxConstructorParamOutput {
        name: param.name.clone(),
        annotation: type_output(&param.annotation),
        has_default: param.default.is_some(),
        default: param
            .default
            .as_ref()
            .map(|expr| expr_output_with_span(expr, span)),
        is_varargs: param.is_varargs,
        span: param.span.into(),
    }
}

fn constructor_clause_output(clause: &ConstructorClause) -> SyntaxConstructorClauseOutput {
    let span: EbnfSourceSpan = clause.span.into();
    SyntaxConstructorClauseOutput {
        params: clause.params.iter().map(constructor_param_output).collect(),
        return_type: type_output(&clause.return_type),
        body: expr_output_with_span(&clause.body, span),
        body_text: expr_to_output_text(&clause.body),
        span: clause.span.into(),
    }
}

fn function_clause_output(clause: &FunctionClause) -> SyntaxFunctionClauseOutput {
    let span: EbnfSourceSpan = clause.span.into();
    SyntaxFunctionClauseOutput {
        patterns: clause.patterns.iter().map(pattern_output).collect(),
        guard: clause
            .guard
            .as_deref()
            .map(|expr| expr_output_with_span(expr, span)),
        body: expr_output_with_span(&clause.body, span),
        has_guard: clause.guard.is_some(),
        span: clause.span.into(),
    }
}

fn trait_method_output(method: &TraitMethodDecl) -> SyntaxTraitMethodOutput {
    let span: EbnfSourceSpan = method.span.into();
    SyntaxTraitMethodOutput {
        name: method.name.clone(),
        params: method.params.iter().map(param_output).collect(),
        return_type: type_output(&method.return_type),
        generic_bounds: method.generic_bounds.clone(),
        default_body: method
            .default_body
            .as_ref()
            .map(|expr| expr_output_with_span(expr, span)),
        is_public: method.is_public,
        docs: method.docs.clone(),
        span: method.span.into(),
    }
}

/// Converts an explicit conformance method into syntax-output form.
///
/// Inputs:
/// - `method`: function declaration parsed inside an `impl` block.
///
/// Output:
/// - Serializable method summary containing signature, clauses, and span.
///
/// Transformation:
/// - Reuses function clause output so impl methods preserve the same body shape
///   as normal declarations until trait conformance lowering resolves them.
fn impl_method_output(method: &FunctionDecl) -> SyntaxImplMethodOutput {
    SyntaxImplMethodOutput {
        name: method.name.clone(),
        params: method.params.iter().map(param_output).collect(),
        return_type: type_output(&method.return_type),
        generic_bounds: method.generic_bounds.clone(),
        clauses: method.clauses.iter().map(function_clause_output).collect(),
        span: method.span.into(),
    }
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
        Pattern::MapField(key, value, required) => pattern_node(
            SyntaxPatternKind::MapField,
            Some(key.clone()),
            vec![pattern_output(value)],
            vec![SyntaxPatternFieldOutput {
                key: key.clone(),
                required: *required,
                value: Box::new(pattern_output(value)),
            }],
        ),
        Pattern::Ignore => pattern_leaf(SyntaxPatternKind::Ignore, None),
        Pattern::Placeholder => pattern_leaf(SyntaxPatternKind::Placeholder, None),
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
        Expr::Receive {
            clauses,
            after_clause,
        } => {
            let mut output = expr_node(
                SyntaxExprKind::Receive,
                None,
                None,
                None,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                clauses
                    .iter()
                    .map(|clause| case_clause_output(clause, span))
                    .collect(),
                span,
            );
            output.receive_after = after_clause.as_ref().map(|after| SyntaxReceiveAfterOutput {
                trigger: Box::new(expr_output_with_span(&after.trigger, span)),
                body: Box::new(expr_output_with_span(&after.body, span)),
            });
            output
        }
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
        Expr::RemoteFunRef {
            module,
            function,
            arity,
        } => expr_node(
            SyntaxExprKind::RemoteFunRef,
            Some(function.clone()),
            None,
            Some(module.clone()),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            span,
        )
        .with_arity(*arity),
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
        receive_after: None,
        html_nodes: Vec::new(),
    }
}

fn html_node_output(node: &HtmlNode) -> SyntaxHtmlNodeOutput {
    match node {
        HtmlNode::Text(text) => SyntaxHtmlNodeOutput::Text { text: text.clone() },
        HtmlNode::Expr(expr) => SyntaxHtmlNodeOutput::Expr {
            expr: Box::new(expr_output_with_span(expr, EbnfSourceSpan::default())),
        },
        HtmlNode::Element(element) => SyntaxHtmlNodeOutput::Element {
            element: html_element_output(element),
        },
        HtmlNode::NamedSlot(slot) => SyntaxHtmlNodeOutput::NamedSlot {
            slot: html_named_slot_output(slot),
        },
    }
}

fn html_element_output(element: &HtmlElement) -> SyntaxHtmlElementOutput {
    SyntaxHtmlElementOutput {
        name: element.name.clone(),
        attrs: element.attrs.iter().map(html_attr_output).collect(),
        children: element.children.iter().map(html_node_output).collect(),
    }
}

fn html_named_slot_output(slot: &HtmlNamedSlot) -> SyntaxHtmlNamedSlotOutput {
    SyntaxHtmlNamedSlotOutput {
        name: slot.name.clone(),
        children: slot.children.iter().map(html_node_output).collect(),
    }
}

fn html_attr_output(attr: &HtmlAttr) -> SyntaxHtmlAttrOutput {
    SyntaxHtmlAttrOutput {
        name: attr.name.clone(),
        value: attr.value.as_ref().map(html_attr_value_output),
    }
}

fn html_attr_value_output(value: &HtmlAttrValue) -> SyntaxHtmlAttrValueOutput {
    match value {
        HtmlAttrValue::Text(text) => SyntaxHtmlAttrValueOutput::Text { text: text.clone() },
        HtmlAttrValue::Expr(expr) => SyntaxHtmlAttrValueOutput::Expr {
            expr: Box::new(expr_output_with_span(expr, EbnfSourceSpan::default())),
        },
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
        Expr::Atom(name) | Expr::Var(name) => name.clone(),
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
        Expr::RemoteFunRef {
            module,
            function,
            arity,
        } => {
            format!("fun {}:{}/{}", module, function, arity)
        }
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
        BinaryOp::Eq => "=",
        BinaryOp::EqEq => "==",
        BinaryOp::EqEqEq => "==",
        BinaryOp::NotEq => "!=",
        BinaryOp::NotEqEq => "!=",
        BinaryOp::Lt => "<",
        BinaryOp::Gt => ">",
        BinaryOp::LtEq => "<=",
        BinaryOp::GtEq => ">=",
        BinaryOp::DivRem => "div",
        BinaryOp::Rem => "rem",
        BinaryOp::And => "and",
        BinaryOp::Or => "or",
        BinaryOp::PipeForward => "|>",
        BinaryOp::Send => "!",
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
mod tests {
    use super::*;

    #[test]
    fn module_syntax_output_wraps_ebnf_contract_and_metadata() {
        let output = parse_module_as_syntax_output(
            r#"
            module demo.

            import lib.Mod.
            type Item = Int.
            pub add(X: Int): Int -> X + 1.
            "#,
        )
        .expect("syntax output");

        assert_eq!(output.schema, SYNTAX_MODULE_OUTPUT_SCHEMA);
        assert_eq!(output.source_kind, SyntaxSourceKind::Module);
        assert_eq!(output.module_name, "demo");
        assert_eq!(output.contract.entry_rule.as_deref(), Some("Program"));
        assert_eq!(output.declarations.len(), 3);
        assert_eq!(output.declarations[0].class, "ImportDecl");
        assert_eq!(output.declarations[1].class, "TypeDecl");
        assert_eq!(output.declarations[2].class, "FunctionDecl");
        match &output.declarations[0].payload {
            SyntaxDeclarationPayload::Import {
                import_kind,
                module_name,
                ..
            } => {
                assert_eq!(*import_kind, SyntaxImportKind::Module);
                assert_eq!(module_name, "lib");
            }
            other => panic!("unexpected import payload: {other:?}"),
        }
        match &output.declarations[1].payload {
            SyntaxDeclarationPayload::Type {
                name,
                is_public,
                is_opaque,
                variants,
                ..
            } => {
                assert_eq!(name, "Item");
                assert!(!is_public);
                assert!(!is_opaque);
                assert_eq!(variants.len(), 1);
                assert_eq!(variants[0].text, "Int");
            }
            other => panic!("unexpected type payload: {other:?}"),
        }
        match &output.declarations[2].payload {
            SyntaxDeclarationPayload::Function {
                name,
                params,
                return_type,
                is_public,
                is_macro,
                clauses,
                ..
            } => {
                assert_eq!(name, "add");
                assert_eq!(params.len(), 1);
                assert_eq!(params[0].name, "X");
                assert_eq!(params[0].annotation.text, "Int");
                assert_eq!(return_type.text, "Int");
                assert!(*is_public);
                assert!(!is_macro);
                assert_eq!(clauses.len(), 1);
                assert_eq!(clauses[0].patterns.len(), 1);
                assert_eq!(clauses[0].patterns[0].kind, SyntaxPatternKind::Var);
                assert_eq!(clauses[0].patterns[0].text.as_deref(), Some("X"));
                assert_eq!(clauses[0].body.kind, SyntaxExprKind::BinaryOp);
                assert_eq!(clauses[0].body.operator.as_deref(), Some("+"));
                assert_eq!(clauses[0].body.children.len(), 2);
                assert_eq!(clauses[0].body.children[0].text.as_deref(), Some("X"));
                assert_eq!(clauses[0].body.children[1].text.as_deref(), Some("1"));
                assert!(!clauses[0].has_guard);
                assert!(clauses[0].guard.is_none());
            }
            other => panic!("unexpected function payload: {other:?}"),
        }
        assert!(output.syntax_contract.fingerprint.starts_with("fnv1a64:"));

        let raw = serde_json::to_string(&output).expect("serialize syntax output");
        let decoded =
            serde_json::from_str::<SyntaxModuleOutput>(&raw).expect("deserialize syntax output");
        assert_eq!(decoded, output);
    }

    /// Verifies declaration annotations are preserved in syntax output.
    ///
    /// Inputs:
    /// - A module with one path-only annotation and one metadata-block
    ///   annotation before declarations.
    ///
    /// Output:
    /// - Assertions over `SyntaxDeclarationOutput.annotations`.
    ///
    /// Transformation:
    /// - Parses source through `parse_module_as_syntax_output` and confirms
    ///   parser annotation metadata is serialized beside the routed
    ///   declarations.
    #[test]
    fn syntax_output_preserves_declaration_annotations() {
        let output = parse_module_as_syntax_output(
            r#"
            module annotation_output.

            @compiler.inline
            type Tagged = :tagged.

            @target.erlang {
              otp_application: true
            }
            run(): Int -> 1.
            "#,
        )
        .expect("annotation syntax output");

        assert_eq!(output.declarations.len(), 2);
        let type_annotations = &output.declarations[0].annotations;
        assert_eq!(type_annotations.len(), 1);
        assert_eq!(type_annotations[0].path, vec!["compiler", "inline"]);
        assert!(type_annotations[0].args.is_none());

        let function_annotations = &output.declarations[1].annotations;
        assert_eq!(function_annotations.len(), 1);
        assert_eq!(function_annotations[0].path, vec!["target", "erlang"]);
        let args = function_annotations[0]
            .args
            .as_deref()
            .expect("annotation args");
        assert!(args.starts_with('{'));
        assert!(args.ends_with('}'));
        assert!(args.contains("otp_application"));
        assert!(args.contains("true"));
    }

    /// Verifies receiver methods are emitted as formal method declarations.
    ///
    /// Inputs:
    /// - A module containing a receiver-style method declaration.
    ///
    /// Output:
    /// - Assertions over declaration class, receiver metadata, method params,
    ///   return type, and body payload.
    ///
    /// Transformation:
    /// - Parses source through `parse_module_as_syntax_output` and confirms
    ///   receiver-method syntax is no longer downgraded to raw output.
    #[test]
    fn syntax_output_preserves_receiver_methods_as_method_decls() {
        let output = parse_module_as_syntax_output(
            r#"
            module method_output.

            (self: User) identity(): User -> self.
            "#,
        )
        .expect("method syntax output");

        assert_eq!(output.declarations.len(), 1);
        assert_eq!(output.declarations[0].class, "MethodDecl");
        match &output.declarations[0].payload {
            SyntaxDeclarationPayload::Method {
                receiver,
                name,
                params,
                return_type,
                clauses,
                ..
            } => {
                assert_eq!(receiver.name, "self");
                assert_eq!(receiver.annotation.text, "User");
                assert!(!receiver.is_mutable);
                assert_eq!(name, "identity");
                assert!(params.is_empty());
                assert_eq!(return_type.text, "User");
                assert_eq!(clauses.len(), 1);
                assert_eq!(clauses[0].body.kind, SyntaxExprKind::Var);
            }
            other => panic!("unexpected method payload: {other:?}"),
        }
    }

    /// Verifies mutable receiver metadata survives syntax output.
    ///
    /// Inputs:
    /// - A module containing a receiver method declared with contextual `mut`.
    ///
    /// Output:
    /// - Assertions over the method payload showing `receiver.mutable = true`.
    ///
    /// Transformation:
    /// - Parses source through syntax output and preserves the receiver
    ///   mutability marker without lowering or resolving its semantics.
    #[test]
    fn syntax_output_preserves_mutable_receiver_marker() {
        let output = parse_module_as_syntax_output(
            r#"
            module method_output_mutable.

            pub (mut self: User) rename(name: String): User -> self.
            "#,
        )
        .expect("method syntax output");

        match &output.declarations[0].payload {
            SyntaxDeclarationPayload::Method { receiver, name, .. } => {
                assert_eq!(name, "rename");
                assert_eq!(receiver.name, "self");
                assert_eq!(receiver.annotation.text, "User");
                assert!(receiver.is_mutable);
            }
            other => panic!("unexpected method payload: {other:?}"),
        }
    }

    /// Verifies release core collection contracts survive formal syntax output.
    ///
    /// Inputs:
    /// - Release source contracts for `std.collections.Map`, `std.collections.List`, and
    ///   `std.collections.Set`.
    ///
    /// Output:
    /// - Test passes when the formal syntax-output boundary preserves each
    ///   collection module name and each mutable receiver method required by
    ///   the P0.3 contract.
    ///
    /// Transformation:
    /// - Parses release contracts through `parse_module_as_syntax_output`,
    ///   filters method declarations, and checks receiver mutability without
    ///   typechecking or backend lowering.
    #[test]
    fn syntax_output_preserves_release_core_collection_contracts() {
        let contracts = [
            (
                "std.collections.Map",
                include_str!("../../../std/collections/map.tl"),
                vec![
                    ("put", "map", "Map[K, V]", true),
                    ("remove", "map", "Map[K, V]", true),
                    ("clear", "map", "Map[K, V]", true),
                ],
            ),
            (
                "std.collections.List",
                include_str!("../../../std/collections/list.tl"),
                vec![
                    ("push", "list", "List[T]", true),
                    ("clear", "list", "List[T]", true),
                ],
            ),
            (
                "std.collections.Set",
                include_str!("../../../std/collections/set.tl"),
                vec![
                    ("add", "set", "Set[T]", true),
                    ("remove", "set", "Set[T]", true),
                    ("clear", "set", "Set[T]", true),
                ],
            ),
        ];

        for (expected_module, source, expected_methods) in contracts {
            let output = parse_module_as_syntax_output(source)
                .expect("syntax output release collection contract");
            assert_eq!(output.module_name, expected_module);

            for (expected_name, expected_receiver_name, expected_receiver_type, is_mutable) in
                expected_methods
            {
                let method = output
                    .declarations
                    .iter()
                    .find_map(|declaration| match &declaration.payload {
                        SyntaxDeclarationPayload::Method { name, receiver, .. }
                            if name == expected_name =>
                        {
                            Some(receiver)
                        }
                        _ => None,
                    })
                    .unwrap_or_else(|| {
                        panic!("missing method `{expected_name}` in {expected_module}")
                    });

                assert_eq!(method.name, expected_receiver_name);
                assert_eq!(method.annotation.text, expected_receiver_type);
                assert_eq!(method.is_mutable, is_mutable);
            }
        }
    }

    /// Verifies release iterator/iterable contracts survive syntax output.
    ///
    /// Inputs:
    /// - Release interface contracts for `std.collections.Iterator` and
    ///   `std.collections.Iterable`.
    ///
    /// Output:
    /// - Test passes when syntax output preserves `Iterator.next` as a
    ///   function signature and `Iterable.iterator` as a trait method
    ///   signature.
    ///
    /// Transformation:
    /// - Parses release contracts through interface syntax output and inspects
    ///   structured declarations without typechecking or backend lowering.
    #[test]
    fn syntax_output_preserves_release_traversal_contracts() {
        let iterator_output =
            parse_module_as_syntax_output(include_str!("../../../std/collections/iterator.tl"))
                .expect("syntax output iterator contract");
        assert_eq!(iterator_output.module_name, "std.collections.Iterator");
        let iterator_function_shapes: Vec<String> = iterator_output
            .declarations
            .iter()
            .filter_map(|declaration| match &declaration.payload {
                SyntaxDeclarationPayload::Function {
                    name,
                    params,
                    return_type,
                    ..
                } => Some(format!(
                    "{}({}) -> {}",
                    name,
                    params
                        .iter()
                        .map(|param| param.annotation.text.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    return_type.text
                )),
                _ => None,
            })
            .collect();
        assert!(
            iterator_output.declarations.iter().any(|declaration| {
                matches!(
                    &declaration.payload,
                    SyntaxDeclarationPayload::Function {
                        name,
                        params,
                        return_type,
                        ..
                    } if name == "next"
                        && params.len() == 1
                        && params[0].annotation.text == "Iterator[T]"
                        && return_type.text == "Option[Step[T]]"
                )
            }),
            "iterator function shapes: {iterator_function_shapes:?}"
        );

        let iterable_output =
            parse_module_as_syntax_output(include_str!("../../../std/collections/iterable.tl"))
                .expect("syntax output iterable contract");
        assert_eq!(iterable_output.module_name, "std.collections.Iterable");
        let trait_decl = iterable_output
            .declarations
            .iter()
            .find_map(|declaration| match &declaration.payload {
                SyntaxDeclarationPayload::Trait { name, methods, .. } if name == "Iterable" => {
                    Some(methods)
                }
                _ => None,
            })
            .expect("Iterable trait declaration");
        assert!(trait_decl.iter().any(|method| {
            method.name == "iterator"
                && method.params.len() == 1
                && method.params[0].annotation.text == "C"
                && method.return_type.text == "Iterator[T]"
        }));
    }

    /// Verifies canonical config declarations are exposed as structured syntax
    /// output instead of raw declarations.
    ///
    /// Inputs:
    /// - A module containing parser-preserved target config syntax.
    ///
    /// Outputs:
    /// - A `ConfigDecl` declaration class and `Config` payload with target text.
    ///
    /// Transformation:
    /// - Parses through the existing raw parser branch, then normalizes the
    ///   syntax-output payload to match the EBNF `ConfigDecl` contract.
    #[test]
    fn syntax_output_normalizes_config_declarations() {
        let output = parse_module_as_syntax_output(
            r#"
            module config_output.

            target erlang {
              otp_application: true;
              adapter: postgres;
              features: [sockets, ssl];
              options: #{ssl: false, retries: 3}
            }.
            "#,
        )
        .expect("config syntax output");

        assert_eq!(output.declarations.len(), 1);
        assert_eq!(output.declarations[0].class, "ConfigDecl");
        match &output.declarations[0].payload {
            SyntaxDeclarationPayload::Config {
                name,
                target,
                text,
                entries,
            } => {
                assert_eq!(name, "target");
                assert_eq!(target, "erlang");
                assert!(text.starts_with("target erlang {"));
                assert_eq!(entries.len(), 4);
                assert_eq!(entries[0].key, "otp_application");
                assert_eq!(
                    entries[0].value,
                    SyntaxConfigValueOutput::Bool { value: true }
                );
                assert_eq!(entries[1].key, "adapter");
                assert_eq!(
                    entries[1].value,
                    SyntaxConfigValueOutput::Symbol {
                        value: "postgres".to_string()
                    }
                );
                assert_eq!(entries[2].key, "features");
                assert_eq!(
                    entries[2].value,
                    SyntaxConfigValueOutput::List {
                        values: vec![
                            SyntaxConfigValueOutput::Symbol {
                                value: "sockets".to_string()
                            },
                            SyntaxConfigValueOutput::Symbol {
                                value: "ssl".to_string()
                            }
                        ]
                    }
                );
                assert_eq!(entries[3].key, "options");
                assert_eq!(
                    entries[3].value,
                    SyntaxConfigValueOutput::Map {
                        entries: vec![
                            SyntaxConfigEntryOutput {
                                key: "ssl".to_string(),
                                value: SyntaxConfigValueOutput::Bool { value: false },
                            },
                            SyntaxConfigEntryOutput {
                                key: "retries".to_string(),
                                value: SyntaxConfigValueOutput::Int {
                                    value: "3".to_string()
                                },
                            }
                        ]
                    }
                );
            }
            other => panic!("unexpected config payload: {other:?}"),
        }
    }

    #[test]
    fn interface_syntax_output_marks_source_kind() {
        let output = parse_interface_module_as_syntax_output(
            r#"
            module demo.

            export demo/1.
            "#,
        )
        .expect("interface syntax output");

        assert_eq!(output.source_kind, SyntaxSourceKind::Interface);
        assert_eq!(output.declarations.len(), 1);
        assert_eq!(output.declarations[0].class, "ExportDecl");
        match &output.declarations[0].payload {
            SyntaxDeclarationPayload::Export { items } => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].name, "demo");
                assert_eq!(items[0].arity, 1);
            }
            other => panic!("unexpected export payload: {other:?}"),
        }
    }

    #[test]
    fn syntax_output_includes_recursive_expression_and_pattern_trees() {
        let output = parse_module_as_syntax_output(
            r#"
            module recursive.

            pick(Value: Int): Int ->
                case Value {
                    {:ok, value} -> value;
                    _ -> 0
                }.
            "#,
        )
        .expect("syntax output");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let body = &clauses[0].body;
        assert_eq!(body.kind, SyntaxExprKind::Case);
        assert_eq!(body.children.len(), 1);
        assert_eq!(body.children[0].kind, SyntaxExprKind::Var);
        assert_eq!(body.children[0].text.as_deref(), Some("Value"));
        assert_eq!(body.clauses.len(), 2);

        let first_pattern = &body.clauses[0].patterns[0];
        assert_eq!(first_pattern.kind, SyntaxPatternKind::Tuple);
        assert_eq!(first_pattern.children.len(), 2);
        assert_eq!(first_pattern.children[0].kind, SyntaxPatternKind::Atom);
        assert_eq!(first_pattern.children[0].text.as_deref(), Some("ok"));
        assert_eq!(first_pattern.children[1].kind, SyntaxPatternKind::Var);
        assert_eq!(first_pattern.children[1].text.as_deref(), Some("value"));
        assert_eq!(body.clauses[0].body.kind, SyntaxExprKind::Var);
        assert_eq!(body.clauses[0].body.text.as_deref(), Some("value"));

        assert_eq!(
            body.clauses[1].patterns[0].kind,
            SyntaxPatternKind::Wildcard
        );
        assert_eq!(body.clauses[1].body.kind, SyntaxExprKind::Int);
        assert_eq!(body.clauses[1].body.text.as_deref(), Some("0"));
    }

    /// Verifies syntax output preserves explicit cast expressions.
    ///
    /// Inputs:
    /// - A source expression using `value as Option[String]`.
    ///
    /// Output:
    /// - Test passes when syntax output exposes `kind: cast`, `operator: as`,
    ///   the target type text, and the casted child expression.
    ///
    /// Transformation:
    /// - Parses the expression through the public syntax-output entry point
    ///   and inspects the compiler-facing serialized expression shape.
    #[test]
    fn syntax_output_preserves_cast_expression_shape() {
        let output =
            parse_expr_as_syntax_output("value as Option[String]").expect("cast syntax output");

        assert_eq!(output.kind, SyntaxExprKind::Cast);
        assert_eq!(output.operator.as_deref(), Some("as"));
        assert_eq!(output.text.as_deref(), Some("Option[String]"));
        assert_eq!(output.children.len(), 1);
        assert_eq!(output.children[0].kind, SyntaxExprKind::Var);
        assert_eq!(output.children[0].text.as_deref(), Some("value"));
    }

    #[test]
    fn syntax_output_includes_case_guard_trees() {
        let output = parse_module_as_syntax_output(
            r#"
            module guarded_case.

            pick(value: Int): Int ->
                case value {
                    x when x > 0 -> x;
                    _ -> 0
                }.
            "#,
        )
        .expect("syntax output guarded case");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let body = &clauses[0].body;
        assert_eq!(body.kind, SyntaxExprKind::Case);
        assert_eq!(body.clauses.len(), 2);

        let first_clause = &body.clauses[0];
        let guard = first_clause.guard.as_ref().expect("case guard tree");
        assert_eq!(guard.kind, SyntaxExprKind::BinaryOp);
        assert_eq!(guard.operator.as_deref(), Some(">"));
        assert_eq!(guard.children.len(), 2);
        assert_eq!(guard.children[0].kind, SyntaxExprKind::Var);
        assert_eq!(guard.children[0].text.as_deref(), Some("x"));
        assert_eq!(guard.children[1].kind, SyntaxExprKind::Int);
        assert_eq!(guard.children[1].text.as_deref(), Some("0"));

        assert!(body.clauses[1].guard.is_none());
    }

    #[test]
    fn syntax_output_includes_function_clause_guard_trees() {
        let output = parse_module_as_syntax_output(
            r#"
            module guarded_function.

            pick(value) when value > 0 -> value;
            pick(_) -> 0.
            "#,
        )
        .expect("syntax output guarded function");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        assert_eq!(clauses.len(), 2);
        assert!(clauses[0].has_guard);
        let guard = clauses[0].guard.as_ref().expect("function guard tree");
        assert_eq!(guard.kind, SyntaxExprKind::BinaryOp);
        assert_eq!(guard.operator.as_deref(), Some(">"));
        assert_eq!(guard.children.len(), 2);
        assert_eq!(guard.children[0].kind, SyntaxExprKind::Var);
        assert_eq!(guard.children[0].text.as_deref(), Some("value"));
        assert_eq!(guard.children[1].kind, SyntaxExprKind::Int);
        assert_eq!(guard.children[1].text.as_deref(), Some("0"));

        assert!(!clauses[1].has_guard);
        assert!(clauses[1].guard.is_none());
    }

    #[test]
    fn syntax_output_preserves_expression_precedence_tree() {
        let output = parse_module_as_syntax_output(
            r#"
            module precedence_tree.

            demo(a: Int, b: Int, c: Int, pid: Pid): Dynamic ->
                a + b * c |> inspect() ! pid.
            "#,
        )
        .expect("syntax output precedence tree");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let send = &clauses[0].body;
        assert_eq!(send.kind, SyntaxExprKind::BinaryOp);
        assert_eq!(send.operator.as_deref(), Some("!"));
        assert_eq!(send.children.len(), 2);

        let pipe = &send.children[0];
        assert_eq!(pipe.kind, SyntaxExprKind::BinaryOp);
        assert_eq!(pipe.operator.as_deref(), Some("|>"));
        assert_eq!(pipe.children.len(), 2);

        let add = &pipe.children[0];
        assert_eq!(add.kind, SyntaxExprKind::BinaryOp);
        assert_eq!(add.operator.as_deref(), Some("+"));
        assert_eq!(add.children.len(), 2);
        assert_eq!(add.children[0].kind, SyntaxExprKind::Var);
        assert_eq!(add.children[0].text.as_deref(), Some("a"));

        let mul = &add.children[1];
        assert_eq!(mul.kind, SyntaxExprKind::BinaryOp);
        assert_eq!(mul.operator.as_deref(), Some("*"));
        assert_eq!(mul.children.len(), 2);
        assert_eq!(mul.children[0].kind, SyntaxExprKind::Var);
        assert_eq!(mul.children[0].text.as_deref(), Some("b"));
        assert_eq!(mul.children[1].kind, SyntaxExprKind::Var);
        assert_eq!(mul.children[1].text.as_deref(), Some("c"));

        assert_eq!(pipe.children[1].kind, SyntaxExprKind::Call);
        assert_eq!(send.children[1].kind, SyntaxExprKind::Var);
        assert_eq!(send.children[1].text.as_deref(), Some("pid"));
    }

    /// Verifies that boolean operators are preserved in formal syntax output.
    ///
    /// Inputs:
    /// - A module whose function body combines pipe, `or`, `and`, comparison,
    ///   and arithmetic operators.
    ///
    /// Output:
    /// - Test passes when syntax output carries `or` and `and` as binary
    ///   operator nodes in canonical precedence order.
    ///
    /// Transformation:
    /// - Parses source to `SyntaxModuleOutput` and inspects the nested
    ///   expression tree used by the formal compiler path.
    #[test]
    fn syntax_output_preserves_boolean_expression_precedence_tree() {
        let output = parse_module_as_syntax_output(
            r#"
            module boolean_precedence_tree.

            demo(a: Bool, b: Bool, c: Bool, d: Int, e: Int): Dynamic ->
                a |> inspect() or b and c == d + e.
            "#,
        )
        .expect("syntax output boolean precedence tree");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let pipe = &clauses[0].body;
        assert_eq!(pipe.kind, SyntaxExprKind::BinaryOp);
        assert_eq!(pipe.operator.as_deref(), Some("|>"));

        let or_expr = &pipe.children[1];
        assert_eq!(or_expr.kind, SyntaxExprKind::BinaryOp);
        assert_eq!(or_expr.operator.as_deref(), Some("or"));

        let and_expr = &or_expr.children[1];
        assert_eq!(and_expr.kind, SyntaxExprKind::BinaryOp);
        assert_eq!(and_expr.operator.as_deref(), Some("and"));

        let cmp = &and_expr.children[1];
        assert_eq!(cmp.kind, SyntaxExprKind::BinaryOp);
        assert_eq!(cmp.operator.as_deref(), Some("=="));

        let add = &cmp.children[1];
        assert_eq!(add.kind, SyntaxExprKind::BinaryOp);
        assert_eq!(add.operator.as_deref(), Some("+"));
    }

    /// Verifies local `let` expressions preserve binding order and explicit
    /// body shape.
    ///
    /// Inputs:
    /// - A module with two explicit-body `let` expressions.
    ///
    /// Output:
    /// - Test passes when binding names are preserved in `patterns`, binding
    ///   values are preserved in leading `children`, and an explicit body is
    ///   represented as the final child.
    ///
    /// Transformation:
    /// - Parses source through syntax output and inspects the formal tree
    ///   shape used by typecheck/CoreIR lowering.
    #[test]
    fn syntax_output_preserves_let_expression_tree() {
        let output = parse_module_as_syntax_output(
            r#"
            module let_tree.

            with_body(x: Int): Int ->
                let y = x + 1; z = y * 2; z + y.

            final_value(x: Int): Int ->
                let y = x + 1; z = y * 2; z.
            "#,
        )
        .expect("syntax output let tree");

        let SyntaxDeclarationPayload::Function {
            clauses: with_body_clauses,
            ..
        } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };
        let with_body = &with_body_clauses[0].body;
        assert_eq!(with_body.kind, SyntaxExprKind::Let);
        assert_eq!(with_body.arity, 2);
        assert_eq!(with_body.patterns.len(), 2);
        assert_eq!(with_body.patterns[0].text.as_deref(), Some("y"));
        assert_eq!(with_body.patterns[1].text.as_deref(), Some("z"));
        assert_eq!(with_body.children.len(), 3);
        assert_eq!(with_body.children[2].kind, SyntaxExprKind::BinaryOp);
        assert_eq!(with_body.children[2].operator.as_deref(), Some("+"));

        let SyntaxDeclarationPayload::Function {
            clauses: final_value_clauses,
            ..
        } = &output.declarations[1].payload
        else {
            panic!("expected function declaration");
        };
        let final_value = &final_value_clauses[0].body;
        assert_eq!(final_value.kind, SyntaxExprKind::Let);
        assert_eq!(final_value.arity, 2);
        assert_eq!(final_value.patterns.len(), 2);
        assert_eq!(final_value.children.len(), 3);
        assert_eq!(final_value.patterns[1].text.as_deref(), Some("z"));
        assert_eq!(final_value.children[2].kind, SyntaxExprKind::Var);
        assert_eq!(final_value.children[2].text.as_deref(), Some("z"));
    }

    #[test]
    fn syntax_output_preserves_unary_expression_precedence_tree() {
        let output = parse_module_as_syntax_output(
            r#"
            module unary_precedence_tree.

            demo(ready: Bool, value: Int, scale: Int): Bool ->
                not ready == (-value * scale).
            "#,
        )
        .expect("syntax output unary precedence tree");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let cmp = &clauses[0].body;
        assert_eq!(cmp.kind, SyntaxExprKind::BinaryOp);
        assert_eq!(cmp.operator.as_deref(), Some("=="));

        let not_expr = &cmp.children[0];
        assert_eq!(not_expr.kind, SyntaxExprKind::UnaryOp);
        assert_eq!(not_expr.operator.as_deref(), Some("not"));
        assert_eq!(not_expr.children[0].kind, SyntaxExprKind::Var);

        let mul = &cmp.children[1];
        assert_eq!(mul.kind, SyntaxExprKind::BinaryOp);
        assert_eq!(mul.operator.as_deref(), Some("*"));
        assert_eq!(mul.children[0].kind, SyntaxExprKind::UnaryOp);
        assert_eq!(mul.children[0].operator.as_deref(), Some("-"));
    }

    #[test]
    fn syntax_output_rejects_remote_fun_ref_source_syntax() {
        let error = parse_module_as_syntax_output(
            r#"
            module remote_fun_ref_tree.

            demo(): Dynamic ->
                fun math:double/1.
            "#,
        )
        .expect_err("remote fun refs are not canonical source syntax");

        let message = format!("{error:?}");
        assert!(
            message.contains("unexpected tokens after expression") || message.contains("expected"),
            "unexpected diagnostic: {message}"
        );
    }

    #[test]
    fn syntax_output_includes_colon_remote_call_trees() {
        let output = parse_module_as_syntax_output(
            r#"
            module remote_call_tree.

            demo(): Dynamic ->
                io_lib:format("~p", []).
            "#,
        )
        .expect("syntax output colon remote call");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let body = &clauses[0].body;
        assert_eq!(body.kind, SyntaxExprKind::Call);
        assert_eq!(body.remote.as_deref(), Some("io_lib"));
        assert_eq!(body.children[0].kind, SyntaxExprKind::Atom);
        assert_eq!(body.children[0].text.as_deref(), Some("format"));
        assert_eq!(body.children.len(), 3);
    }

    /// Verifies function-value invocation uses expression-call syntax output.
    ///
    /// Inputs:
    /// - A module containing `f.(10, 20)` in function body position.
    ///
    /// Output:
    /// - Test passes when syntax output records a call whose callee child is the
    ///   value expression `f`, not a remote call or constructor candidate.
    ///
    /// Transformation:
    /// - Parses source through `parse_module_as_syntax_output` and inspects the
    ///   emitted `SyntaxExprKind::Call` children and remote marker.
    #[test]
    fn syntax_output_includes_function_value_invocation_trees() {
        let output = parse_module_as_syntax_output(
            r#"
            module function_value_invocation.

            invoke(f: Dynamic): Dynamic ->
                f.(10, 20).
            "#,
        )
        .expect("syntax output function-value invocation");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let body = &clauses[0].body;
        assert_eq!(body.kind, SyntaxExprKind::FunctionCall);
        assert_eq!(body.remote, None);
        assert_eq!(body.children.len(), 3);
        assert_eq!(body.children[0].kind, SyntaxExprKind::Var);
        assert_eq!(body.children[0].text.as_deref(), Some("f"));
        assert_eq!(body.children[1].kind, SyntaxExprKind::Int);
        assert_eq!(body.children[1].text.as_deref(), Some("10"));
        assert_eq!(body.children[2].kind, SyntaxExprKind::Int);
        assert_eq!(body.children[2].text.as_deref(), Some("20"));
    }

    /// Verifies receiver method calls are syntax-output calls over field access.
    ///
    /// Inputs:
    /// - A module containing `user.display_name("short")` in function body
    ///   position.
    ///
    /// Output:
    /// - Test passes when syntax output records a normal call whose callee child
    ///   is a `FieldAccess` expression.
    ///
    /// Transformation:
    /// - Parses source through `parse_module_as_syntax_output` and inspects the
    ///   emitted call tree consumed by later method-resolution phases.
    #[test]
    fn syntax_output_includes_method_call_suffix_trees() {
        let output = parse_module_as_syntax_output(
            r#"
            module method_call_suffix.

            display(user: Dynamic): Dynamic ->
                user.display_name("short").
            "#,
        )
        .expect("syntax output method call suffix");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let body = &clauses[0].body;
        assert_eq!(body.kind, SyntaxExprKind::Call);
        assert_eq!(body.remote, None);
        assert_eq!(body.children.len(), 2);
        assert_eq!(body.children[0].kind, SyntaxExprKind::FieldAccess);
        assert_eq!(body.children[0].text.as_deref(), Some("display_name"));
        assert_eq!(body.children[0].children[0].kind, SyntaxExprKind::Var);
        assert_eq!(body.children[0].children[0].text.as_deref(), Some("user"));
        assert_eq!(body.children[1].kind, SyntaxExprKind::Binary);
        assert_eq!(body.children[1].text.as_deref(), Some("\"short\""));
    }

    #[test]
    fn syntax_output_includes_macro_expr_trees() {
        let output = parse_module_as_syntax_output(
            r#"
            module macro_expr_tree.

            module_name(): Dynamic ->
                ?MODULE.

            compare(a: Int, b: Int): Dynamic ->
                ?assert_equal(a, b).
            "#,
        )
        .expect("syntax output macro expr");

        let SyntaxDeclarationPayload::Function {
            clauses: module_clauses,
            ..
        } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };
        assert_eq!(module_clauses[0].body.kind, SyntaxExprKind::Macro);
        assert_eq!(module_clauses[0].body.text.as_deref(), Some("MODULE"));
        assert_eq!(module_clauses[0].body.arity, 0);

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[1].payload
        else {
            panic!("expected function declaration");
        };
        assert_eq!(clauses[0].body.kind, SyntaxExprKind::Macro);
        assert_eq!(clauses[0].body.text.as_deref(), Some("assert_equal"));
        assert_eq!(clauses[0].body.children.len(), 2);
        assert_eq!(clauses[0].body.arity, 2);
    }

    #[test]
    fn syntax_output_includes_raw_macro_expr_trees() {
        let output = parse_module_as_syntax_output(
            r#"
            module raw_macro_expr_tree.

            query(): Dynamic ->
                sql{select * from users}.
            "#,
        )
        .expect("syntax output raw macro expr");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let body = &clauses[0].body;
        assert_eq!(body.kind, SyntaxExprKind::RawMacro);
        assert_eq!(body.text.as_deref(), Some("sql"));
        assert_eq!(body.raw.as_deref(), Some("select * from users"));
    }

    #[test]
    fn syntax_output_includes_quoted_atom_literals() {
        let output = parse_module_as_syntax_output(
            r#"
            module quoted_atom_tree.

            module_atom(): Dynamic ->
                :'Elixir.Module'.

            classify(value: Dynamic): Dynamic ->
                case value {
                    :'some atom' -> :ok
                }.
            "#,
        )
        .expect("syntax output quoted atom literals");

        let SyntaxDeclarationPayload::Function {
            clauses: atom_clauses,
            ..
        } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };
        assert_eq!(atom_clauses[0].body.kind, SyntaxExprKind::Atom);
        assert_eq!(atom_clauses[0].body.text.as_deref(), Some("Elixir.Module"));

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[1].payload
        else {
            panic!("expected function declaration");
        };
        let case_expr = &clauses[0].body;
        assert_eq!(case_expr.kind, SyntaxExprKind::Case);
        assert_eq!(
            case_expr.clauses[0].patterns[0].text.as_deref(),
            Some("some atom")
        );
    }

    /// Verifies prefixed integer literals cross the formal syntax-output
    /// boundary as normalized integer values.
    ///
    /// Inputs:
    /// - A module containing decimal, binary, hexadecimal, and octal integer
    ///   literal function bodies.
    ///
    /// Output:
    /// - Test passes when each function body is a `SyntaxExprKind::Int` and the
    ///   prefixed forms normalize to decimal value text.
    ///
    /// Transformation:
    /// - Parses source through `parse_module_as_syntax_output`, extracts each
    ///   function clause body, and compares the syntax-output value text.
    #[test]
    fn syntax_output_normalizes_prefixed_integer_literals() {
        let output = parse_module_as_syntax_output(
            r#"
            module radix_literals.

            decimal_int(): Int -> 42.
            binary_int(): Int -> 0b101010.
            hex_int(): Int -> 0x2a.
            octal_int(): Int -> 0o52.
            "#,
        )
        .expect("syntax output radix literals");

        let literal_texts = output
            .declarations
            .iter()
            .map(|declaration| match &declaration.payload {
                SyntaxDeclarationPayload::Function { clauses, .. } => {
                    assert_eq!(clauses[0].body.kind, SyntaxExprKind::Int);
                    clauses[0].body.text.as_deref()
                }
                other => panic!("unexpected declaration payload: {other:?}"),
            })
            .collect::<Vec<_>>();

        assert_eq!(
            literal_texts,
            vec![Some("42"), Some("42"), Some("42"), Some("42")]
        );
    }

    /// Verifies binary segment syntax is preserved at the syntax-output
    /// boundary.
    ///
    /// Inputs:
    /// - A module containing a binary expression with size and segment type
    ///   modifiers.
    ///
    /// Output:
    /// - Test passes when syntax output records the body as a binary expression
    ///   and preserves the complete source token text.
    ///
    /// Transformation:
    /// - Parses source through `parse_module_as_syntax_output` and inspects the
    ///   binary expression payload without interpreting segment semantics.
    #[test]
    fn syntax_output_preserves_binary_segment_text() {
        let output = parse_module_as_syntax_output(
            r#"
            module binary_segment_text.

            byte(value: Int): Binary ->
                <<value:8/integer-unsigned-big>>.
            "#,
        )
        .expect("syntax output binary segment text");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let body = &clauses[0].body;
        assert_eq!(body.kind, SyntaxExprKind::Binary);
        assert_eq!(
            body.text.as_deref(),
            Some("<<value:8/integer-unsigned-big>>")
        );
    }

    #[test]
    fn syntax_output_includes_constructor_chain_expr_trees() {
        let output = parse_module_as_syntax_output(
            r#"
            module constructor_chain_expr_tree.

            demo(id: Int, name: Binary): Dynamic ->
                User(id, name) with Admin { id = id, name = name }.
            "#,
        )
        .expect("syntax output constructor chain expr");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let body = &clauses[0].body;
        assert_eq!(body.kind, SyntaxExprKind::ConstructorChain);
        assert_eq!(body.children.len(), 2);
        assert_eq!(body.children[0].kind, SyntaxExprKind::Call);
        assert_eq!(body.children[1].kind, SyntaxExprKind::RecordConstruct);
        assert_eq!(body.children[1].text.as_deref(), Some("Admin"));
    }

    #[test]
    fn syntax_output_allows_keyword_expressions_in_operator_chains() {
        let output = parse_module_as_syntax_output(
            r#"
            module keyword_expr_chain.

            demo(option: Dynamic): Dynamic ->
                case option {
                    :none -> 0;
                    value -> value
                } |> inspect().
            "#,
        )
        .expect("syntax output keyword expression chain");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let pipe = &clauses[0].body;
        assert_eq!(pipe.kind, SyntaxExprKind::BinaryOp);
        assert_eq!(pipe.operator.as_deref(), Some("|>"));
        assert_eq!(pipe.children.len(), 2);

        let case_expr = &pipe.children[0];
        assert_eq!(case_expr.kind, SyntaxExprKind::Case);
        assert_eq!(case_expr.clauses.len(), 2);
        assert_eq!(
            case_expr.clauses[0].patterns[0].kind,
            SyntaxPatternKind::Atom
        );
        assert_eq!(
            case_expr.clauses[0].patterns[0].text.as_deref(),
            Some("none")
        );

        assert_eq!(pipe.children[1].kind, SyntaxExprKind::Call);
    }

    #[test]
    fn syntax_output_includes_if_expression_trees() {
        let output = parse_module_as_syntax_output(
            r#"
            module if_expr.

            choose(flag: Bool): Int ->
                if {
                    flag -> 1;
                    true -> 0
                }.
            "#,
        )
        .expect("syntax output if expression");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let body = &clauses[0].body;
        assert_eq!(body.kind, SyntaxExprKind::If);
        assert_eq!(body.clauses.len(), 2);
        let condition = body.clauses[0].guard.as_ref().expect("if condition");
        assert_eq!(condition.kind, SyntaxExprKind::Var);
        assert_eq!(condition.text.as_deref(), Some("flag"));
        assert_eq!(body.clauses[0].body.kind, SyntaxExprKind::Int);
        assert_eq!(body.clauses[0].body.text.as_deref(), Some("1"));
    }

    #[test]
    fn syntax_output_includes_receive_expression_trees() {
        let output = parse_module_as_syntax_output(
            r#"
            module receive_expr.

            wait(): Int ->
                receive {
                    {:ok, value} -> value;
                    :stop -> 0
                }.
            "#,
        )
        .expect("syntax output receive expression");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let body = &clauses[0].body;
        assert_eq!(body.kind, SyntaxExprKind::Receive);
        assert_eq!(body.clauses.len(), 2);
        assert_eq!(body.clauses[0].patterns[0].kind, SyntaxPatternKind::Tuple);
        assert_eq!(body.clauses[0].body.kind, SyntaxExprKind::Var);
        assert_eq!(body.clauses[0].body.text.as_deref(), Some("value"));
        assert_eq!(body.clauses[1].patterns[0].kind, SyntaxPatternKind::Atom);
        assert_eq!(body.clauses[1].patterns[0].text.as_deref(), Some("stop"));
    }

    #[test]
    fn syntax_output_includes_receive_after_expression_trees() {
        let output = parse_module_as_syntax_output(
            r#"
            module receive_after_expr.

            wait(): Int ->
                receive {
                    {:ok, value} -> value;
                    after
                        0 -> 1
                    }.
            "#,
        )
        .expect("syntax output receive after expression");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let body = &clauses[0].body;
        assert_eq!(body.kind, SyntaxExprKind::Receive);
        assert_eq!(body.clauses.len(), 1);
        assert!(body.clauses[0].patterns[0].kind == SyntaxPatternKind::Tuple);
        let after = body
            .receive_after
            .as_ref()
            .expect("expected receive after output");
        assert_eq!(after.trigger.kind, SyntaxExprKind::Int);
        assert_eq!(after.trigger.text.as_deref(), Some("0"));
        assert_eq!(after.body.kind, SyntaxExprKind::Int);
    }

    #[test]
    fn syntax_output_includes_try_expression_trees() {
        let output = parse_module_as_syntax_output(
            r#"
            module try_expr.

            wait(): Int ->
                try risky() {
                    {:ok, value} -> value
                catch
                    :error -> 0
                after
                    0 -> cleanup()
                }.
            "#,
        )
        .expect("syntax output try expression");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let body = &clauses[0].body;
        assert_eq!(body.kind, SyntaxExprKind::Try);
        assert_eq!(body.children[0].kind, SyntaxExprKind::Call);
        assert_eq!(body.clauses.len(), 1);
        assert_eq!(body.catch_clauses.len(), 1);
        assert_eq!(body.clauses[0].patterns[0].kind, SyntaxPatternKind::Tuple);
        assert_eq!(
            body.catch_clauses[0].patterns[0].kind,
            SyntaxPatternKind::Atom
        );
        let after = body.try_after.as_ref().expect("expected try after output");
        assert_eq!(after.trigger.kind, SyntaxExprKind::Int);
        assert_eq!(after.trigger.text.as_deref(), Some("0"));
        assert_eq!(after.body.kind, SyntaxExprKind::Call);
    }

    #[test]
    fn syntax_output_includes_structured_html_nodes() {
        let output = parse_module_as_syntax_output(
            r#"
            module html_tree.

            view(Title: Text): Html[:none] ->
                html {
                    <section class={["hero", "compact"]}>
                        <h1>{Title}</h1>
                    </section>
                }.
            "#,
        )
        .expect("syntax output");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };
        let body = &clauses[0].body;

        assert_eq!(body.kind, SyntaxExprKind::HtmlBlock);
        assert_eq!(body.html_nodes.len(), 1);
        let SyntaxHtmlNodeOutput::Element { element } = &body.html_nodes[0] else {
            panic!("expected root html element");
        };
        assert_eq!(element.name, "section");
        assert_eq!(element.attrs.len(), 1);
        assert_eq!(element.attrs[0].name, "class");
        match element.attrs[0].value.as_ref().expect("class value") {
            SyntaxHtmlAttrValueOutput::Expr { expr } => assert_eq!(expr.kind, SyntaxExprKind::List),
            other => panic!("unexpected html attr value: {other:?}"),
        }
        let SyntaxHtmlNodeOutput::Element { element: heading } = &element.children[0] else {
            panic!("expected heading child");
        };
        assert_eq!(heading.name, "h1");
        let SyntaxHtmlNodeOutput::Expr { expr } = &heading.children[0] else {
            panic!("expected heading interpolation");
        };
        assert_eq!(expr.kind, SyntaxExprKind::Var);
        assert_eq!(expr.text.as_deref(), Some("Title"));
    }

    #[test]
    fn syntax_output_marks_constructor_pattern_candidates() {
        let output = parse_module_as_syntax_output(
            r#"
            module constructor_patterns.

            unwrap(Result: Result): Int ->
                case Result {
                    Ok(value) -> value;
                    None -> 0
                }.
            "#,
        )
        .expect("syntax output");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };
        let body = &clauses[0].body;
        let ok_pattern = &body.clauses[0].patterns[0];
        assert_eq!(ok_pattern.kind, SyntaxPatternKind::Constructor);
        assert_eq!(ok_pattern.text.as_deref(), Some("Ok"));
        assert_eq!(ok_pattern.children.len(), 1);
        assert_eq!(ok_pattern.children[0].kind, SyntaxPatternKind::Var);

        let none_pattern = &body.clauses[1].patterns[0];
        assert_eq!(none_pattern.kind, SyntaxPatternKind::Constructor);
        assert_eq!(none_pattern.text.as_deref(), Some("None"));
        assert!(none_pattern.children.is_empty());
    }

    #[test]
    fn syntax_output_keeps_constructor_call_candidates_as_named_calls() {
        let output = parse_module_as_syntax_output(
            r#"
            module constructor_calls.

            make(): Dynamic ->
                Ok(123).
            "#,
        )
        .expect("syntax output constructor call candidate");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };

        let body = &clauses[0].body;
        assert_eq!(body.kind, SyntaxExprKind::Call);
        assert_eq!(body.children.len(), 2);
        assert_eq!(body.children[0].kind, SyntaxExprKind::Var);
        assert_eq!(body.children[0].text.as_deref(), Some("Ok"));
        assert_eq!(body.children[1].kind, SyntaxExprKind::Int);
        assert_eq!(body.children[1].text.as_deref(), Some("123"));
    }

    #[test]
    fn syntax_output_includes_list_cons_expr_and_pattern_trees() {
        let output = parse_module_as_syntax_output(
            r#"
            module list_cons_trees.

            prepend(head: Dynamic, tail: List[Dynamic]): Dynamic ->
                [head | tail].

            pick(input: List[Dynamic]): Dynamic ->
                case input {
                    [head | tail] -> head;
                    [] -> :empty
                }.
            "#,
        )
        .expect("syntax output list cons trees");

        let SyntaxDeclarationPayload::Function {
            clauses: prepend_clauses,
            ..
        } = &output.declarations[0].payload
        else {
            panic!("expected prepend function declaration");
        };
        let prepend = &prepend_clauses[0].body;
        assert_eq!(prepend.kind, SyntaxExprKind::ListCons);
        assert_eq!(prepend.children.len(), 2);
        assert_eq!(prepend.children[0].kind, SyntaxExprKind::Var);
        assert_eq!(prepend.children[0].text.as_deref(), Some("head"));
        assert_eq!(prepend.children[1].kind, SyntaxExprKind::Var);
        assert_eq!(prepend.children[1].text.as_deref(), Some("tail"));

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[1].payload
        else {
            panic!("expected pick function declaration");
        };
        let case_expr = &clauses[0].body;
        let pattern = &case_expr.clauses[0].patterns[0];
        assert_eq!(pattern.kind, SyntaxPatternKind::ListCons);
        assert_eq!(pattern.children.len(), 2);
        assert_eq!(pattern.children[0].kind, SyntaxPatternKind::Var);
        assert_eq!(pattern.children[0].text.as_deref(), Some("head"));
        assert_eq!(pattern.children[1].kind, SyntaxPatternKind::Var);
        assert_eq!(pattern.children[1].text.as_deref(), Some("tail"));
    }

    #[test]
    fn syntax_output_includes_record_suffix_trees() {
        let output = parse_module_as_syntax_output(
            r#"
            module record_suffix_trees.

            field(user: Dynamic): Dynamic ->
                user#foo.bar.

            update(user: Dynamic): Dynamic ->
                user#foo{bar = 2}.
            "#,
        )
        .expect("syntax output record suffix trees");

        let SyntaxDeclarationPayload::Function {
            clauses: field_clauses,
            ..
        } = &output.declarations[0].payload
        else {
            panic!("expected field function declaration");
        };
        let access = &field_clauses[0].body;
        assert_eq!(access.kind, SyntaxExprKind::RecordAccess);
        assert_eq!(access.text.as_deref(), Some("foo.bar"));
        assert_eq!(access.children.len(), 1);
        assert_eq!(access.children[0].kind, SyntaxExprKind::Var);
        assert_eq!(access.children[0].text.as_deref(), Some("user"));

        let SyntaxDeclarationPayload::Function {
            clauses: update_clauses,
            ..
        } = &output.declarations[1].payload
        else {
            panic!("expected update function declaration");
        };
        let update = &update_clauses[0].body;
        assert_eq!(update.kind, SyntaxExprKind::RecordUpdate);
        assert_eq!(update.text.as_deref(), Some("foo"));
        assert_eq!(update.children.len(), 1);
        assert_eq!(update.children[0].kind, SyntaxExprKind::Var);
        assert_eq!(update.children[0].text.as_deref(), Some("user"));
        assert_eq!(update.fields.len(), 1);
        assert_eq!(update.fields[0].key, "bar");
        assert_eq!(update.fields[0].value.kind, SyntaxExprKind::Int);
        assert_eq!(update.fields[0].value.text.as_deref(), Some("2"));
    }

    #[test]
    fn syntax_output_includes_sequence_primary_expr_trees() {
        let output = parse_module_as_syntax_output(
            r#"
            module sequence_primary_trees.

            binary(): Binary ->
                <<"hello">>.

            fixed(): FixedArray[3, Int] ->
                #[1, 2, 3].

            indexed(items: List[Int]): Int ->
                items[0].

            generated(items: List[Int]): List[Int] ->
                [item | item <- items].
            "#,
        )
        .expect("syntax output sequence primary trees");

        let SyntaxDeclarationPayload::Function {
            clauses: binary_clauses,
            ..
        } = &output.declarations[0].payload
        else {
            panic!("expected binary function declaration");
        };
        let binary = &binary_clauses[0].body;
        assert_eq!(binary.kind, SyntaxExprKind::Binary);
        assert_eq!(binary.text.as_deref(), Some("<<\"hello\">>"));

        let SyntaxDeclarationPayload::Function {
            clauses: fixed_clauses,
            ..
        } = &output.declarations[1].payload
        else {
            panic!("expected fixed array function declaration");
        };
        let fixed = &fixed_clauses[0].body;
        assert_eq!(fixed.kind, SyntaxExprKind::FixedArray);
        assert_eq!(fixed.children.len(), 3);
        assert_eq!(fixed.children[0].text.as_deref(), Some("1"));
        assert_eq!(fixed.children[2].text.as_deref(), Some("3"));

        let SyntaxDeclarationPayload::Function {
            clauses: index_clauses,
            ..
        } = &output.declarations[2].payload
        else {
            panic!("expected indexed function declaration");
        };
        let indexed = &index_clauses[0].body;
        assert_eq!(indexed.kind, SyntaxExprKind::Index);
        assert_eq!(indexed.children.len(), 2);
        assert_eq!(indexed.children[0].kind, SyntaxExprKind::Var);
        assert_eq!(indexed.children[0].text.as_deref(), Some("items"));
        assert_eq!(indexed.children[1].kind, SyntaxExprKind::Int);
        assert_eq!(indexed.children[1].text.as_deref(), Some("0"));

        let SyntaxDeclarationPayload::Function {
            clauses: generated_clauses,
            ..
        } = &output.declarations[3].payload
        else {
            panic!("expected generated function declaration");
        };
        let generated = &generated_clauses[0].body;
        assert_eq!(generated.kind, SyntaxExprKind::ListComprehension);
        assert_eq!(generated.children.len(), 2);
        assert_eq!(generated.children[0].kind, SyntaxExprKind::Var);
        assert_eq!(generated.children[0].text.as_deref(), Some("item"));
        assert_eq!(generated.children[1].kind, SyntaxExprKind::Var);
        assert_eq!(generated.children[1].text.as_deref(), Some("items"));
        assert_eq!(generated.patterns.len(), 1);
        assert_eq!(generated.patterns[0].kind, SyntaxPatternKind::Var);
        assert_eq!(generated.patterns[0].text.as_deref(), Some("item"));
    }

    #[test]
    fn syntax_output_includes_map_constructor_record_and_template_field_trees() {
        let output = parse_module_as_syntax_output(
            r#"
            module field_payload_trees.

            map(): Map ->
                #{a := 1, b => 2}.

            chain(id: Int): Dynamic ->
                User(id) with Admin{name = "Ada"}.

            render_template(): Dynamic ->
                Page{title = "hello"}.
            "#,
        )
        .expect("syntax output field payload trees");

        let SyntaxDeclarationPayload::Function {
            clauses: map_clauses,
            ..
        } = &output.declarations[0].payload
        else {
            panic!("expected map function declaration");
        };
        let map = &map_clauses[0].body;
        assert_eq!(map.kind, SyntaxExprKind::Map);
        assert_eq!(map.fields.len(), 2);
        assert_eq!(map.fields[0].key, "a");
        assert!(map.fields[0].required);
        assert_eq!(map.fields[0].value.kind, SyntaxExprKind::Int);
        assert_eq!(map.fields[0].value.text.as_deref(), Some("1"));
        assert_eq!(map.fields[1].key, "b");
        assert!(!map.fields[1].required);
        assert_eq!(map.fields[1].value.kind, SyntaxExprKind::Int);
        assert_eq!(map.fields[1].value.text.as_deref(), Some("2"));

        let SyntaxDeclarationPayload::Function {
            clauses: chain_clauses,
            ..
        } = &output.declarations[1].payload
        else {
            panic!("expected chain function declaration");
        };
        let chain = &chain_clauses[0].body;
        assert_eq!(chain.kind, SyntaxExprKind::ConstructorChain);
        assert_eq!(chain.children.len(), 2);
        let record = &chain.children[1];
        assert_eq!(record.kind, SyntaxExprKind::RecordConstruct);
        assert_eq!(record.text.as_deref(), Some("Admin"));
        assert_eq!(record.fields.len(), 1);
        assert_eq!(record.fields[0].key, "name");
        assert!(record.fields[0].required);
        assert_eq!(record.fields[0].value.kind, SyntaxExprKind::Binary);
        assert_eq!(record.fields[0].value.text.as_deref(), Some("\"Ada\""));

        let SyntaxDeclarationPayload::Function {
            clauses: template_clauses,
            ..
        } = &output.declarations[2].payload
        else {
            panic!("expected template function declaration");
        };
        let template = &template_clauses[0].body;
        assert_eq!(template.kind, SyntaxExprKind::TemplateInstantiate);
        assert_eq!(template.text.as_deref(), Some("Page"));
        assert_eq!(template.fields.len(), 1);
        assert_eq!(template.fields[0].key, "title");
        assert!(template.fields[0].required);
        assert_eq!(template.fields[0].value.kind, SyntaxExprKind::Binary);
        assert_eq!(template.fields[0].value.text.as_deref(), Some("\"hello\""));
    }

    #[test]
    fn syntax_output_includes_struct_constructor_trait_and_template_signatures() {
        let output = parse_module_as_syntax_output(
            r#"
            module rich.

            pub struct User derives Person {
                /// Stable internal ID.
                id: Int,
                name: Text = :guest
            }.

            pub constructor Queue[T] {
                (Items: List[T], Limit: Int = 10): Queue[T] ->
                    from_list(Items)
            }.

            pub trait Show[A] {
                show(Value: A): Text.
            }.

            template Page from "./page.tl.html" {
                title: Text
            }.
            "#,
        )
        .expect("rich syntax output");

        match &output.declarations[0].payload {
            SyntaxDeclarationPayload::Struct {
                name,
                is_public,
                derives,
                implements,
                fields,
            } => {
                assert_eq!(name, "User");
                assert!(*is_public);
                assert_eq!(derives, &vec!["Person".to_string()]);
                assert!(implements.is_empty());
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].name, "id");
                assert_eq!(fields[0].annotation.text, "Int");
                assert_eq!(fields[0].docs, vec!["Stable internal ID."]);
                assert!(!fields[0].has_default);
                assert_eq!(fields[1].name, "name");
                assert_eq!(fields[1].annotation.text, "Text");
                assert!(fields[1].has_default);
                let default = fields[1].default.as_ref().expect("field default");
                assert_eq!(default.kind, SyntaxExprKind::Atom);
                assert_eq!(default.text.as_deref(), Some("guest"));
            }
            other => panic!("unexpected struct payload: {other:?}"),
        }

        match &output.declarations[1].payload {
            SyntaxDeclarationPayload::Constructor {
                name,
                params,
                is_public,
                clauses,
            } => {
                assert_eq!(name, "Queue");
                assert_eq!(params, &vec!["T".to_string()]);
                assert!(*is_public);
                assert_eq!(clauses.len(), 1);
                assert_eq!(clauses[0].params[0].name, "Items");
                assert_eq!(clauses[0].params[0].annotation.text, "List[T]");
                assert_eq!(clauses[0].params[1].name, "Limit");
                assert!(clauses[0].params[1].has_default);
                let default = clauses[0].params[1]
                    .default
                    .as_ref()
                    .expect("constructor param default");
                assert_eq!(default.kind, SyntaxExprKind::Int);
                assert_eq!(default.text.as_deref(), Some("10"));
                assert_eq!(clauses[0].return_type.text, "Queue[T]");
            }
            other => panic!("unexpected constructor payload: {other:?}"),
        }

        match &output.declarations[2].payload {
            SyntaxDeclarationPayload::Trait {
                name,
                params,
                is_public,
                methods,
                ..
            } => {
                assert_eq!(name, "Show");
                assert_eq!(params, &vec!["A".to_string()]);
                assert!(*is_public);
                assert_eq!(methods.len(), 1);
                assert_eq!(methods[0].name, "show");
                assert_eq!(methods[0].params[0].name, "Value");
                assert_eq!(methods[0].params[0].annotation.text, "A");
                assert_eq!(methods[0].return_type.text, "Text");
            }
            other => panic!("unexpected trait payload: {other:?}"),
        }

        match &output.declarations[3].payload {
            SyntaxDeclarationPayload::Template {
                name,
                source_path,
                props,
            } => {
                assert_eq!(name, "Page");
                assert_eq!(source_path, "./page.tl.html");
                assert_eq!(props.len(), 1);
                assert_eq!(props[0].name, "title");
                assert_eq!(props[0].annotation.text, "Text");
            }
            other => panic!("unexpected template payload: {other:?}"),
        }
    }
}
