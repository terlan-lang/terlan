use crate::terlan_syntax::span::Span;

/// Parsed Terlan source module.
#[derive(Debug, Clone)]
pub struct Module {
    pub name: String,
    pub docs: Vec<String>,
    pub declarations: Vec<Decl>,
    pub declaration_annotations: Vec<Vec<Annotation>>,
    pub span: Span,
}

/// Parsed source annotation attached to a declaration or item.
#[derive(Debug, Clone)]
pub struct Annotation {
    pub path: Vec<String>,
    pub args: Option<String>,
    pub entries: Vec<AnnotationEntry>,
    pub values: Vec<AnnotationValue>,
    pub span: Span,
}

/// Key-value entry inside an annotation object body.
#[derive(Debug, Clone)]
pub struct AnnotationEntry {
    pub key: Vec<String>,
    pub value: AnnotationValue,
    pub span: Span,
}

/// Literal value accepted by annotation syntax.
#[derive(Debug, Clone)]
pub enum AnnotationValue {
    Name(Vec<String>),
    Bool(bool),
    Int(String),
    Float(String),
    String(String),
    List(Vec<AnnotationValue>),
    Object(Vec<AnnotationEntry>),
}

/// Top-level declaration node in a Terlan module.
#[derive(Debug, Clone)]
pub enum Decl {
    Import(ImportDecl),
    Export(ExportDecl),
    Type(TypeDecl),
    Struct(StructDecl),
    Constructor(ConstructorDecl),
    Function(FunctionDecl),
    Method(MethodDecl),
    Trait(TraitDecl),
    TraitImpl(TraitImplDecl),
    AnnotationSchema(AnnotationSchemaDecl),
    Template(TemplateDecl),
    Raw(UnsupportedDecl),
}

/// Source import declaration.
#[derive(Debug, Clone)]
pub struct ImportDecl {
    pub kind: ImportKind,
    pub module_name: String,
    pub items: Vec<ImportItem>,
    pub is_type: bool,
    pub is_selected: bool,
    pub source_path: Option<String>,
    pub span: Span,
}

/// Import source category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportKind {
    Module,
    File,
    Css,
    Markdown,
}

/// One imported name and its optional local alias.
#[derive(Debug, Clone)]
pub struct ImportItem {
    pub name: String,
    pub as_alias: Option<String>,
    pub span: Span,
}

/// Explicit export declaration retained for parsed source compatibility.
#[derive(Debug, Clone)]
pub struct ExportDecl {
    pub items: Vec<ExportItem>,
    pub span: Span,
}

/// One exported function identity.
#[derive(Debug, Clone)]
pub struct ExportItem {
    pub name: String,
    pub arity: usize,
    pub span: Span,
}

/// Type or opaque type declaration.
#[derive(Debug, Clone)]
pub struct TypeDecl {
    pub name: String,
    pub params: Vec<String>,
    pub variants: Vec<TypeExpr>,
    pub implements: Vec<TypeExpr>,
    pub is_public: bool,
    pub is_opaque: bool,
    pub docs: Vec<String>,
    pub span: Span,
}

/// Struct declaration and its declared fields.
#[derive(Debug, Clone)]
pub struct StructDecl {
    pub name: String,
    pub includes: Vec<String>,
    pub implements: Vec<TypeExpr>,
    pub fields: Vec<StructFieldDecl>,
    pub is_public: bool,
    pub docs: Vec<String>,
    pub span: Span,
}

/// One field in a struct declaration.
#[derive(Debug, Clone)]
pub struct StructFieldDecl {
    pub name: String,
    pub annotation: TypeExpr,
    pub default: Option<Expr>,
    pub is_private: bool,
    pub docs: Vec<String>,
    pub span: Span,
}

/// Constructor declaration for a named type.
#[derive(Debug, Clone)]
pub struct ConstructorDecl {
    pub name: String,
    pub params: Vec<String>,
    pub clauses: Vec<ConstructorClause>,
    pub is_public: bool,
    pub docs: Vec<String>,
    pub span: Span,
}

/// One constructor clause body and signature.
#[derive(Debug, Clone)]
pub struct ConstructorClause {
    pub params: Vec<ConstructorParam>,
    pub return_type: TypeExpr,
    pub body: Expr,
    pub span: Span,
}

/// One constructor parameter.
#[derive(Debug, Clone)]
pub struct ConstructorParam {
    pub name: String,
    pub annotation: TypeExpr,
    pub default: Option<Expr>,
    pub is_varargs: bool,
    pub span: Span,
}

/// Named function declaration.
#[derive(Debug, Clone)]
pub struct FunctionDecl {
    pub name: String,
    pub generic_params: Vec<String>,
    pub params: Vec<Param>,
    pub return_type: TypeExpr,
    pub is_public: bool,
    pub is_macro: bool,
    pub generic_bounds: Vec<String>,
    pub clauses: Vec<FunctionClause>,
    pub docs: Vec<String>,
    pub span: Span,
}

/// Receiver method declaration.
#[derive(Debug, Clone)]
pub struct MethodDecl {
    pub receiver: Param,
    pub name: String,
    pub generic_params: Vec<String>,
    pub params: Vec<Param>,
    pub return_type: TypeExpr,
    pub is_public: bool,
    pub generic_bounds: Vec<String>,
    pub clauses: Vec<FunctionClause>,
    pub docs: Vec<String>,
    pub span: Span,
}

/// One pattern-matched function or lambda clause.
#[derive(Debug, Clone)]
pub struct FunctionClause {
    pub patterns: Vec<Pattern>,
    pub body: Expr,
    pub span: Span,
    pub guard: Option<Box<Expr>>,
}

/// Function, method, or receiver parameter.
#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub annotation: TypeExpr,
    pub is_mutable: bool,
    pub default: Option<Expr>,
    pub span: Span,
}

/// Raw parsed type expression text with source span.
#[derive(Debug, Clone)]
pub struct TypeExpr {
    pub text: String,
    pub span: Span,
}

/// Template declaration linking a template name to a source asset.
#[derive(Debug, Clone)]
pub struct TemplateDecl {
    pub name: String,
    pub source_path: String,
    pub props: Vec<TemplatePropDecl>,
    pub docs: Vec<String>,
    pub span: Span,
}

/// Compile-time schema declaration for a source annotation path.
#[derive(Debug, Clone)]
pub struct AnnotationSchemaDecl {
    pub path: Vec<String>,
    pub entries: Vec<AnnotationSchemaEntry>,
    pub is_public: bool,
    pub docs: Vec<String>,
    pub span: Span,
}

/// One entry inside an annotation schema declaration.
#[derive(Debug, Clone)]
pub enum AnnotationSchemaEntry {
    AppliesTo {
        targets: Vec<String>,
        span: Span,
    },
    Key {
        key: Vec<String>,
        value_type: AnnotationValueType,
        options: Vec<AnnotationKeyOption>,
        span: Span,
    },
}

/// One option attached to an annotation schema key.
#[derive(Debug, Clone)]
pub enum AnnotationKeyOption {
    Required { value: bool, span: Span },
    Repeatable { value: bool, span: Span },
    Default { value: AnnotationValue, span: Span },
    AppliesTo { targets: Vec<String>, span: Span },
}

/// Annotation metadata value type accepted by a schema key.
#[derive(Debug, Clone)]
pub struct AnnotationValueType {
    pub text: String,
}

/// One typed property accepted by a template declaration.
#[derive(Debug, Clone)]
pub struct TemplatePropDecl {
    pub name: String,
    pub annotation: TypeExpr,
    pub default: Option<Expr>,
    pub span: Span,
}

/// Pattern map field.
#[derive(Debug, Clone)]
pub struct MapField {
    pub key: String,
    pub value: Box<Pattern>,
    pub required: bool,
}

/// Expression map field.
#[derive(Debug, Clone)]
pub struct MapExprField {
    pub key: String,
    pub value: Box<Expr>,
    pub required: bool,
}

/// Pattern syntax tree used in cases, clauses, and destructuring.
#[derive(Debug, Clone)]
pub enum Pattern {
    Wildcard,
    Var(String),
    Int(i64),
    Float(f64),
    Atom(String),
    Tuple(Vec<Pattern>),
    List(Vec<Pattern>),
    ListCons(Box<Pattern>, Box<Pattern>),
    Map(Vec<MapField>),
    Record { name: String, fields: Vec<MapField> },
}

/// Expression syntax tree produced by the parser.
#[derive(Debug, Clone)]
pub enum Expr {
    Int(i64),
    Float(f64),
    Atom(String),
    AtomLiteral(String),
    Binary(String),
    Var(String),
    Tuple(Vec<Expr>),
    List(Vec<Expr>),
    ListCons(Box<Expr>, Box<Expr>),
    FixedArray(Vec<Expr>),
    Index(Box<Expr>, Box<Expr>),
    IndexAssign {
        collection: Box<Expr>,
        index: Box<Expr>,
        value: Box<Expr>,
    },
    Map(Vec<MapExprField>),
    ListComprehension {
        expr: Box<Expr>,
        pattern: Pattern,
        source: Box<Expr>,
        guard: Option<Box<Expr>>,
    },
    Let {
        bindings: Vec<LetBinding>,
        body: Option<Box<Expr>>,
    },
    Call {
        callee: Box<Expr>,
        type_args: Vec<TypeExpr>,
        args: Vec<Expr>,
        arg_names: Vec<Option<String>>,
        remote: Option<String>,
        is_fun_value: bool,
    },
    Case {
        scrutinee: Box<Expr>,
        clauses: Vec<CaseClause>,
    },
    Try {
        body: Box<Expr>,
        of_clauses: Vec<CaseClause>,
        catch_clauses: Vec<CaseClause>,
        after_clause: Option<TryAfterClause>,
    },
    If {
        clauses: Vec<IfClause>,
    },
    Fun {
        clauses: Vec<FunctionClause>,
    },
    MacroCall {
        name: String,
        args: Vec<Expr>,
    },
    RawMacro {
        name: String,
        type_args: Vec<TypeExpr>,
        interpolations: Vec<Expr>,
        raw: String,
    },
    HtmlBlock(HtmlBlockExpr),
    RecordAccess {
        value: Box<Expr>,
        name: String,
        field: String,
    },
    FieldAccess {
        value: Box<Expr>,
        field: String,
    },
    RecordUpdate {
        value: Box<Expr>,
        name: String,
        fields: Vec<MapExprField>,
    },
    RecordConstruct {
        name: String,
        fields: Vec<MapExprField>,
    },
    TemplateInstantiate {
        name: String,
        fields: Vec<MapExprField>,
    },
    ConstructorChain {
        base: Box<Expr>,
        record: Box<Expr>,
    },
    UnaryOp {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Cast {
        expr: Box<Expr>,
        target_type: TypeExpr,
    },
    BinaryOp {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Quote(Box<Expr>),
    Unquote(Box<Expr>),
    Sequence(Vec<Expr>),
}

/// One binding in a `let` expression.
#[derive(Debug, Clone)]
pub struct LetBinding {
    pub pattern: Pattern,
    pub value: Expr,
}

/// Parsed built-in block macro body.
#[derive(Debug, Clone)]
pub struct HtmlBlockExpr {
    pub macro_kind: BuiltinBlockMacro,
    pub raw: String,
    pub nodes: Vec<HtmlNode>,
}

/// Built-in raw block macro kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinBlockMacro {
    Html,
}

impl BuiltinBlockMacro {
    /// Resolves a source macro name to a built-in block macro.
    ///
    /// Inputs:
    /// - `name`: raw source macro name.
    ///
    /// Output:
    /// - Matching built-in block macro kind, or `None` for user macros.
    ///
    /// Transformation:
    /// - Keeps the parser's reserved built-in macro name table explicit.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "html" => Some(Self::Html),
            _ => None,
        }
    }

    /// Returns the canonical source name for a built-in block macro.
    ///
    /// Inputs:
    /// - `self`: built-in macro kind.
    ///
    /// Output:
    /// - Source spelling used by the parser and formatter.
    ///
    /// Transformation:
    /// - Converts the enum discriminant back to its reserved macro name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Html => "html",
        }
    }
}

/// Node inside a parsed HTML block.
#[derive(Debug, Clone)]
pub enum HtmlNode {
    Text(String),
    Element(HtmlElement),
    Expr(Expr),
    NamedSlot(HtmlNamedSlot),
}

/// HTML element node inside an HTML block.
#[derive(Debug, Clone)]
pub struct HtmlElement {
    pub name: String,
    pub attrs: Vec<HtmlAttr>,
    pub children: Vec<HtmlNode>,
}

/// Named slot node inside an HTML block.
#[derive(Debug, Clone)]
pub struct HtmlNamedSlot {
    pub name: String,
    pub children: Vec<HtmlNode>,
}

/// HTML attribute inside an HTML element node.
#[derive(Debug, Clone)]
pub struct HtmlAttr {
    pub name: String,
    pub value: Option<HtmlAttrValue>,
}

/// HTML attribute value.
#[derive(Debug, Clone)]
pub enum HtmlAttrValue {
    Text(String),
    Expr(Expr),
}

/// Pattern, optional guard, and body for case-like expressions.
#[derive(Debug, Clone)]
pub struct CaseClause {
    pub pattern: Pattern,
    pub guard: Option<Box<Expr>>,
    pub body: Expr,
}

/// Cleanup clause attached to a try expression.
#[derive(Debug, Clone)]
pub struct TryAfterClause {
    pub trigger: Box<Expr>,
    pub body: Box<Expr>,
}

/// One branch in an `if` expression.
#[derive(Debug, Clone)]
pub struct IfClause {
    pub condition: Expr,
    pub body: Expr,
}

/// Unary operator kind.
#[derive(Debug, Clone, Copy)]
pub enum UnaryOp {
    Neg,
    Not,
    Bang,
}

/// Binary operator kind.
#[derive(Debug, Clone, Copy)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    EqEq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    DivRem,
    Rem,
    And,
    Or,
    PipeForward,
}

/// Trait declaration with required and default methods.
#[derive(Debug, Clone)]
pub struct TraitDecl {
    pub name: String,
    pub params: Vec<String>,
    pub super_traits: Vec<String>,
    pub methods: Vec<TraitMethodDecl>,
    pub is_public: bool,
    pub docs: Vec<String>,
    pub span: Span,
}

/// Method signature declared inside a trait.
#[derive(Debug, Clone)]
pub struct TraitMethodDecl {
    pub name: String,
    pub generic_params: Vec<String>,
    pub params: Vec<Param>,
    pub return_type: TypeExpr,
    pub generic_bounds: Vec<String>,
    pub default_body: Option<Expr>,
    pub docs: Vec<String>,
    pub is_public: bool,
    pub span: Span,
}

/// Explicit trait implementation for a concrete type.
#[derive(Debug, Clone)]
pub struct TraitImplDecl {
    pub trait_ref: TypeExpr,
    pub for_type: TypeExpr,
    pub methods: Vec<FunctionDecl>,
    pub is_public: bool,
    pub docs: Vec<String>,
    pub span: Span,
}

/// Parsed unsupported declaration preserved for diagnostics.
#[derive(Debug, Clone)]
pub struct UnsupportedDecl {
    pub kind: String,
    pub text: String,
    pub docs: Vec<String>,
    pub span: Span,
}
