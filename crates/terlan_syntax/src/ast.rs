use crate::span::Span;

#[derive(Debug, Clone)]
pub struct Module {
    pub name: String,
    pub docs: Vec<String>,
    pub declarations: Vec<Decl>,
    pub declaration_annotations: Vec<Vec<Annotation>>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Annotation {
    pub path: Vec<String>,
    pub args: Option<String>,
    pub span: Span,
}

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
    Template(TemplateDecl),
    Raw(UnsupportedDecl),
}

#[derive(Debug, Clone)]
pub struct ImportDecl {
    pub kind: ImportKind,
    pub module_name: String,
    pub items: Vec<ImportItem>,
    pub is_type: bool,
    pub source_path: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportKind {
    Module,
    File,
    Css,
    Markdown,
}

#[derive(Debug, Clone)]
pub struct ImportItem {
    pub name: String,
    pub as_alias: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ExportDecl {
    pub items: Vec<ExportItem>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ExportItem {
    pub name: String,
    pub arity: usize,
    pub span: Span,
}

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

#[derive(Debug, Clone)]
pub struct StructDecl {
    pub name: String,
    pub derives: Vec<String>,
    pub implements: Vec<TypeExpr>,
    pub fields: Vec<StructFieldDecl>,
    pub is_public: bool,
    pub docs: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct StructFieldDecl {
    pub name: String,
    pub annotation: TypeExpr,
    pub default: Option<Expr>,
    pub docs: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ConstructorDecl {
    pub name: String,
    pub params: Vec<String>,
    pub clauses: Vec<ConstructorClause>,
    pub is_public: bool,
    pub docs: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ConstructorClause {
    pub params: Vec<ConstructorParam>,
    pub return_type: TypeExpr,
    pub body: Expr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ConstructorParam {
    pub name: String,
    pub annotation: TypeExpr,
    pub default: Option<Expr>,
    pub is_varargs: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FunctionDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: TypeExpr,
    pub is_public: bool,
    pub is_macro: bool,
    pub generic_bounds: Vec<String>,
    pub clauses: Vec<FunctionClause>,
    pub docs: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct MethodDecl {
    pub receiver: Param,
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: TypeExpr,
    pub is_public: bool,
    pub generic_bounds: Vec<String>,
    pub clauses: Vec<FunctionClause>,
    pub docs: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FunctionClause {
    pub patterns: Vec<Pattern>,
    pub body: Expr,
    pub span: Span,
    pub guard: Option<Box<Expr>>,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub annotation: TypeExpr,
    pub is_mutable: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TypeExpr {
    pub text: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TemplateDecl {
    pub name: String,
    pub source_path: String,
    pub props: Vec<TemplatePropDecl>,
    pub docs: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TemplatePropDecl {
    pub name: String,
    pub annotation: TypeExpr,
    pub docs: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct MapField {
    pub key: String,
    pub value: Box<Pattern>,
    pub required: bool,
}

#[derive(Debug, Clone)]
pub struct MapExprField {
    pub key: String,
    pub value: Box<Expr>,
    pub required: bool,
}

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
    MapField(String, Box<Pattern>, bool),
    Ignore,
    Placeholder,
    Record { name: String, fields: Vec<MapField> },
}

#[derive(Debug, Clone)]
pub enum Expr {
    Int(i64),
    Float(f64),
    Atom(String),
    Binary(String),
    Var(String),
    Tuple(Vec<Expr>),
    List(Vec<Expr>),
    ListCons(Box<Expr>, Box<Expr>),
    FixedArray(Vec<Expr>),
    Index(Box<Expr>, Box<Expr>),
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
        args: Vec<Expr>,
        remote: Option<String>,
        is_fun_value: bool,
    },
    Case {
        scrutinee: Box<Expr>,
        clauses: Vec<CaseClause>,
    },
    Receive {
        clauses: Vec<CaseClause>,
        after_clause: Option<ReceiveAfterClause>,
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
    RemoteFunRef {
        module: String,
        function: String,
        arity: usize,
    },
    MacroCall {
        name: String,
        args: Vec<Expr>,
    },
    RawMacro {
        name: String,
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

#[derive(Debug, Clone)]
pub struct LetBinding {
    pub name: String,
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub struct HtmlBlockExpr {
    pub macro_kind: BuiltinBlockMacro,
    pub raw: String,
    pub nodes: Vec<HtmlNode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinBlockMacro {
    Html,
}

impl BuiltinBlockMacro {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "html" => Some(Self::Html),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Html => "html",
        }
    }
}

#[derive(Debug, Clone)]
pub enum HtmlNode {
    Text(String),
    Element(HtmlElement),
    Expr(Expr),
    NamedSlot(HtmlNamedSlot),
}

#[derive(Debug, Clone)]
pub struct HtmlElement {
    pub name: String,
    pub attrs: Vec<HtmlAttr>,
    pub children: Vec<HtmlNode>,
}

#[derive(Debug, Clone)]
pub struct HtmlNamedSlot {
    pub name: String,
    pub children: Vec<HtmlNode>,
}

#[derive(Debug, Clone)]
pub struct HtmlAttr {
    pub name: String,
    pub value: Option<HtmlAttrValue>,
}

#[derive(Debug, Clone)]
pub enum HtmlAttrValue {
    Text(String),
    Expr(Expr),
}

#[derive(Debug, Clone)]
pub struct CaseClause {
    pub pattern: Pattern,
    pub guard: Option<Box<Expr>>,
    pub body: Expr,
}

#[derive(Debug, Clone)]
pub struct TryAfterClause {
    pub trigger: Box<Expr>,
    pub body: Box<Expr>,
}

#[derive(Debug, Clone)]
pub struct ReceiveAfterClause {
    pub trigger: Box<Expr>,
    pub body: Box<Expr>,
}

#[derive(Debug, Clone)]
pub struct IfClause {
    pub condition: Expr,
    pub body: Expr,
}

#[derive(Debug, Clone, Copy)]
pub enum UnaryOp {
    Neg,
    Not,
    Bang,
}

#[derive(Debug, Clone, Copy)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    EqEq,
    EqEqEq,
    NotEq,
    NotEqEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    DivRem,
    Rem,
    And,
    Or,
    PipeForward,
    Send,
}

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

#[derive(Debug, Clone)]
pub struct TraitMethodDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: TypeExpr,
    pub generic_bounds: Vec<String>,
    pub default_body: Option<Expr>,
    pub docs: Vec<String>,
    pub is_public: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TraitImplDecl {
    pub trait_ref: TypeExpr,
    pub for_type: TypeExpr,
    pub methods: Vec<FunctionDecl>,
    pub is_public: bool,
    pub docs: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct UnsupportedDecl {
    pub kind: String,
    pub text: String,
    pub docs: Vec<String>,
    pub span: Span,
}
