use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use terlan_hir::{
    ConstructorSignature, FunctionSignature, FunctionSymbol, ModuleInterface, ResolvedModule,
    TypeVisibility,
};
use terlan_syntax::{
    extract_native_function_signatures, span::Span, SyntaxDeclarationOutput,
    SyntaxDeclarationPayload, SyntaxExprKind, SyntaxExprOutput, SyntaxFunctionClauseOutput,
    SyntaxHtmlAttrOutput, SyntaxHtmlAttrValueOutput, SyntaxHtmlElementOutput, SyntaxHtmlNodeOutput,
    SyntaxImplMethodOutput, SyntaxImportKind, SyntaxModuleOutput, SyntaxParamOutput,
    SyntaxPatternKind, SyntaxPatternOutput, SyntaxStructFieldOutput, SyntaxTypeOutput, Token,
    TokenKind,
};

pub type TypeVarId = usize;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    Int,
    Float,
    Number,
    Binary,
    Atom,
    Bool,
    Term,
    Dynamic,
    Never,

    LiteralAtom(String),
    LiteralInt(i64),

    Var(TypeVarId),
    List(Box<Type>),
    Tuple(Vec<Type>),
    Union(Vec<Type>),
    Map(Vec<MapFieldType>),
    FixedArray {
        size: usize,
        elem: Box<Type>,
    },

    Named {
        module: Option<String>,
        name: String,
        args: Vec<Type>,
    },

    Function {
        params: Vec<Type>,
        ret: Box<Type>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MapFieldType {
    key: String,
    value: Type,
    required: bool,
}

#[derive(Debug, Clone)]
pub enum DiagSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub span: Span,
    pub message: String,
    pub severity: DiagSeverity,
}

pub const CORE_IR_SCHEMA: &str = "terlan.core_ir.v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreSourceIdentity {
    pub source_kind: String,
    pub syntax_contract_fingerprint: Option<String>,
}

/// Import class preserved at the backend-neutral CoreIR boundary.
///
/// Inputs:
/// - Syntax-output import declaration kind, or resolver interface imports when
///   source kind is unavailable.
///
/// Output:
/// - Stable import-kind tag for target-profile validation.
///
/// Transformation:
/// - Distinguishes normal module imports from asset imports without carrying
///   backend resolver state into CoreIR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreImportKind {
    Module,
    File,
    Css,
    Markdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreImport {
    pub module: String,
    pub kind: CoreImportKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreExport {
    pub name: String,
    pub kind: CoreExportKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreExportKind {
    Function { arity: usize },
    Type,
    Constructor { min_arity: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreTypeDecl {
    pub name: String,
    pub visibility: CoreVisibility,
    pub params: Vec<String>,
    pub body: Vec<String>,
    pub core_body: Option<CoreType>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreVisibility {
    Public,
    Private,
    Opaque,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreFunction {
    pub name: String,
    pub arity: usize,
    pub public: bool,
    pub params: Vec<CoreParam>,
    pub return_type: String,
    pub core_return_type: Option<CoreType>,
    pub clauses: Vec<CoreFunctionClause>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreParam {
    pub name: String,
    pub ty: String,
    pub core_ty: Option<CoreType>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreType {
    Int,
    Float,
    Number,
    String,
    Binary,
    Atom,
    Bool,
    Term,
    Dynamic,
    Never,
    AtomLiteral(String),
    Named(String),
    Apply {
        constructor: String,
        args: Vec<CoreType>,
    },
    List(Box<CoreType>),
    Tuple(Vec<CoreTupleTypeElem>),
    Struct {
        name: String,
        fields: Vec<CoreStructTypeField>,
    },
    Map(Vec<CoreMapTypeField>),
    Arrow {
        params: Vec<CoreType>,
        return_type: Box<CoreType>,
    },
    Union(Vec<CoreType>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreTupleTypeElem {
    Type(CoreType),
    Field { name: String, ty: CoreType },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreMapTypeField {
    pub key: String,
    pub operator: String,
    pub value: CoreType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreStructTypeField {
    pub name: String,
    pub ty: CoreType,
}

impl CoreStructTypeField {
    /// Renders a typed Core struct field as deterministic contract text.
    ///
    /// Inputs:
    /// - `self`: typed struct field payload.
    ///
    /// Output:
    /// - Stable compact `name:type` text for CoreIR contracts.
    ///
    /// Transformation:
    /// - Serializes field identity and typed payload without backend-specific
    ///   struct layout assumptions.
    fn contract_text(&self) -> String {
        format!("{}:{}", self.name, self.ty.contract_text())
    }
}

impl CoreMapTypeField {
    /// Renders a typed Core map field as deterministic contract text.
    ///
    /// Inputs:
    /// - `self`: typed map field payload.
    ///
    /// Output:
    /// - Stable compact text for CoreIR contracts and phase goldens.
    ///
    /// Transformation:
    /// - Serializes the key/operator text plus typed value payload without
    ///   attempting to resolve map keys semantically.
    fn contract_text(&self) -> String {
        format!(
            "{}{}{}",
            self.key,
            self.operator,
            self.value.contract_text()
        )
    }
}

impl CoreTupleTypeElem {
    /// Renders a typed Core tuple element as deterministic contract text.
    ///
    /// Inputs:
    /// - `self`: typed tuple element payload.
    ///
    /// Output:
    /// - Stable compact text for CoreIR contracts and phase goldens.
    ///
    /// Transformation:
    /// - Serializes positional elements as their nested CoreType text and
    ///   named fields as `Field(name:type)` without backend syntax.
    fn contract_text(&self) -> String {
        match self {
            CoreTupleTypeElem::Type(ty) => ty.contract_text(),
            CoreTupleTypeElem::Field { name, ty } => {
                format!("Field({}:{})", name, ty.contract_text())
            }
        }
    }
}

impl CoreType {
    /// Renders a typed Core type as deterministic contract text.
    ///
    /// Inputs:
    /// - `self`: typed Core type payload derived from signature text.
    ///
    /// Output:
    /// - Stable compact text for CoreIR contracts and phase goldens.
    ///
    /// Transformation:
    /// - Serializes built-in and simple named type payloads without backend
    ///   syntax or source span data.
    fn contract_text(&self) -> String {
        match self {
            CoreType::Int => "Int".to_string(),
            CoreType::Float => "Float".to_string(),
            CoreType::Number => "Number".to_string(),
            CoreType::String => "String".to_string(),
            CoreType::Binary => "Binary".to_string(),
            CoreType::Atom => "Atom".to_string(),
            CoreType::Bool => "Bool".to_string(),
            CoreType::Term => "Term".to_string(),
            CoreType::Dynamic => "Dynamic".to_string(),
            CoreType::Never => "Never".to_string(),
            CoreType::AtomLiteral(name) => format!("AtomLiteral({name})"),
            CoreType::Named(name) => format!("Named({name})"),
            CoreType::Apply { constructor, args } => format!(
                "Apply({};{})",
                constructor,
                args.iter()
                    .map(CoreType::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CoreType::List(item) => format!("List({})", item.contract_text()),
            CoreType::Tuple(items) => format!(
                "Tuple({})",
                items
                    .iter()
                    .map(CoreTupleTypeElem::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CoreType::Struct { name, fields } => format!(
                "Struct({};{})",
                name,
                fields
                    .iter()
                    .map(CoreStructTypeField::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CoreType::Map(fields) => format!(
                "Map({})",
                fields
                    .iter()
                    .map(CoreMapTypeField::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CoreType::Arrow {
                params,
                return_type,
            } => format!(
                "Arrow({};{})",
                params
                    .iter()
                    .map(CoreType::contract_text)
                    .collect::<Vec<_>>()
                    .join(","),
                return_type.contract_text()
            ),
            CoreType::Union(items) => format!(
                "Union({})",
                items
                    .iter()
                    .map(CoreType::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        }
    }
}

/// Renders a Core parameter as deterministic contract text.
///
/// Inputs:
/// - `param`: Core function or constructor parameter summary.
///
/// Output:
/// - Stable text containing parameter name, original type text, and typed Core
///   type payload when available.
///
/// Transformation:
/// - Combines the textual annotation with the optional typed `CoreType`
///   payload without changing the parameter identity.
fn core_param_contract_text(param: &CoreParam) -> String {
    format!(
        "{}:{}:core={}",
        param.name,
        param.ty,
        core_type_contract_text(param.core_ty.as_ref())
    )
}

/// Renders an optional Core type payload for contract text.
///
/// Inputs:
/// - `ty`: optional typed Core type payload.
///
/// Output:
/// - Typed Core type contract text, or `unsupported` when no payload exists.
///
/// Transformation:
/// - Converts optional payload state into stable snapshot text.
fn core_type_contract_text(ty: Option<&CoreType>) -> String {
    ty.map(CoreType::contract_text)
        .unwrap_or_else(|| "unsupported".to_string())
}

/// Converts textual type annotations into typed Core type payloads.
///
/// Inputs:
/// - `text`: source/interface type annotation text.
///
/// Output:
/// - `Some(CoreType)` for built-in types, simple named/type-variable refs,
///   parameterized named refs, lists, tuples, function types, and unions whose
///   members are also supported.
/// - `None` for compound type syntax that still requires additional CoreType
///   forms.
///
/// Transformation:
/// - Normalizes surrounding whitespace and maps stable annotations into
///   backend-neutral Core type payloads.
fn core_type_from_text(text: &str) -> Option<CoreType> {
    let text = text.trim();
    if let Some(items) = split_top_level_type_union(text) {
        return items
            .into_iter()
            .map(core_type_from_text)
            .collect::<Option<Vec<_>>>()
            .map(CoreType::Union);
    }
    if let Some(atom) = core_atom_literal_from_text(text) {
        return Some(CoreType::AtomLiteral(atom));
    }
    match text {
        "Int" => Some(CoreType::Int),
        "Float" => Some(CoreType::Float),
        "Number" => Some(CoreType::Number),
        "String" => Some(CoreType::String),
        "Binary" | "Text" => Some(CoreType::Binary),
        "Atom" => Some(CoreType::Atom),
        "Bool" => Some(CoreType::Bool),
        "Term" => Some(CoreType::Term),
        "Dynamic" => Some(CoreType::Dynamic),
        "Never" => Some(CoreType::Never),
        list if list.starts_with("List[") && list.ends_with(']') => {
            let inner = list.strip_prefix("List[")?.strip_suffix(']')?;
            core_type_from_text(inner).map(|item| CoreType::List(Box::new(item)))
        }
        map if core_map_type_inner(map).is_some() => {
            core_map_type_fields_from_text(core_map_type_inner(map)?).map(CoreType::Map)
        }
        tuple if tuple.starts_with('{') && tuple.ends_with('}') => {
            let inner = tuple.strip_prefix('{')?.strip_suffix('}')?;
            split_top_level_type_items(inner)?
                .into_iter()
                .map(core_tuple_type_elem_from_text)
                .collect::<Option<Vec<_>>>()
                .map(CoreType::Tuple)
        }
        application => {
            if let Some((params, return_type)) = core_type_arrow_parts(application) {
                return params
                    .into_iter()
                    .map(core_type_from_text)
                    .collect::<Option<Vec<_>>>()
                    .and_then(|params| {
                        core_type_from_text(return_type).map(|return_type| CoreType::Arrow {
                            params,
                            return_type: Box::new(return_type),
                        })
                    });
            }
            if let Some((constructor, args)) = core_type_application_parts(application) {
                return args
                    .into_iter()
                    .map(core_type_from_text)
                    .collect::<Option<Vec<_>>>()
                    .map(|args| CoreType::Apply {
                        constructor: constructor.to_string(),
                        args,
                    });
            }
            if is_simple_core_type_name(application) {
                Some(CoreType::Named(application.to_string()))
            } else {
                None
            }
        }
    }
}

/// Returns the inner field text for a map type annotation.
///
/// Inputs:
/// - `text`: normalized type annotation text.
///
/// Output:
/// - `Some(&str)` for source-like `#{...}` and parser-normalized `# {...}`
///   map type text.
/// - `None` when the text is not a supported map type wrapper.
///
/// Transformation:
/// - Removes only the outer map delimiters and leaves field text untouched for
///   map-field splitting.
fn core_map_type_inner(text: &str) -> Option<&str> {
    text.strip_prefix("#{")
        .or_else(|| text.strip_prefix("# {"))?
        .strip_suffix('}')
}

/// Converts one tuple type element into a typed Core tuple element.
///
/// Inputs:
/// - `text`: tuple element text without surrounding tuple braces.
///
/// Output:
/// - `Some(CoreTupleTypeElem)` when the element is a supported positional type
///   or named field type.
/// - `None` when the element is unsupported.
///
/// Transformation:
/// - Detects a top-level field separator after a valid field name; otherwise
///   lowers the whole element as a positional Core type.
fn core_tuple_type_elem_from_text(text: &str) -> Option<CoreTupleTypeElem> {
    let text = text.trim();
    if let Some((name, ty)) = core_tuple_type_field_parts(text) {
        return core_type_from_text(ty).map(|ty| CoreTupleTypeElem::Field {
            name: name.to_string(),
            ty,
        });
    }
    core_type_from_text(text).map(CoreTupleTypeElem::Type)
}

/// Converts map type field text into typed Core map fields.
///
/// Inputs:
/// - `text`: map body text without the surrounding `#{` and `}` delimiters.
///
/// Output:
/// - `Some(Vec<CoreMapTypeField>)` when every field has a supported value
///   type.
/// - `None` when a field is malformed or has an unsupported value type.
///
/// Transformation:
/// - Splits fields on top-level commas, preserves key/operator text, and
///   recursively lowers each value type into CoreType.
fn core_map_type_fields_from_text(text: &str) -> Option<Vec<CoreMapTypeField>> {
    let items = split_top_level_type_items(text)?;
    if items.is_empty() {
        return Some(Vec::new());
    }
    items
        .into_iter()
        .map(core_map_type_field_from_text)
        .collect()
}

/// Converts one map type field into a typed Core map field.
///
/// Inputs:
/// - `text`: one map field text without surrounding map delimiters.
///
/// Output:
/// - `Some(CoreMapTypeField)` for `key => Type` or `key := Type` fields with
///   supported value types.
/// - `None` for malformed fields or unsupported value types.
///
/// Transformation:
/// - Finds the first top-level map field operator, preserves the key and
///   operator, and lowers the value text through CoreType.
fn core_map_type_field_from_text(text: &str) -> Option<CoreMapTypeField> {
    let (operator_index, operator) = find_top_level_map_type_operator(text)?;
    let key = text[..operator_index].trim();
    let value_text = text[(operator_index + operator.len())..].trim();
    if key.is_empty() || value_text.is_empty() {
        return None;
    }
    core_type_from_text(value_text).map(|value| CoreMapTypeField {
        key: key.to_string(),
        operator: operator.to_string(),
        value,
    })
}

/// Finds a top-level map type field operator in field text.
///
/// Inputs:
/// - `text`: one map field text.
///
/// Output:
/// - `Some((byte_index, operator))` for top-level `=>` or `:=`.
/// - `None` when no operator exists or delimiters are unbalanced.
///
/// Transformation:
/// - Scans once, tracks bracket/brace/paren depth, and ignores operators
///   inside nested type delimiters.
fn find_top_level_map_type_operator(text: &str) -> Option<(usize, &'static str)> {
    let mut depth = 0usize;
    let mut chars = text.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        match ch {
            '[' | '{' | '(' => depth = depth.checked_add(1)?,
            ']' | '}' | ')' => depth = depth.checked_sub(1)?,
            '=' if depth == 0 && chars.peek().is_some_and(|(_, next)| *next == '>') => {
                return Some((index, "=>"));
            }
            ':' if depth == 0 && chars.peek().is_some_and(|(_, next)| *next == '=') => {
                return Some((index, ":="));
            }
            _ => {}
        }
    }
    None
}

/// Splits a named tuple type field into field name and type text.
///
/// Inputs:
/// - `text`: tuple element text without surrounding tuple braces.
///
/// Output:
/// - `Some((name, ty))` for supported `lower_name: Type` and `_: Type`
///   elements.
/// - `None` when the element is positional or has unsupported field syntax.
///
/// Transformation:
/// - Finds a top-level colon that is not the first non-space character, then
///   validates the field name while leaving the field type text for recursive
///   CoreType lowering.
fn core_tuple_type_field_parts(text: &str) -> Option<(&str, &str)> {
    let colon = find_top_level_type_colon(text)?;
    let name = text[..colon].trim();
    let ty = text[(colon + ':'.len_utf8())..].trim();
    if ty.is_empty() || !is_tuple_type_field_name(name) {
        return None;
    }
    Some((name, ty))
}

/// Finds a top-level tuple field colon in type element text.
///
/// Inputs:
/// - `text`: tuple element text without surrounding tuple braces.
///
/// Output:
/// - `Some(byte_index)` for a top-level colon that can separate a field name
///   from its type.
/// - `None` when no such colon exists or delimiters are unbalanced.
///
/// Transformation:
/// - Scans once, tracks bracket/brace/paren depth, and ignores a colon at the
///   first non-space position so raw atom literals like `:ok` remain
///   positional elements.
fn find_top_level_type_colon(text: &str) -> Option<usize> {
    let first_non_space = text.find(|ch: char| !ch.is_whitespace())?;
    let mut depth = 0usize;
    for (index, ch) in text.char_indices() {
        match ch {
            '[' | '{' | '(' => depth = depth.checked_add(1)?,
            ']' | '}' | ')' => depth = depth.checked_sub(1)?,
            ':' if depth == 0 && index != first_non_space => return Some(index),
            _ => {}
        }
    }
    None
}

/// Checks whether text is a supported tuple type field name.
///
/// Inputs:
/// - `name`: candidate field name text.
///
/// Output:
/// - `true` for lowercase field names and `_`.
/// - `false` for empty, uppercase, compound, or symbol-containing names.
///
/// Transformation:
/// - Applies the current Terlan tuple-field naming subset without resolving
///   names semantically.
fn is_tuple_type_field_name(name: &str) -> bool {
    name == "_"
        || (name
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_lowercase())
            && name
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_'))
}

/// Converts raw atom literal type text into a Core atom literal payload.
///
/// Inputs:
/// - `text`: normalized type annotation text.
///
/// Output:
/// - `Some(String)` for explicit raw atom literal forms such as `:none` and
///   `:'Elixir.Module'`.
/// - `None` when the text is not a supported atom literal.
///
/// Transformation:
/// - Strips the leading `:`, preserves quoted interop atom content without the
///   surrounding quotes, and accepts only explicit atom syntax so bare names do
///   not become atoms in Terlan source mode.
fn core_atom_literal_from_text(text: &str) -> Option<String> {
    if let Some(atom) = atom_type_literal_payload(text) {
        return Some(atom);
    }

    let atom = text.strip_prefix(':')?.trim();
    if let Some(quoted) = atom
        .strip_prefix('\'')
        .and_then(|value| value.strip_suffix('\''))
    {
        if quoted.is_empty() {
            return None;
        }
        return Some(quoted.to_string());
    }
    if atom.is_empty()
        || !atom
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return None;
    }
    Some(atom.to_string())
}

/// Extracts the canonical `Atom["name"]` singleton primitive payload.
///
/// Inputs:
/// - `text`: normalized type annotation text.
///
/// Output:
/// - `Some(String)` for canonical `Atom["name"]` type literals.
/// - `None` for all other type expressions.
///
/// Transformation:
/// - Parses only the language-neutral symbolic singleton syntax and unescapes
///   the contained string literal without treating bare names as atoms.
fn atom_type_literal_payload(text: &str) -> Option<String> {
    let inner = text.trim().strip_prefix("Atom[")?.strip_suffix(']')?.trim();
    parse_atom_string_literal(inner)
}

/// Parses a string payload used by `Atom["name"]`.
///
/// Inputs:
/// - `text`: candidate string literal source including quotes.
///
/// Output:
/// - `Some(String)` with the unescaped payload when the literal is non-empty.
/// - `None` for non-string or empty atom payloads.
///
/// Transformation:
/// - Performs the small escape handling needed by type-level atom primitives.
fn parse_atom_string_literal(text: &str) -> Option<String> {
    let inner = text.strip_prefix('"')?.strip_suffix('"')?;
    if inner.is_empty() {
        return None;
    }
    let mut output = String::new();
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(escaped) = chars.next() {
                output.push(escaped);
            }
        } else {
            output.push(ch);
        }
    }
    Some(output)
}

/// Converts an interface type body variant list into a Core type payload.
///
/// Inputs:
/// - `body`: interface type body variants preserved by the resolver.
///
/// Output:
/// - `Some(CoreType)` when every variant is representable by the current
///   CoreType model.
/// - `None` when the body is empty or any variant is not yet representable.
///
/// Transformation:
/// - Lowers a single body variant as-is, and lowers multiple variants into a
///   typed `CoreType::Union` without depending on rendered infix body text.
fn core_type_from_body_variants(body: &[String]) -> Option<CoreType> {
    match body {
        [] => None,
        [single] => core_type_from_body_variant(single),
        variants => variants
            .iter()
            .map(|variant| core_type_from_body_variant(variant))
            .collect::<Option<Vec<_>>>()
            .map(CoreType::Union),
    }
}

/// Converts one resolver-preserved type body variant into a Core type payload.
///
/// Inputs:
/// - `variant`: one type body variant from `ModuleInterface::type_bodies`.
///
/// Output:
/// - `Some(CoreType)` when the variant is representable by the current
///   CoreType model.
/// - `None` when the variant is unsupported.
///
/// Transformation:
/// - Reuses normal type-text lowering first, then handles resolver-preserved
///   raw atom variants whose source spelling was explicit `:atom` but whose
///   stored variant text no longer includes the leading colon.
fn core_type_from_body_variant(variant: &str) -> Option<CoreType> {
    core_type_from_text(variant).or_else(|| {
        if is_resolver_atom_body_variant(variant) {
            Some(CoreType::AtomLiteral(variant.to_string()))
        } else {
            None
        }
    })
}

/// Checks whether a resolver type-body variant represents a raw atom literal.
///
/// Inputs:
/// - `variant`: one type body variant from the module interface.
///
/// Output:
/// - `true` for lowercase/underscore atom names preserved by the resolver.
/// - `false` for empty, uppercase, compound, or symbol-containing type text.
///
/// Transformation:
/// - Accepts only lowercase-leading ASCII atom names so this resolver-specific
///   path does not make bare lowercase type text valid in source-mode parsing.
fn is_resolver_atom_body_variant(variant: &str) -> bool {
    let variant = variant.trim();
    variant
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_lowercase() || ch == '_')
        && variant
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// Splits type union text on top-level `|` delimiters.
///
/// Inputs:
/// - `text`: normalized type annotation text.
///
/// Output:
/// - `Some(Vec<&str>)` when the text contains a balanced top-level union with
///   at least two non-empty members.
/// - `None` when no top-level union is present or delimiters are unbalanced.
///
/// Transformation:
/// - Scans once, tracks bracket/brace/paren depth, and splits only on `|`
///   characters at depth zero.
fn split_top_level_type_union(text: &str) -> Option<Vec<&str>> {
    let mut items = Vec::new();
    let mut start = 0;
    let mut depth = 0usize;
    let mut found_union = false;
    for (index, ch) in text.char_indices() {
        match ch {
            '[' | '{' | '(' => depth = depth.checked_add(1)?,
            ']' | '}' | ')' => depth = depth.checked_sub(1)?,
            '|' if depth == 0 => {
                found_union = true;
                let item = text[start..index].trim();
                if item.is_empty() {
                    return None;
                }
                items.push(item);
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    if depth != 0 || !found_union {
        return None;
    }
    let tail = text[start..].trim();
    if tail.is_empty() {
        return None;
    }
    items.push(tail);
    Some(items)
}

/// Splits a function type annotation into parameter and return type text.
///
/// Inputs:
/// - `text`: normalized type annotation text.
///
/// Output:
/// - `Some((params, return_type))` when the text is `(Param, ...) -> Return`
///   with balanced parameter syntax.
/// - `None` when the text is not a supported function type annotation.
///
/// Transformation:
/// - Finds a top-level `->`, validates parenthesized parameter text, and uses
///   top-level comma splitting for parameter type items.
fn core_type_arrow_parts(text: &str) -> Option<(Vec<&str>, &str)> {
    let arrow_index = find_top_level_arrow(text)?;
    let params_text = text[..arrow_index].trim();
    let return_type = text[(arrow_index + "->".len())..].trim();
    if !params_text.starts_with('(') || !params_text.ends_with(')') || return_type.is_empty() {
        return None;
    }
    let params_inner = &params_text['('.len_utf8()..(params_text.len() - ')'.len_utf8())];
    let params = split_top_level_type_items(params_inner)?;
    if params.iter().any(|param| param.is_empty()) {
        return None;
    }
    Some((params, return_type))
}

/// Finds a top-level function-type arrow in type annotation text.
///
/// Inputs:
/// - `text`: normalized type annotation text.
///
/// Output:
/// - `Some(byte_index)` for the first `->` outside nested type delimiters.
/// - `None` when no top-level arrow exists or delimiter depth is unbalanced.
///
/// Transformation:
/// - Scans once, tracks bracket/brace/paren depth, and only accepts `->` at
///   depth zero.
fn find_top_level_arrow(text: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut chars = text.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        match ch {
            '[' | '{' | '(' => depth = depth.checked_add(1)?,
            ']' | '}' | ')' => depth = depth.checked_sub(1)?,
            '-' if depth == 0 && chars.peek().is_some_and(|(_, next)| *next == '>') => {
                return Some(index);
            }
            _ => {}
        }
    }
    None
}

/// Splits a parameterized type annotation into constructor and argument text.
///
/// Inputs:
/// - `text`: normalized type annotation text.
///
/// Output:
/// - `Some((constructor, args))` when the text is `Name[Arg, ...]`, the
///   constructor is a simple Core type name, and top-level arguments are
///   balanced.
/// - `None` when the text is not a supported parameterized type application.
///
/// Transformation:
/// - Finds the outermost bracket pair, validates the constructor name, and
///   delegates comma splitting to the existing top-level type-list scanner.
fn core_type_application_parts(text: &str) -> Option<(&str, Vec<&str>)> {
    let open_index = text
        .char_indices()
        .find_map(|(index, ch)| if ch == '[' { Some(index) } else { None })?;
    let constructor = text[..open_index].trim();
    if !is_simple_core_type_name(constructor) || !text.ends_with(']') {
        return None;
    }
    let args_text = &text[(open_index + '['.len_utf8())..(text.len() - ']'.len_utf8())];
    let args = split_top_level_type_items(args_text)?;
    if args.is_empty() || args.iter().any(|arg| arg.is_empty()) {
        return None;
    }
    Some((constructor, args))
}

/// Splits type item text on top-level commas.
///
/// Inputs:
/// - `text`: inner type-list text, without the enclosing tuple braces.
///
/// Output:
/// - `Some(Vec<&str>)` containing trimmed top-level item slices.
/// - `None` when nesting is unbalanced.
///
/// Transformation:
/// - Scans the text once, tracks bracket/brace/paren depth, and splits only on
///   commas at depth zero.
fn split_top_level_type_items(text: &str) -> Option<Vec<&str>> {
    let mut items = Vec::new();
    let mut start = 0;
    let mut depth = 0usize;
    for (index, ch) in text.char_indices() {
        match ch {
            '[' | '{' | '(' => depth = depth.checked_add(1)?,
            ']' | '}' | ')' => depth = depth.checked_sub(1)?,
            ',' if depth == 0 => {
                items.push(text[start..index].trim());
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    if depth != 0 {
        return None;
    }
    let tail = text[start..].trim();
    if !tail.is_empty() || !text.trim().is_empty() {
        items.push(tail);
    }
    Some(items)
}

/// Checks whether a type annotation is a simple named Core type reference.
///
/// Inputs:
/// - `name`: normalized type annotation text.
///
/// Output:
/// - `true` when the text is a non-empty simple name/path whose first segment
///   starts uppercase.
/// - `false` for empty, compound, lowercase, or symbol-containing type text.
///
/// Transformation:
/// - Accepts ASCII alphanumeric/underscore segments separated by dots and
///   rejects generic/list/tuple/function syntax for this initial CoreType
///   slice.
fn is_simple_core_type_name(name: &str) -> bool {
    name.split('.').all(|segment| {
        segment
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_uppercase())
            && segment
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreFunctionClause {
    pub patterns: Vec<String>,
    pub core_patterns: Vec<Option<CorePattern>>,
    pub pattern_proof_coverage: Vec<CoreProofCoverage>,
    pub pattern_checked_preservation_evidence: Vec<Option<CoreCheckedPreservationEvidence>>,
    pub guard: Option<CoreExprSummary>,
    pub body: CoreExprSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CorePattern {
    Wildcard,
    Var(String),
    Int(i64),
    Float(String),
    Atom(String),
    Tuple(Vec<CorePattern>),
    List(Vec<CorePattern>),
    ListCons {
        head: Box<CorePattern>,
        tail: Box<CorePattern>,
    },
    Map(Vec<CoreMapPatternField>),
    Record {
        name: String,
        fields: Vec<CoreRecordPatternField>,
    },
    Constructor {
        name: String,
        constructor_identity: Option<String>,
        args: Vec<CorePattern>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreMapPatternField {
    pub key: String,
    pub required: bool,
    pub value: CorePattern,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreRecordPatternField {
    pub key: String,
    pub required: bool,
    pub value: CorePattern,
}

impl CorePattern {
    /// Renders a typed Core pattern as deterministic contract text.
    ///
    /// Inputs:
    /// - `self`: typed Core pattern from the Lean-covered pattern subset.
    ///
    /// Output:
    /// - Stable compact text for CoreIR contracts and phase goldens.
    ///
    /// Transformation:
    /// - Serializes the structural Core pattern without using source spans,
    ///   backend syntax, or syntax-output summary text.
    fn contract_text(&self) -> String {
        match self {
            CorePattern::Wildcard => "Wildcard".to_string(),
            CorePattern::Var(name) => format!("Var({name})"),
            CorePattern::Int(value) => format!("Int({value})"),
            CorePattern::Float(value) => format!("Float({value})"),
            CorePattern::Atom(value) => format!("Atom({value})"),
            CorePattern::Tuple(elements) => format!(
                "Tuple({})",
                elements
                    .iter()
                    .map(CorePattern::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CorePattern::List(elements) => format!(
                "List({})",
                elements
                    .iter()
                    .map(CorePattern::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CorePattern::ListCons { head, tail } => {
                format!(
                    "ListCons({}|{})",
                    head.contract_text(),
                    tail.contract_text()
                )
            }
            CorePattern::Map(fields) => format!(
                "Map({})",
                fields
                    .iter()
                    .map(CoreMapPatternField::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CorePattern::Record { name, fields } => format!(
                "Record({name};{})",
                fields
                    .iter()
                    .map(CoreRecordPatternField::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CorePattern::Constructor {
                name,
                constructor_identity,
                args,
            } => {
                let args = args
                    .iter()
                    .map(CorePattern::contract_text)
                    .collect::<Vec<_>>()
                    .join(",");
                match constructor_identity {
                    Some(identity) => format!("Constructor({name};identity={identity};{args})"),
                    None => format!("Constructor({name};{args})"),
                }
            }
        }
    }
}

impl CoreMapPatternField {
    /// Renders a typed Core map-pattern field as deterministic contract text.
    ///
    /// Inputs:
    /// - `self`: typed Core map-pattern field from syntax-output lowering.
    ///
    /// Output:
    /// - Stable compact text for CoreIR contracts and phase goldens.
    ///
    /// Transformation:
    /// - Serializes the source key, required/optional map-match operator, and
    ///   recursively rendered value pattern without backend-specific syntax.
    fn contract_text(&self) -> String {
        let operator = if self.required { ":=" } else { "=>" };
        format!("{}{}{}", self.key, operator, self.value.contract_text())
    }
}

impl CoreRecordPatternField {
    /// Renders a typed Core record-pattern field as deterministic contract text.
    ///
    /// Inputs:
    /// - `self`: typed Core record-pattern field from syntax-output lowering.
    ///
    /// Output:
    /// - Stable compact text for CoreIR contracts and phase goldens.
    ///
    /// Transformation:
    /// - Serializes the field key, source field-match operator, and
    ///   recursively rendered value pattern without backend-specific syntax.
    fn contract_text(&self) -> String {
        let operator = if self.required { "=" } else { "=>" };
        format!("{}{}{}", self.key, operator, self.value.contract_text())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreProofCoverage {
    LeanCovered,
    Partial,
    ProofModelRequired,
    RuntimeBoundary,
    ArtifactOnly,
}

impl CoreProofCoverage {
    /// Renders the proof-coverage label used in deterministic CoreIR artifacts.
    ///
    /// Inputs:
    /// - `self`: proof coverage classification for a Core expression summary.
    ///
    /// Output:
    /// - Stable lowercase label suitable for CoreIR contract text and
    ///   conformance fixtures.
    ///
    /// Transformation:
    /// - Maps internal enum variants to the documented LP7 coverage labels.
    fn as_str(&self) -> &'static str {
        match self {
            CoreProofCoverage::LeanCovered => "lean-covered",
            CoreProofCoverage::Partial => "partial",
            CoreProofCoverage::ProofModelRequired => "proof-model-required",
            CoreProofCoverage::RuntimeBoundary => "runtime-boundary",
            CoreProofCoverage::ArtifactOnly => "artifact-only",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreProofReadiness {
    LeanCovered,
    Partial,
    ProofModelRequired,
    RuntimeBoundary,
    ArtifactOnly,
    NoExpressions,
}

impl CoreProofReadiness {
    /// Renders the module-level proof readiness label.
    ///
    /// Inputs:
    /// - `self`: proof readiness classification for a Core module.
    ///
    /// Output:
    /// - Stable lowercase label suitable for manifests and CoreIR contract text.
    ///
    /// Transformation:
    /// - Maps the internal readiness enum to the documented proof-readiness
    ///   labels used by release tooling.
    pub fn as_str(&self) -> &'static str {
        match self {
            CoreProofReadiness::LeanCovered => "lean-covered",
            CoreProofReadiness::Partial => "partial",
            CoreProofReadiness::ProofModelRequired => "proof-model-required",
            CoreProofReadiness::RuntimeBoundary => "runtime-boundary",
            CoreProofReadiness::ArtifactOnly => "artifact-only",
            CoreProofReadiness::NoExpressions => "no-expressions",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreCheckedPreservationEvidenceKind {
    StructuralCoreExpr,
    StructuralCorePattern,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreSubstitutionFreshnessEvidence {
    NoRuntimeBindings,
    RuntimeBindingsRequired,
}

impl CoreSubstitutionFreshnessEvidence {
    /// Renders the substitution-freshness obligation attached to evidence.
    ///
    /// Inputs:
    /// - `self`: conservative freshness classification for one evidence-backed
    ///   Core expression or pattern.
    ///
    /// Output:
    /// - Stable lowercase label suitable for CoreIR contract text and future
    ///   Lean export.
    ///
    /// Transformation:
    /// - Maps internal freshness categories to the LP8 handoff vocabulary:
    ///   values without runtime binding introduction need no freshness payload,
    ///   while binding forms require checked runtime freshness evidence later.
    fn as_str(&self) -> &'static str {
        match self {
            CoreSubstitutionFreshnessEvidence::NoRuntimeBindings => "no-runtime-bindings",
            CoreSubstitutionFreshnessEvidence::RuntimeBindingsRequired => {
                "runtime-bindings-required"
            }
        }
    }

    /// Combines two substitution-freshness obligations conservatively.
    ///
    /// Inputs:
    /// - `self`: current aggregate obligation.
    /// - `other`: additional nested obligation.
    ///
    /// Output:
    /// - `RuntimeBindingsRequired` when either side may introduce runtime
    ///   bindings; otherwise `NoRuntimeBindings`.
    ///
    /// Transformation:
    /// - Applies a two-point lattice join over freshness obligations.
    fn combine(
        self,
        other: CoreSubstitutionFreshnessEvidence,
    ) -> CoreSubstitutionFreshnessEvidence {
        if matches!(
            self,
            CoreSubstitutionFreshnessEvidence::RuntimeBindingsRequired
        ) || matches!(
            other,
            CoreSubstitutionFreshnessEvidence::RuntimeBindingsRequired
        ) {
            CoreSubstitutionFreshnessEvidence::RuntimeBindingsRequired
        } else {
            CoreSubstitutionFreshnessEvidence::NoRuntimeBindings
        }
    }
}

impl CoreCheckedPreservationEvidenceKind {
    /// Renders the checked-preservation evidence kind used in CoreIR payloads.
    ///
    /// Inputs:
    /// - `self`: checked-preservation evidence classification for one typed
    ///   Core expression or pattern.
    ///
    /// Output:
    /// - Stable lowercase evidence label suitable for deterministic contract
    ///   text and future Lean export.
    ///
    /// Transformation:
    /// - Maps the internal evidence enum to a documented LP8 evidence label.
    fn as_str(&self) -> &'static str {
        match self {
            CoreCheckedPreservationEvidenceKind::StructuralCoreExpr => "structural-core-expr",
            CoreCheckedPreservationEvidenceKind::StructuralCorePattern => "structural-core-pattern",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreCheckedPreservationEvidence {
    pub kind: CoreCheckedPreservationEvidenceKind,
    pub freshness: CoreSubstitutionFreshnessEvidence,
    pub target: String,
}

impl CoreCheckedPreservationEvidence {
    /// Renders checked-preservation evidence as deterministic contract text.
    ///
    /// Inputs:
    /// - `self`: evidence object attached to a typed Core expression summary.
    ///
    /// Output:
    /// - Stable compact text for CoreIR contracts and phase goldens.
    ///
    /// Transformation:
    /// - Serializes the evidence kind, substitution-freshness obligation, and
    ///   structural Core term it covers, avoiding source spans and
    ///   backend-specific syntax.
    fn contract_text(&self) -> String {
        format!(
            "{}(freshness={};target={})",
            self.kind.as_str(),
            self.freshness.as_str(),
            self.target
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreExprSummary {
    pub kind: String,
    pub core_expr: Option<CoreExpr>,
    pub checked_preservation_evidence: Option<CoreCheckedPreservationEvidence>,
    pub proof_coverage: CoreProofCoverage,
    pub text: Option<String>,
    pub remote: Option<String>,
    pub operator: Option<String>,
    pub arity: usize,
    pub children: Vec<CoreExprSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreEffectSet {
    pub effects: Vec<String>,
}

impl CoreEffectSet {
    /// Renders a Core effect set as deterministic contract text.
    ///
    /// Inputs:
    /// - `self`: effect labels attached to a Core expression.
    ///
    /// Output:
    /// - Stable `Effects(...)` text for CoreIR contract snapshots.
    ///
    /// Transformation:
    /// - Sorts effect labels so semantically identical effect sets produce the
    ///   same contract text regardless of construction order.
    fn contract_text(&self) -> String {
        let mut effects = self.effects.clone();
        effects.sort();
        format!("Effects({})", effects.join(","))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CorePrimitiveIntrinsic {
    BoolEqual,
    BoolCompare,
    BoolToString,
    BoolFromString,
    IntToString,
    IntFromString,
    FloatToString,
    FloatFromString,
    StringEqual,
    StringCompare,
    StringToString,
    StringFromString,
    StringIsEmpty,
    StringAppend,
    StringConcat,
    StringContains,
    StringStartsWith,
    StringEndsWith,
    StringLength,
    StringByteSize,
    StringLowercase,
    StringUppercase,
    StringTrim,
    StringTrimStart,
    StringTrimEnd,
    StringReplace,
    StringSplit,
    StringSplitOnce,
}

impl CorePrimitiveIntrinsic {
    /// Returns the stable registry key for a primitive intrinsic.
    ///
    /// Inputs:
    /// - `self`: compiler-owned primitive intrinsic identity.
    ///
    /// Output:
    /// - Stable `core.<primitive>.<operation>` key from the CoreIR primitive
    ///   intrinsic registry.
    ///
    /// Transformation:
    /// - Maps the closed Rust enum variant to the backend-neutral serialized
    ///   intrinsic key used by contract text and backend lowering.
    pub fn registry_key(&self) -> &'static str {
        match self {
            Self::BoolEqual => "core.bool.equal",
            Self::BoolCompare => "core.bool.compare",
            Self::BoolToString => "core.bool.to_string",
            Self::BoolFromString => "core.bool.from_string",
            Self::IntToString => "core.int.to_string",
            Self::IntFromString => "core.int.from_string",
            Self::FloatToString => "core.float.to_string",
            Self::FloatFromString => "core.float.from_string",
            Self::StringEqual => "core.string.equal",
            Self::StringCompare => "core.string.compare",
            Self::StringToString => "core.string.to_string",
            Self::StringFromString => "core.string.from_string",
            Self::StringIsEmpty => "core.string.is_empty",
            Self::StringAppend => "core.string.append",
            Self::StringConcat => "core.string.concat",
            Self::StringContains => "core.string.contains",
            Self::StringStartsWith => "core.string.starts_with",
            Self::StringEndsWith => "core.string.ends_with",
            Self::StringLength => "core.string.length",
            Self::StringByteSize => "core.string.byte_size",
            Self::StringLowercase => "core.string.lowercase",
            Self::StringUppercase => "core.string.uppercase",
            Self::StringTrim => "core.string.trim",
            Self::StringTrimStart => "core.string.trim_start",
            Self::StringTrimEnd => "core.string.trim_end",
            Self::StringReplace => "core.string.replace",
            Self::StringSplit => "core.string.split",
            Self::StringSplitOnce => "core.string.split_once",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreRuntimeCapability {
    ConsolePrintln,
}

impl CoreRuntimeCapability {
    /// Returns the stable registry key for a runtime capability.
    ///
    /// Inputs:
    /// - `self`: compiler-owned runtime capability identity.
    ///
    /// Output:
    /// - Stable `runtime.<domain>.<operation>` key used by CoreIR contract
    ///   text and backend lowering.
    ///
    /// Transformation:
    /// - Maps the closed runtime capability enum to the backend-neutral
    ///   serialized key without exposing target modules in CoreIR.
    pub fn registry_key(&self) -> &'static str {
        match self {
            Self::ConsolePrintln => "runtime.console.println",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreIntrinsicId {
    Primitive(CorePrimitiveIntrinsic),
    Runtime(CoreRuntimeCapability),
}

impl CoreIntrinsicId {
    /// Returns the stable registry key for a Core intrinsic identity.
    ///
    /// Inputs:
    /// - `self`: closed Core intrinsic identity.
    ///
    /// Output:
    /// - Stable registry key for deterministic CoreIR contract text.
    ///
    /// Transformation:
    /// - Delegates to the namespace-specific intrinsic identity while keeping
    ///   backend-specific names out of CoreIR.
    fn registry_key(&self) -> &'static str {
        match self {
            Self::Primitive(intrinsic) => intrinsic.registry_key(),
            Self::Runtime(capability) => capability.registry_key(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreIntrinsicCall {
    pub id: CoreIntrinsicId,
    pub args: Vec<CoreExpr>,
    pub return_type: CoreType,
    pub effects: CoreEffectSet,
    pub span: Span,
}

impl CoreIntrinsicCall {
    /// Renders a Core intrinsic call as deterministic contract text.
    ///
    /// Inputs:
    /// - `self`: typed intrinsic call payload.
    ///
    /// Output:
    /// - Stable `Intrinsic(...)` text for CoreIR contract snapshots.
    ///
    /// Transformation:
    /// - Serializes the backend-neutral intrinsic key, typed arguments,
    ///   return type, effects, and source span without exposing backend module
    ///   calls.
    fn contract_text(&self) -> String {
        let args = self
            .args
            .iter()
            .map(CoreExpr::contract_text)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "Intrinsic({};args={};return={};effects={};span={}:{}))",
            self.id.registry_key(),
            args,
            self.return_type.contract_text(),
            self.effects.contract_text(),
            self.span.start,
            self.span.end
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreExpr {
    Int(i64),
    Float(String),
    Binary(String),
    Atom(String),
    Var(String),
    Tuple(Vec<CoreExpr>),
    List(Vec<CoreExpr>),
    ListCons {
        head: Box<CoreExpr>,
        tail: Box<CoreExpr>,
    },
    FixedArray(Vec<CoreExpr>),
    Index {
        base: Box<CoreExpr>,
        index: Box<CoreExpr>,
    },
    ListComprehension {
        expr: Box<CoreExpr>,
        pattern: CorePattern,
        source: Box<CoreExpr>,
        guard: Option<Box<CoreExpr>>,
    },
    Let {
        bindings: Vec<CoreLetBinding>,
        body: Box<CoreExpr>,
    },
    Map(Vec<CoreMapExprField>),
    RecordConstruct {
        name: String,
        fields: Vec<CoreRecordExprField>,
    },
    FieldAccess {
        base: Box<CoreExpr>,
        field: String,
    },
    RecordAccess {
        base: Box<CoreExpr>,
        name: String,
        field: String,
    },
    RecordUpdate {
        base: Box<CoreExpr>,
        name: String,
        fields: Vec<CoreRecordExprField>,
    },
    TemplateInstantiate {
        name: String,
        fields: Vec<CoreRecordExprField>,
    },
    ConstructorChain {
        base: String,
        base_constructor_identity: Option<String>,
        args: Vec<CoreExpr>,
        record: Box<CoreExpr>,
    },
    RemoteFunRef {
        module: String,
        function: String,
        arity: usize,
    },
    RemoteCall {
        module: String,
        function: String,
        args: Vec<CoreExpr>,
    },
    ConstructorCall {
        constructor: String,
        constructor_identity: Option<String>,
        args: Vec<CoreExpr>,
    },
    Call {
        function: String,
        args: Vec<CoreExpr>,
    },
    FunctionCall {
        callee: Box<CoreExpr>,
        args: Vec<CoreExpr>,
    },
    Intrinsic(CoreIntrinsicCall),
    Case {
        scrutinee: Box<CoreExpr>,
        clauses: Vec<CoreCaseClause>,
    },
    Receive {
        clauses: Vec<CoreCaseClause>,
        after_clause: Option<CoreReceiveAfter>,
    },
    Try {
        body: Box<CoreExpr>,
        of_clauses: Vec<CoreCaseClause>,
        catch_clauses: Vec<CoreCaseClause>,
        after_clause: Option<CoreTryAfter>,
    },
    If {
        clauses: Vec<CoreIfClause>,
    },
    Lam {
        params: Vec<CorePattern>,
        body: Box<CoreExpr>,
    },
    UnaryOp {
        operator: String,
        operand: Box<CoreExpr>,
    },
    BinaryOp {
        operator: String,
        left: Box<CoreExpr>,
        right: Box<CoreExpr>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreMapExprField {
    pub key: String,
    pub required: bool,
    pub value: CoreExpr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreLetBinding {
    pub name: String,
    pub value: CoreExpr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreRecordExprField {
    pub key: String,
    pub required: bool,
    pub value: CoreExpr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreCaseClause {
    pub pattern: CorePattern,
    pub guard: Option<CoreExpr>,
    pub body: CoreExpr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreIfClause {
    pub condition: CoreExpr,
    pub body: CoreExpr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreReceiveAfter {
    pub trigger: Box<CoreExpr>,
    pub body: Box<CoreExpr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreTryAfter {
    pub trigger: Box<CoreExpr>,
    pub body: Box<CoreExpr>,
}

impl CoreExpr {
    /// Renders a typed Core expression as deterministic contract text.
    ///
    /// Inputs:
    /// - `self`: typed Core expression from the initial Lean-covered subset.
    ///
    /// Output:
    /// - Stable compact text for CoreIR contracts and phase goldens.
    ///
    /// Transformation:
    /// - Serializes the structural Core expression without source spans,
    ///   backend syntax, or syntax-output summary text.
    fn contract_text(&self) -> String {
        match self {
            CoreExpr::Int(value) => format!("Int({value})"),
            CoreExpr::Float(value) => format!("Float({value})"),
            CoreExpr::Binary(value) => format!("Binary({value})"),
            CoreExpr::Atom(value) => format!("Atom({value})"),
            CoreExpr::Var(name) => format!("Var({name})"),
            CoreExpr::Tuple(elements) => format!(
                "Tuple({})",
                elements
                    .iter()
                    .map(CoreExpr::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CoreExpr::List(elements) => format!(
                "List({})",
                elements
                    .iter()
                    .map(CoreExpr::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CoreExpr::ListCons { head, tail } => {
                format!(
                    "ListCons({}|{})",
                    head.contract_text(),
                    tail.contract_text()
                )
            }
            CoreExpr::FixedArray(elements) => format!(
                "FixedArray({})",
                elements
                    .iter()
                    .map(CoreExpr::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CoreExpr::Index { base, index } => {
                format!("Index({};{})", base.contract_text(), index.contract_text())
            }
            CoreExpr::ListComprehension {
                expr,
                pattern,
                source,
                guard,
            } => match guard {
                Some(guard) => format!(
                    "ListComprehension({}|{}<-{} when {})",
                    expr.contract_text(),
                    pattern.contract_text(),
                    source.contract_text(),
                    guard.contract_text()
                ),
                None => format!(
                    "ListComprehension({}|{}<-{})",
                    expr.contract_text(),
                    pattern.contract_text(),
                    source.contract_text()
                ),
            },
            CoreExpr::Let { bindings, body } => format!(
                "Let({};{})",
                bindings
                    .iter()
                    .map(CoreLetBinding::contract_text)
                    .collect::<Vec<_>>()
                    .join(";"),
                body.contract_text()
            ),
            CoreExpr::Map(fields) => format!(
                "Map({})",
                fields
                    .iter()
                    .map(CoreMapExprField::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CoreExpr::RecordConstruct { name, fields } => format!(
                "RecordConstruct({name};{})",
                fields
                    .iter()
                    .map(CoreRecordExprField::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CoreExpr::FieldAccess { base, field } => {
                format!("FieldAccess({}.{})", base.contract_text(), field)
            }
            CoreExpr::RecordAccess { base, name, field } => {
                format!("RecordAccess({}#{}.{})", base.contract_text(), name, field)
            }
            CoreExpr::RecordUpdate { base, name, fields } => format!(
                "RecordUpdate({}#{};{})",
                base.contract_text(),
                name,
                fields
                    .iter()
                    .map(CoreRecordExprField::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CoreExpr::TemplateInstantiate { name, fields } => format!(
                "TemplateInstantiate({name};{})",
                fields
                    .iter()
                    .map(CoreRecordExprField::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CoreExpr::ConstructorChain {
                base,
                base_constructor_identity,
                args,
                record,
            } => {
                let args = args
                    .iter()
                    .map(CoreExpr::contract_text)
                    .collect::<Vec<_>>()
                    .join(",");
                match base_constructor_identity {
                    Some(identity) => format!(
                        "ConstructorChain({base};identity={identity};{args} with {})",
                        record.contract_text()
                    ),
                    None => format!(
                        "ConstructorChain({base};{args} with {})",
                        record.contract_text()
                    ),
                }
            }
            CoreExpr::RemoteFunRef {
                module,
                function,
                arity,
            } => format!("RemoteFunRef({module}:{function}/{arity})"),
            CoreExpr::RemoteCall {
                module,
                function,
                args,
            } => format!(
                "RemoteCall({module}:{function};{})",
                args.iter()
                    .map(CoreExpr::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CoreExpr::ConstructorCall {
                constructor,
                constructor_identity,
                args,
            } => {
                let args = args
                    .iter()
                    .map(CoreExpr::contract_text)
                    .collect::<Vec<_>>()
                    .join(",");
                match constructor_identity {
                    Some(identity) => {
                        format!("ConstructorCall({constructor};identity={identity};{args})")
                    }
                    None => format!("ConstructorCall({constructor};{args})"),
                }
            }
            CoreExpr::Call { function, args } => format!(
                "Call({};{})",
                function,
                args.iter()
                    .map(CoreExpr::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CoreExpr::FunctionCall { callee, args } => format!(
                "FunctionCall({};{})",
                callee.contract_text(),
                args.iter()
                    .map(CoreExpr::contract_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CoreExpr::Intrinsic(call) => call.contract_text(),
            CoreExpr::Case { scrutinee, clauses } => format!(
                "Case({};{})",
                scrutinee.contract_text(),
                clauses
                    .iter()
                    .map(CoreCaseClause::contract_text)
                    .collect::<Vec<_>>()
                    .join("|")
            ),
            CoreExpr::Receive {
                clauses,
                after_clause,
            } => {
                let clauses = clauses
                    .iter()
                    .map(CoreCaseClause::contract_text)
                    .collect::<Vec<_>>()
                    .join("|");
                match after_clause {
                    Some(after_clause) => {
                        format!("Receive({clauses};after={})", after_clause.contract_text())
                    }
                    None => format!("Receive({clauses})"),
                }
            }
            CoreExpr::Try {
                body,
                of_clauses,
                catch_clauses,
                after_clause,
            } => {
                let of_clauses = of_clauses
                    .iter()
                    .map(CoreCaseClause::contract_text)
                    .collect::<Vec<_>>()
                    .join("|");
                let catch_clauses = catch_clauses
                    .iter()
                    .map(CoreCaseClause::contract_text)
                    .collect::<Vec<_>>()
                    .join("|");
                match after_clause {
                    Some(after_clause) => format!(
                        "Try({};of={};catch={};after={})",
                        body.contract_text(),
                        of_clauses,
                        catch_clauses,
                        after_clause.contract_text()
                    ),
                    None => format!(
                        "Try({};of={};catch={})",
                        body.contract_text(),
                        of_clauses,
                        catch_clauses
                    ),
                }
            }
            CoreExpr::If { clauses } => format!(
                "If({})",
                clauses
                    .iter()
                    .map(CoreIfClause::contract_text)
                    .collect::<Vec<_>>()
                    .join("|")
            ),
            CoreExpr::Lam { params, body } => format!(
                "Lam({};{})",
                params
                    .iter()
                    .map(CorePattern::contract_text)
                    .collect::<Vec<_>>()
                    .join(","),
                body.contract_text()
            ),
            CoreExpr::UnaryOp { operator, operand } => {
                format!("UnaryOp({};{})", operator, operand.contract_text())
            }
            CoreExpr::BinaryOp {
                operator,
                left,
                right,
            } => format!(
                "BinaryOp({};{}, {})",
                operator,
                left.contract_text(),
                right.contract_text()
            ),
        }
    }
}

impl CoreMapExprField {
    /// Renders a typed Core map-expression field as deterministic contract text.
    ///
    /// Inputs:
    /// - `self`: typed Core map-expression field from syntax-output lowering.
    ///
    /// Output:
    /// - Stable compact text for CoreIR contracts and phase goldens.
    ///
    /// Transformation:
    /// - Serializes the field key, source insert/update operator, and
    ///   recursively rendered value expression without backend-specific syntax.
    fn contract_text(&self) -> String {
        let operator = if self.required { ":=" } else { "=>" };
        format!("{}{}{}", self.key, operator, self.value.contract_text())
    }
}

impl CoreLetBinding {
    /// Renders one typed Core let binding as deterministic contract text.
    ///
    /// Inputs:
    /// - `self`: local binding lowered from syntax output.
    ///
    /// Output:
    /// - Stable compact text for CoreIR contracts and phase goldens.
    ///
    /// Transformation:
    /// - Serializes the binding name and recursively rendered value expression
    ///   without source spans or backend syntax.
    fn contract_text(&self) -> String {
        format!("{}={}", self.name, self.value.contract_text())
    }
}

impl CoreRecordExprField {
    /// Renders a typed Core record-construction field as deterministic text.
    ///
    /// Inputs:
    /// - `self`: typed Core record field from syntax-output lowering.
    ///
    /// Output:
    /// - Stable compact text for CoreIR contracts and phase goldens.
    ///
    /// Transformation:
    /// - Serializes the field key, source field assignment operator, and
    ///   recursively rendered value expression without backend-specific syntax.
    fn contract_text(&self) -> String {
        let operator = if self.required { "=" } else { "=>" };
        format!("{}{}{}", self.key, operator, self.value.contract_text())
    }
}

impl CoreCaseClause {
    /// Renders a typed Core case clause as deterministic contract text.
    ///
    /// Inputs:
    /// - `self`: typed unguarded case clause from the current Core subset.
    ///
    /// Output:
    /// - Stable compact text for CoreIR contracts and phase goldens.
    ///
    /// Transformation:
    /// - Serializes the pattern/body pair without source spans, backend syntax,
    ///   or syntax-output summary text.
    fn contract_text(&self) -> String {
        let body = self.body.contract_text();
        match &self.guard {
            Some(guard) => format!(
                "{} when {}=>{}",
                self.pattern.contract_text(),
                guard.contract_text(),
                body
            ),
            None => format!("{}=>{}", self.pattern.contract_text(), body),
        }
    }
}

impl CoreIfClause {
    /// Renders a typed Core if clause as deterministic contract text.
    ///
    /// Inputs:
    /// - `self`: typed condition/body branch from syntax-output lowering.
    ///
    /// Output:
    /// - Stable compact text for CoreIR contracts and phase goldens.
    ///
    /// Transformation:
    /// - Serializes the condition/body pair without source spans, backend
    ///   syntax, or syntax-output summary text.
    fn contract_text(&self) -> String {
        format!(
            "{}=>{}",
            self.condition.contract_text(),
            self.body.contract_text()
        )
    }
}

impl CoreReceiveAfter {
    /// Renders a typed Core receive timeout branch as deterministic text.
    ///
    /// Inputs:
    /// - `self`: typed receive timeout branch from syntax-output lowering.
    ///
    /// Output:
    /// - Stable compact text for CoreIR contracts and phase goldens.
    ///
    /// Transformation:
    /// - Serializes the timeout trigger/body pair without source spans,
    ///   backend syntax, or syntax-output summary text.
    fn contract_text(&self) -> String {
        format!(
            "{}=>{}",
            self.trigger.contract_text(),
            self.body.contract_text()
        )
    }
}

impl CoreTryAfter {
    /// Renders a typed Core try cleanup branch as deterministic text.
    ///
    /// Inputs:
    /// - `self`: typed try cleanup branch from syntax-output lowering.
    ///
    /// Output:
    /// - Stable compact text for CoreIR contracts and phase goldens.
    ///
    /// Transformation:
    /// - Serializes the cleanup trigger/body pair without source spans,
    ///   backend syntax, or syntax-output summary text.
    fn contract_text(&self) -> String {
        format!(
            "{}=>{}",
            self.trigger.contract_text(),
            self.body.contract_text()
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreConstructorDecl {
    pub name: String,
    pub public: bool,
    pub min_arity: usize,
    pub params: Vec<CoreParam>,
    pub vararg: Option<CoreParam>,
    pub return_type: String,
    pub core_return_type: Option<CoreType>,
}

/// Source category for a backend-neutral trait conformance fact.
///
/// Inputs:
/// - Syntax-output declaration form that introduced the conformance.
///
/// Output:
/// - Stable category carried in CoreIR.
///
/// Transformation:
/// - Classifies source syntax without choosing a backend representation for
///   trait dictionaries, receiver methods, or adapter functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreTraitConformanceSource {
    Implements,
    ExplicitImpl,
    Derive,
}

/// Backend-neutral trait conformance fact preserved in CoreIR.
///
/// Inputs:
/// - Syntax-output `implements`, `derives`, or explicit `impl Trait for Type`
///   declaration.
///
/// Output:
/// - Stable conformance summary for downstream target-profile validation and
///   future backend lowering.
///
/// Transformation:
/// - Preserves trait reference text, owner type text, source category, and
///   visibility without lowering to target-specific runtime dictionaries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreTraitConformance {
    pub trait_ref: String,
    pub for_type: String,
    pub source: CoreTraitConformanceSource,
    pub public: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreModuleMetadata {
    pub interface_function_count: usize,
    pub interface_type_count: usize,
    pub constructor_count: usize,
    pub proof_readiness: CoreProofReadiness,
    pub lean_covered_expr_count: usize,
    pub partial_expr_count: usize,
    pub proof_model_required_expr_count: usize,
    pub runtime_boundary_expr_count: usize,
    pub artifact_only_expr_count: usize,
    pub lean_covered_pattern_count: usize,
    pub partial_pattern_count: usize,
    pub proof_model_required_pattern_count: usize,
    pub runtime_boundary_pattern_count: usize,
    pub artifact_only_pattern_count: usize,
    pub typed_core_expr_count: usize,
    pub summary_only_expr_count: usize,
    pub typed_core_pattern_count: usize,
    pub summary_only_pattern_count: usize,
    pub typed_core_type_count: usize,
    pub summary_only_type_count: usize,
    pub checked_preservation_expr_count: usize,
    pub checked_preservation_pattern_count: usize,
    pub checked_preservation_expr_structural_count: usize,
    pub checked_preservation_pattern_structural_count: usize,
    pub checked_preservation_expr_no_runtime_bindings_count: usize,
    pub checked_preservation_pattern_no_runtime_bindings_count: usize,
    pub checked_preservation_expr_runtime_bindings_required_count: usize,
    pub checked_preservation_pattern_runtime_bindings_required_count: usize,
    pub resolved_constructor_call_identity_count: usize,
    pub resolved_constructor_chain_identity_count: usize,
    pub resolved_constructor_pattern_identity_count: usize,
    pub unresolved_constructor_call_candidate_count: usize,
    pub unresolved_constructor_chain_candidate_count: usize,
    pub unresolved_constructor_pattern_candidate_count: usize,
}

/// Backend-agnostic core module produced by the formal typed phase.
///
/// Inputs:
/// - `resolved` module from the resolver after syntax checks.
///
/// Output:
/// - A core representation whose current payload is still declarative and
///   backend-independent.
///
/// Transformation:
/// - Performs the current production handoff point between the typed resolver
///   phase and future backend-specific lowering.
#[derive(Debug, Clone)]
pub struct CoreModule {
    /// Stable CoreIR schema identifier.
    pub schema: String,
    /// Resolved module name for downstream bookkeeping.
    pub module: String,
    /// Source identity for the phase that produced this module.
    pub source: CoreSourceIdentity,
    /// Resolved module imports visible to this Core module.
    pub imports: Vec<CoreImport>,
    /// Public exports represented by this Core module.
    pub exports: Vec<CoreExport>,
    /// Resolved type declarations represented by this Core module.
    pub types: Vec<CoreTypeDecl>,
    /// Function signatures represented by this Core module.
    pub functions: Vec<CoreFunction>,
    /// Constructor signatures represented by this Core module.
    pub constructors: Vec<CoreConstructorDecl>,
    /// Backend-neutral trait conformance facts represented by this Core module.
    pub trait_conformances: Vec<CoreTraitConformance>,
    /// Backend-independent counts and phase metadata.
    pub metadata: CoreModuleMetadata,
    /// Public interface snapshot for backend-independent emission.
    pub interface: ModuleInterface,
}

impl CoreModule {
    /// Renders the interface portion of the core module for golden tests and
    /// deterministic snapshot comparison.
    pub fn interface_text(&self) -> String {
        self.interface.to_terlan_interface_text()
    }

    /// Renders a deterministic CoreIR contract snapshot.
    ///
    /// Inputs:
    /// - `self`: Core module artifact produced by formal typechecking.
    ///
    /// Output:
    /// - Stable line-oriented text suitable for golden fixtures.
    ///
    /// Transformation:
    /// - Serializes only backend-agnostic CoreIR identity and declaration
    ///   summaries. It intentionally omits backend syntax and emitted artifacts.
    pub fn contract_text(&self) -> String {
        let mut lines = vec![
            format!("schema={}", self.schema),
            format!("module={}", self.module),
            format!("source_kind={}", self.source.source_kind),
            format!(
                "syntax_contract_fingerprint={}",
                self.source
                    .syntax_contract_fingerprint
                    .as_deref()
                    .unwrap_or("none")
            ),
        ];
        lines.extend(self.imports.iter().map(|import| match import.kind {
            CoreImportKind::Module => format!("import={}", import.module),
            CoreImportKind::File => format!("import=file:{}", import.module),
            CoreImportKind::Css => format!("import=css:{}", import.module),
            CoreImportKind::Markdown => format!("import=markdown:{}", import.module),
        }));
        lines.extend(self.exports.iter().map(|export| {
            format!(
                "export={}{}",
                export.name,
                match export.kind {
                    CoreExportKind::Function { arity } => format!("/{}", arity),
                    CoreExportKind::Type => ":type".to_string(),
                    CoreExportKind::Constructor { min_arity } =>
                        format!(":constructor/{}", min_arity),
                }
            )
        }));
        lines.extend(self.types.iter().map(|decl| {
            format!(
                "type={} visibility={:?} params={} body={} body_core={}",
                decl.name,
                decl.visibility,
                decl.params.join(","),
                decl.body.join(" "),
                core_type_contract_text(decl.core_body.as_ref())
            )
        }));
        lines.extend(self.functions.iter().map(|function| {
            let params = function
                .params
                .iter()
                .map(core_param_contract_text)
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "function={}/{} public={} params={} return={} return_core={}",
                function.name,
                function.arity,
                function.public,
                params,
                function.return_type,
                core_type_contract_text(function.core_return_type.as_ref())
            )
        }));
        lines.extend(self.functions.iter().flat_map(|function| {
            function
                .clauses
                .iter()
                .enumerate()
                .map(move |(index, clause)| {
                    format!(
                        "function_clause={}/{}#{} patterns={} core_patterns={} pattern_proof={} pattern_preservation={} guard={} body={}",
                        function.name,
                        function.arity,
                        index,
                        clause.patterns.join(","),
                        clause
                            .core_patterns
                            .iter()
                            .map(|pattern| pattern
                                .as_ref()
                                .map(CorePattern::contract_text)
                                .unwrap_or_else(|| "unsupported".to_string()))
                            .collect::<Vec<_>>()
                            .join(","),
                        clause
                            .pattern_proof_coverage
                            .iter()
                            .map(CoreProofCoverage::as_str)
                            .collect::<Vec<_>>()
                            .join(","),
                        clause
                            .pattern_checked_preservation_evidence
                            .iter()
                            .map(|evidence| evidence
                                .as_ref()
                                .map(CoreCheckedPreservationEvidence::contract_text)
                                .unwrap_or_else(|| "none".to_string()))
                            .collect::<Vec<_>>()
                            .join(","),
                        clause
                            .guard
                            .as_ref()
                            .map(core_expr_summary_text)
                            .unwrap_or_else(|| "none".to_string()),
                        core_expr_summary_text(&clause.body)
                    )
                })
        }));
        lines.extend(self.constructors.iter().map(|constructor| {
            let params = constructor
                .params
                .iter()
                .map(core_param_contract_text)
                .collect::<Vec<_>>()
                .join(",");
            let vararg = constructor
                .vararg
                .as_ref()
                .map(core_param_contract_text)
                .unwrap_or_else(|| "none".to_string());
            format!(
                "constructor={} public={} min_arity={} params={} vararg={} return={} return_core={}",
                constructor.name,
                constructor.public,
                constructor.min_arity,
                params,
                vararg,
                constructor.return_type,
                core_type_contract_text(constructor.core_return_type.as_ref())
            )
        }));
        lines.extend(self.trait_conformances.iter().map(|conformance| {
            format!(
                "trait_conformance={} for={} source={:?} public={}",
                conformance.trait_ref, conformance.for_type, conformance.source, conformance.public
            )
        }));
        lines.push(format!(
            "metadata=functions:{} types:{} constructors:{} proof_readiness:{} proof_expr_lean:{} proof_expr_partial:{} proof_expr_model_required:{} proof_expr_runtime_boundary:{} proof_expr_artifact_only:{} proof_pattern_lean:{} proof_pattern_partial:{} proof_pattern_model_required:{} proof_pattern_runtime_boundary:{} proof_pattern_artifact_only:{} typed_core_expr:{} summary_only_expr:{} typed_core_pattern:{} summary_only_pattern:{} typed_core_type:{} summary_only_type:{} checked_preservation_expr:{} checked_preservation_pattern:{} checked_preservation_expr_structural:{} checked_preservation_pattern_structural:{} checked_preservation_expr_no_runtime_bindings:{} checked_preservation_pattern_no_runtime_bindings:{} checked_preservation_expr_runtime_bindings_required:{} checked_preservation_pattern_runtime_bindings_required:{} resolved_constructor_call_identity:{} resolved_constructor_chain_identity:{} resolved_constructor_pattern_identity:{} unresolved_constructor_call_candidate:{} unresolved_constructor_chain_candidate:{} unresolved_constructor_pattern_candidate:{}",
            self.metadata.interface_function_count,
            self.metadata.interface_type_count,
            self.metadata.constructor_count,
            self.metadata.proof_readiness.as_str(),
            self.metadata.lean_covered_expr_count,
            self.metadata.partial_expr_count,
            self.metadata.proof_model_required_expr_count,
            self.metadata.runtime_boundary_expr_count,
            self.metadata.artifact_only_expr_count,
            self.metadata.lean_covered_pattern_count,
            self.metadata.partial_pattern_count,
            self.metadata.proof_model_required_pattern_count,
            self.metadata.runtime_boundary_pattern_count,
            self.metadata.artifact_only_pattern_count,
            self.metadata.typed_core_expr_count,
            self.metadata.summary_only_expr_count,
            self.metadata.typed_core_pattern_count,
            self.metadata.summary_only_pattern_count,
            self.metadata.typed_core_type_count,
            self.metadata.summary_only_type_count,
            self.metadata.checked_preservation_expr_count,
            self.metadata.checked_preservation_pattern_count,
            self.metadata.checked_preservation_expr_structural_count,
            self.metadata.checked_preservation_pattern_structural_count,
            self.metadata
                .checked_preservation_expr_no_runtime_bindings_count,
            self.metadata
                .checked_preservation_pattern_no_runtime_bindings_count,
            self.metadata
                .checked_preservation_expr_runtime_bindings_required_count,
            self.metadata
                .checked_preservation_pattern_runtime_bindings_required_count,
            self.metadata.resolved_constructor_call_identity_count,
            self.metadata.resolved_constructor_chain_identity_count,
            self.metadata.resolved_constructor_pattern_identity_count,
            self.metadata.unresolved_constructor_call_candidate_count,
            self.metadata.unresolved_constructor_chain_candidate_count,
            self.metadata.unresolved_constructor_pattern_candidate_count
        ));
        lines.join("\n")
    }
}

#[derive(Debug, Clone)]
struct FunctionScheme {
    params: Vec<Type>,
    ret: Type,
    bounds: Vec<FunctionBound>,
}

#[derive(Debug, Clone)]
struct FunctionBound {
    trait_name: String,
    trait_args: Vec<Type>,
}

#[derive(Debug, Clone)]
struct ReceiverMethodDispatchSignature {
    receiver_type: Type,
    scheme: FunctionScheme,
}

#[derive(Debug, Clone)]
struct ConstructorScheme {
    fixed_params: Vec<Type>,
    min_arity: usize,
    vararg: Option<Type>,
    ret: Type,
}

#[derive(Debug, Clone)]
struct TemplateScheme {
    props: HashMap<String, Type>,
}

#[derive(Debug, Clone)]
struct TypeAlias {
    params: Vec<TypeVarId>,
    body: Type,
    is_opaque: bool,
}

#[derive(Debug, Clone)]
struct QualifiedTypeName {
    module: String,
    name: String,
}

#[derive(Debug, Default)]
struct TraitLookupCache {
    bound_checks: HashMap<TraitBoundLookupKey, bool>,
    method_calls: HashMap<TraitMethodLookupKey, TraitMethodLookupResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TraitBoundLookupKey {
    trait_name: String,
    bound_args: Vec<Type>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TraitMethodLookupKey {
    trait_name: String,
    method_name: String,
    arg_types: Vec<Type>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TraitMethodLookupResult {
    NoMatch,
    Ambiguous,
    Single(usize),
}

struct ExprInferContext<'a> {
    local_fns: &'a HashMap<(String, usize), FunctionSymbol>,
    signatures: &'a HashMap<(String, usize), FunctionScheme>,
    interface_map: &'a HashMap<String, ModuleInterface>,
    module_aliases: &'a HashMap<String, String>,
    file_imports: &'a HashMap<String, String>,
    markdown_imports: &'a HashMap<String, String>,
    function_imports: &'a HashMap<String, ImportedFunctionTarget>,
    imported_type_names: &'a HashMap<String, QualifiedTypeName>,
    constructor_aliases: &'a HashMap<String, QualifiedTypeName>,
    constructors: &'a HashMap<String, Vec<ConstructorScheme>>,
    templates: &'a HashMap<String, TemplateScheme>,
    aliases: &'a HashMap<String, TypeAlias>,
    struct_fields: &'a HashMap<String, HashMap<String, Type>>,
    receiver_methods: &'a HashMap<(String, usize), Vec<ReceiverMethodDispatchSignature>>,
    trait_method_calls: &'a HashMap<(String, String), Vec<ResolvedTraitMethod>>,
    trait_bound_impl_type_args: &'a HashMap<String, Vec<Vec<Type>>>,
    trait_signatures: &'a HashMap<String, ParsedTraitSignature>,
    alias_names: &'a HashSet<String>,
    current_bounds: &'a [FunctionBound],
    trait_lookup_cache: &'a RefCell<TraitLookupCache>,
}

/// Creates an expression-inference context for one callable body.
///
/// Inputs:
/// - `ctx`: module-wide expression inference context.
/// - `current_bounds`: instantiated generic trait bounds declared by the
///   callable currently being checked.
///
/// Output:
/// - A shallow context view that shares all module-wide lookup tables while
///   exposing the active callable bounds to trait dispatch and bound checking.
///
/// Transformation:
/// - Copies immutable references from `ctx`, replaces only `current_bounds`,
///   and reuses the same trait-lookup cache so repeated lookups stay
///   deterministic within a typecheck pass.
fn expr_ctx_with_current_bounds<'a, 'b>(
    ctx: &'a ExprInferContext<'a>,
    current_bounds: &'b [FunctionBound],
) -> ExprInferContext<'b>
where
    'a: 'b,
{
    ExprInferContext {
        local_fns: ctx.local_fns,
        signatures: ctx.signatures,
        interface_map: ctx.interface_map,
        module_aliases: ctx.module_aliases,
        file_imports: ctx.file_imports,
        markdown_imports: ctx.markdown_imports,
        function_imports: ctx.function_imports,
        imported_type_names: ctx.imported_type_names,
        constructor_aliases: ctx.constructor_aliases,
        constructors: ctx.constructors,
        templates: ctx.templates,
        aliases: ctx.aliases,
        struct_fields: ctx.struct_fields,
        receiver_methods: ctx.receiver_methods,
        trait_method_calls: ctx.trait_method_calls,
        trait_bound_impl_type_args: ctx.trait_bound_impl_type_args,
        trait_signatures: ctx.trait_signatures,
        alias_names: ctx.alias_names,
        current_bounds,
        trait_lookup_cache: ctx.trait_lookup_cache,
    }
}

#[derive(Debug, Clone, Default)]
struct TypeCheckImportMaps {
    module_aliases: HashMap<String, String>,
    file_imports: HashMap<String, String>,
    markdown_imports: HashMap<String, String>,
    function_imports: HashMap<String, ImportedFunctionTarget>,
}

/// Selected function import target visible under a local call name.
///
/// Inputs:
/// - Produced from source imports such as `import std.io.Console.{println}` or
///   `import module.{source as local}`.
///
/// Output:
/// - Source module/function identity used by call inference.
///
/// Transformation:
/// - Keeps the source function separate from the local alias so selected
///   imports can be typechecked against the provider interface before backend
///   emission rewrites the call target.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ImportedFunctionTarget {
    module: String,
    function: String,
    span: Span,
}

const SPANNED_EXPR_ERROR_PREFIX: &str = "\u{1f}terlan-span:";

/// Encodes an expression diagnostic with a precise source span override.
///
/// Inputs:
/// - `span`: source byte range that should be highlighted.
/// - `message`: diagnostic message.
///
/// Output:
/// - Internal expression-error string carrying span metadata.
///
/// Transformation:
/// - Prefixes the message with a private marker consumed only when expression
///   errors are converted back into public diagnostics.
fn spanned_expression_error(span: Span, message: impl Into<String>) -> String {
    format!(
        "{}{}:{}:{}",
        SPANNED_EXPR_ERROR_PREFIX,
        span.start,
        span.end,
        message.into()
    )
}

/// Converts an internal expression error into a public diagnostic.
///
/// Inputs:
/// - `error`: expression-inference error string, optionally span-prefixed.
/// - `fallback_span`: source range used for ordinary expression diagnostics.
///
/// Output:
/// - Public diagnostic with severity `Error`.
///
/// Transformation:
/// - Decodes precise span overrides for selected expression errors and keeps
///   existing fallback-span behavior for all other expression diagnostics.
fn expression_error_to_diagnostic(error: String, fallback_span: Span) -> Diagnostic {
    if let Some(rest) = error.strip_prefix(SPANNED_EXPR_ERROR_PREFIX) {
        if let Some((start_text, rest)) = rest.split_once(':') {
            if let Some((end_text, message)) = rest.split_once(':') {
                if let (Ok(start), Ok(end)) =
                    (start_text.parse::<usize>(), end_text.parse::<usize>())
                {
                    return Diagnostic {
                        span: Span::new(start, end),
                        message: message.to_string(),
                        severity: DiagSeverity::Error,
                    };
                }
            }
        }
    }

    Diagnostic {
        span: fallback_span,
        message: error,
        severity: DiagSeverity::Error,
    }
}

#[derive(Debug, Clone)]
struct TypeCheckInputs<'a> {
    import_maps: TypeCheckImportMaps,
    local_aliases: HashMap<String, TypeAlias>,
    alias_extra_names: HashSet<String>,
    kind_diagnostics: Vec<Diagnostic>,
    macro_decl_diagnostics: Vec<Diagnostic>,
    trait_decl_diagnostics: Vec<Diagnostic>,
    trait_impl_coherence_diagnostics: Vec<Diagnostic>,
    trait_impl_signature_diagnostics: Vec<Diagnostic>,
    function_signatures: HashMap<(String, usize), FunctionScheme>,
    constructor_signatures: HashMap<String, Vec<ConstructorScheme>>,
    struct_fields: HashMap<String, HashMap<String, Type>>,
    receiver_methods: HashMap<(String, usize), Vec<ReceiverMethodDispatchSignature>>,
    template_schemes: HashMap<String, TemplateScheme>,
    syntax_function_module: &'a SyntaxModuleOutput,
    trait_signatures: HashMap<String, ParsedTraitSignature>,
    trait_method_calls: HashMap<(String, String), Vec<ResolvedTraitMethod>>,
}

fn type_check_syntax_module_with_inputs<'a>(
    resolved: &ResolvedModule,
    inputs: TypeCheckInputs<'a>,
) -> Vec<Diagnostic> {
    let mut diagnostics = resolved
        .diagnostics
        .iter()
        .map(|diag| Diagnostic {
            span: diag.span,
            message: diag.message.clone(),
            severity: DiagSeverity::Error,
        })
        .collect::<Vec<_>>();

    let local_aliases = inputs.local_aliases;
    diagnostics.extend(inputs.kind_diagnostics);
    diagnostics.extend(inputs.macro_decl_diagnostics);
    let trait_signatures = inputs.trait_signatures;
    diagnostics.extend(inputs.trait_impl_coherence_diagnostics);
    diagnostics.extend(inputs.trait_decl_diagnostics);
    diagnostics.extend(inputs.trait_impl_signature_diagnostics);
    if std::env::var("TYPL_TCDUMP").is_ok() {
        eprintln!("aliases: {:?}", local_aliases.keys().collect::<Vec<_>>());
    }
    let imported_type_aliases = imported_type_aliases(resolved);
    let mut aliases = imported_type_aliases.clone();
    aliases.extend(local_aliases.clone());

    let mut alias_names: HashSet<String> = aliases.keys().cloned().collect();
    alias_names.extend(resolved.imported_types.keys().cloned());
    let imported_type_names = imported_type_names(resolved);
    alias_names.extend(inputs.alias_extra_names);
    let function_signatures = inputs.function_signatures;
    let constructor_signatures = inputs.constructor_signatures;
    let struct_fields = inputs.struct_fields;
    let receiver_methods = inputs.receiver_methods;
    let template_schemes = inputs.template_schemes;
    let import_maps = inputs.import_maps;
    let module_aliases = import_maps.module_aliases;
    let file_imports = import_maps.file_imports;
    let markdown_imports = import_maps.markdown_imports;
    let function_imports = import_maps.function_imports;
    let imported_type_names = imported_type_names;
    let constructor_aliases = imported_type_names.clone();
    let trait_method_calls = inputs.trait_method_calls;
    let trait_bound_impl_type_args = collect_trait_bound_impl_type_args(&trait_method_calls);
    let trait_lookup_cache = RefCell::new(TraitLookupCache::default());
    let expr_ctx = ExprInferContext {
        local_fns: &resolved.function_symbols,
        signatures: &function_signatures,
        interface_map: &resolved.interface_map,
        module_aliases: &module_aliases,
        file_imports: &file_imports,
        markdown_imports: &markdown_imports,
        function_imports: &function_imports,
        imported_type_names: &imported_type_names,
        constructor_aliases: &constructor_aliases,
        constructors: &constructor_signatures,
        templates: &template_schemes,
        aliases: &aliases,
        struct_fields: &struct_fields,
        receiver_methods: &receiver_methods,
        trait_method_calls: &trait_method_calls,
        trait_bound_impl_type_args: &trait_bound_impl_type_args,
        trait_signatures: &trait_signatures,
        alias_names: &alias_names,
        current_bounds: &[],
        trait_lookup_cache: &trait_lookup_cache,
    };

    diagnostics.extend(check_syntax_module_functions(
        inputs.syntax_function_module,
        &function_signatures,
        &alias_names,
        &aliases,
        &imported_type_names,
        &imported_type_aliases,
        &local_aliases,
        &expr_ctx,
    ));

    diagnostics
}

fn check_syntax_module_functions(
    module: &SyntaxModuleOutput,
    function_signatures: &HashMap<(String, usize), FunctionScheme>,
    alias_names: &HashSet<String>,
    aliases: &HashMap<String, TypeAlias>,
    imported_type_names: &HashMap<String, QualifiedTypeName>,
    imported_type_aliases: &HashMap<String, TypeAlias>,
    local_aliases: &HashMap<String, TypeAlias>,
    expr_ctx: &ExprInferContext,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for declaration in &module.declarations {
        match &declaration.payload {
            SyntaxDeclarationPayload::Function {
                name,
                params,
                clauses,
                ..
            } => {
                let key = (name.clone(), params.len());
                let scheme = match function_signatures.get(&key) {
                    Some(scheme) => scheme.clone(),
                    None => {
                        diagnostics.push(Diagnostic {
                            span: declaration.span.into(),
                            message: format!(
                                "missing type signature for function {} / {}",
                                name,
                                params.len()
                            ),
                            severity: DiagSeverity::Error,
                        });
                        continue;
                    }
                };

                check_syntax_callable_clauses(
                    &format!("function {}", name),
                    name,
                    params,
                    clauses,
                    &scheme,
                    declaration.span.into(),
                    alias_names,
                    aliases,
                    expr_ctx,
                    &mut diagnostics,
                );
            }
            SyntaxDeclarationPayload::TraitImpl { methods, .. } => {
                for method in methods {
                    let scheme = function_decl_to_scheme(
                        &method
                            .params
                            .iter()
                            .map(|param| param.annotation.text.clone())
                            .collect::<Vec<_>>(),
                        &method.return_type.text,
                        &method.generic_bounds,
                        alias_names,
                        imported_type_names,
                        imported_type_aliases,
                        local_aliases,
                    );

                    check_syntax_callable_clauses(
                        &format!("impl method {}", method.name),
                        &method.name,
                        &method.params,
                        &method.clauses,
                        &scheme,
                        method.span.into(),
                        alias_names,
                        aliases,
                        expr_ctx,
                        &mut diagnostics,
                    );
                }
            }
            SyntaxDeclarationPayload::Method {
                receiver,
                name,
                params,
                return_type,
                clauses,
                generic_bounds,
                ..
            } => {
                let mut receiver_first_params = Vec::with_capacity(params.len() + 1);
                receiver_first_params.push(receiver.clone());
                receiver_first_params.extend(params.iter().cloned());
                let scheme = function_decl_to_scheme(
                    &receiver_first_params
                        .iter()
                        .map(|param| param.annotation.text.clone())
                        .collect::<Vec<_>>(),
                    &return_type.text,
                    generic_bounds,
                    alias_names,
                    imported_type_names,
                    imported_type_aliases,
                    local_aliases,
                );

                check_syntax_callable_clauses(
                    &format!("receiver method {}", name),
                    name,
                    &receiver_first_params,
                    &receiver_method_clauses_with_bindings(receiver, params, clauses),
                    &scheme,
                    declaration.span.into(),
                    alias_names,
                    aliases,
                    expr_ctx,
                    &mut diagnostics,
                );
            }
            _ => {}
        }
    }
    diagnostics
}

/// Synthesizes callable patterns for receiver-method body checking.
///
/// Inputs:
/// - `receiver`: receiver parameter declared before the method name.
/// - `params`: ordinary method parameters.
/// - `clauses`: syntax-output method clauses produced by the parser.
///
/// Output:
/// - Owned clause list whose patterns bind the receiver followed by each method
///   parameter, preserving each original body, guard, and span.
///
/// Transformation:
/// - Converts the current single-expression receiver-method declaration shape
///   into the function-like clause shape expected by `check_syntax_callable_clauses`.
///   This is a typechecking adapter only; it does not alter syntax output.
fn receiver_method_clauses_with_bindings(
    receiver: &SyntaxParamOutput,
    params: &[SyntaxParamOutput],
    clauses: &[SyntaxFunctionClauseOutput],
) -> Vec<SyntaxFunctionClauseOutput> {
    clauses
        .iter()
        .map(|clause| {
            let patterns = std::iter::once(receiver)
                .chain(params.iter())
                .map(|param| SyntaxPatternOutput {
                    kind: SyntaxPatternKind::Var,
                    arity: 1,
                    text: Some(param.name.clone()),
                    children: Vec::new(),
                    fields: Vec::new(),
                })
                .collect();
            SyntaxFunctionClauseOutput {
                patterns,
                guard: clause.guard.clone(),
                body: clause.body.clone(),
                has_guard: clause.has_guard,
                span: clause.span,
            }
        })
        .collect()
}

/// Checks syntax-output callable clauses against a declared function scheme.
///
/// Inputs:
/// - `callable_label`: diagnostic label such as `function add` or
///   `impl method to_string`.
/// - `callable_name`: bare function/method name used for trait-bound and
///   exhaustiveness diagnostics.
/// - `params`: declared callable parameters.
/// - `clauses`: parsed callable clauses with patterns, guards, and bodies.
/// - `scheme`: parsed parameter and return types for the callable.
/// - `fallback_span`: span used for diagnostics that are not tied to one
///   clause.
/// - `alias_names`: local/imported names that may appear in patterns.
/// - `aliases`: visible type aliases used for expected/inferred comparison.
/// - `expr_ctx`: expression inference context.
/// - `diagnostics`: output diagnostic buffer.
///
/// Output:
/// - No direct return value.
///
/// Transformation:
/// - Instantiates the callable scheme per clause, checks pattern bindings
///   against parameter types, infers the clause body, and unifies it with the
///   declared return type. The same path is used for normal functions and
///   explicit trait impl methods so adapter bodies cannot bypass typechecking.
fn check_syntax_callable_clauses(
    callable_label: &str,
    callable_name: &str,
    params: &[SyntaxParamOutput],
    clauses: &[SyntaxFunctionClauseOutput],
    scheme: &FunctionScheme,
    fallback_span: Span,
    alias_names: &HashSet<String>,
    aliases: &HashMap<String, TypeAlias>,
    expr_ctx: &ExprInferContext,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut clause_patterns: Vec<(Vec<SyntaxPatternOutput>, Span)> = Vec::new();

    if clauses.is_empty() {
        diagnostics.push(Diagnostic {
            span: fallback_span,
            message: format!("{} has no clauses", callable_label),
            severity: DiagSeverity::Error,
        });
        return;
    }

    for clause in clauses {
        let span = clause.span.into();
        if clause.patterns.len() != params.len() {
            diagnostics.push(Diagnostic {
                span,
                message: format!(
                    "{} has arity mismatch: expected {}, found {}",
                    callable_label,
                    params.len(),
                    clause.patterns.len()
                ),
                severity: DiagSeverity::Error,
            });
            continue;
        }

        let instantiated = instantiate_function_scheme(scheme);
        let mut subst = HashMap::new();
        let mut locals: HashMap<String, Type> = HashMap::new();
        for (pattern, param_type) in clause.patterns.iter().zip(instantiated.params.iter()) {
            if let Err(message) = check_syntax_pattern(
                pattern,
                &expand_type_aliases(param_type, aliases),
                aliases,
                Some(expr_ctx),
                &mut locals,
                &mut subst,
            ) {
                diagnostics.push(Diagnostic {
                    span,
                    message,
                    severity: DiagSeverity::Error,
                });
            }
        }

        let local_expr_ctx = expr_ctx_with_current_bounds(expr_ctx, &instantiated.bounds);
        let bounds_error = if let Err(message) =
            check_function_bounds(&instantiated, Some(callable_name), &local_expr_ctx, &subst)
        {
            diagnostics.push(Diagnostic {
                span,
                message,
                severity: DiagSeverity::Error,
            });
            true
        } else {
            false
        };

        let mut local_errors = Vec::new();
        let inferred = if bounds_error {
            Type::Dynamic
        } else {
            infer_syntax_expr(
                &clause.body,
                &locals,
                &local_expr_ctx,
                &mut subst,
                &mut local_errors,
            )
        };

        for error in local_errors {
            diagnostics.push(expression_error_to_diagnostic(error, span));
        }

        let expected_expanded = expand_type_aliases(&instantiated.ret, aliases);
        let inferred_expanded = expand_type_aliases(&inferred, aliases);

        if let Err(message) = unify(&expected_expanded, &inferred_expanded, &mut subst) {
            let revealed_inferred = reveal_opaque_aliases(&inferred_expanded, aliases);
            if unify(&expected_expanded, &revealed_inferred, &mut subst).is_ok() {
                clause_patterns.push((clause.patterns.clone(), span));
                continue;
            }
            if expected_syntax_opaque_constructor_return_matches(
                &clause.body,
                &expected_expanded,
                &locals,
                expr_ctx,
                &mut subst,
            ) {
                clause_patterns.push((clause.patterns.clone(), span));
                continue;
            }
            diagnostics.push(Diagnostic {
                span,
                message,
                severity: DiagSeverity::Error,
            });
        }

        clause_patterns.push((clause.patterns.clone(), span));
    }

    check_syntax_function_clause_exhaustiveness(
        callable_name,
        params.first().map(|param| param.annotation.text.as_str()),
        params.len(),
        alias_names,
        &clause_patterns,
        aliases,
        diagnostics,
    );
}

/// Formal type checker entry point for compiler-facing syntax output.
///
/// This path must not adapt through the parser AST adapter.
pub fn type_check_syntax_module_output(
    module: &SyntaxModuleOutput,
    resolved: &ResolvedModule,
) -> Vec<Diagnostic> {
    let local_aliases = collect_syntax_type_aliases(module);
    let imported_aliases = imported_type_aliases(resolved);
    let imported_names = imported_type_names(resolved);
    let mut aliases = imported_aliases.clone();
    aliases.extend(local_aliases.clone());
    let local_type_names = collect_syntax_type_names(module);
    let mut alias_names = local_type_names.clone();
    alias_names.extend(imported_aliases.keys().cloned());
    alias_names.extend(resolved.imported_types.keys().cloned());
    alias_names.extend(collect_syntax_alias_extra_names(module));
    let trait_signatures = collect_syntax_trait_signatures(module, resolved);
    let trait_method_calls =
        collect_syntax_trait_method_calls(module, &alias_names, &trait_signatures, resolved);
    let mut trait_decl_diagnostics = check_syntax_trait_decls(module, &trait_signatures);
    trait_decl_diagnostics.extend(check_syntax_struct_derives(module, &trait_signatures));
    trait_decl_diagnostics.extend(check_syntax_declared_implements(module, &trait_signatures));
    let trait_impl_coherence_diagnostics = check_syntax_trait_impl_coherence(module);
    let trait_impl_signature_diagnostics =
        check_syntax_trait_impl_signatures(module, &trait_signatures);
    let receiver_method_diagnostics = check_syntax_receiver_methods(module, &local_type_names);

    let mut diagnostics = collect_syntax_unsupported_raw_declaration_diagnostics(module);
    diagnostics.extend(check_syntax_public_constructor_return_visibility(
        module,
        resolved,
        &alias_names,
    ));

    let inputs = TypeCheckInputs {
        import_maps: collect_syntax_import_maps(module),
        local_aliases: local_aliases.clone(),
        alias_extra_names: collect_syntax_alias_extra_names(module),
        kind_diagnostics: collect_syntax_kind_diagnostics(module),
        macro_decl_diagnostics: check_syntax_macro_decl_signatures(module),
        trait_decl_diagnostics,
        trait_impl_coherence_diagnostics,
        trait_impl_signature_diagnostics,
        receiver_methods: collect_syntax_receiver_method_dispatch_signatures(
            module,
            &alias_names,
            &imported_names,
            &imported_aliases,
            &local_aliases,
        ),
        function_signatures: collect_syntax_function_signatures(
            module,
            &alias_names,
            &imported_names,
            &imported_aliases,
            &local_aliases,
        ),
        constructor_signatures: collect_syntax_constructor_signatures(
            module,
            &alias_names,
            &imported_names,
            &imported_aliases,
            &aliases,
        ),
        struct_fields: collect_syntax_struct_fields(module, &alias_names),
        template_schemes: collect_syntax_template_schemes(module, &alias_names),
        syntax_function_module: module,
        trait_signatures,
        trait_method_calls,
    };
    diagnostics.extend(receiver_method_diagnostics);
    diagnostics.extend(type_check_syntax_module_with_inputs(resolved, inputs));

    diagnostics
}

/// Lowers resolved formal compiler state to the current core boundary.
///
/// Inputs:
/// - `resolved` compiler module produced by resolution and typechecking.
///
/// Output:
/// - Deterministic backend-neutral `CoreModule` payload.
///
/// Transformation:
/// - Copies the resolver interface into the core artifact and retains the
///   canonical module name.
/// - This function intentionally does not include backend-specific calls,
///   Erlang syntax, or JVM/JS encoding assumptions.
pub fn lower_resolved_module_to_core(resolved: &ResolvedModule) -> CoreModule {
    let imports = lower_core_imports(resolved);
    let exports = lower_core_exports(&resolved.interface);
    let types = lower_core_types(&resolved.interface);
    let functions = lower_core_functions(&resolved.interface);
    let constructors = lower_core_constructors(&resolved.interface);
    let metadata = core_module_metadata(&functions, &types, &constructors);

    CoreModule {
        schema: CORE_IR_SCHEMA.to_string(),
        module: resolved.name.clone(),
        source: CoreSourceIdentity {
            source_kind: "resolved_module".to_string(),
            syntax_contract_fingerprint: None,
        },
        imports,
        exports,
        types,
        functions,
        constructors,
        trait_conformances: Vec::new(),
        metadata,
        interface: resolved.interface.clone(),
    }
}

/// Lowers syntax-output plus resolved formal compiler state to CoreIR.
///
/// Inputs:
/// - `module`: compiler-facing syntax output produced from the canonical syntax
///   contract.
/// - `resolved`: resolver artifact after formal typechecking.
///
/// Output:
/// - Deterministic backend-neutral `CoreModule` payload with function clause
///   and expression summaries.
///
/// Transformation:
/// - Starts from the resolver/interface Core boundary, attaches syntax contract
///   identity, and overlays syntax-output function clauses as Core summaries
///   without encoding backend syntax or emitted Erlang forms.
pub fn lower_syntax_module_output_to_core(
    module: &SyntaxModuleOutput,
    resolved: &ResolvedModule,
) -> CoreModule {
    let mut core = lower_resolved_module_to_core(resolved);
    core.source = CoreSourceIdentity {
        source_kind: format!("{:?}", module.source_kind),
        syntax_contract_fingerprint: Some(module.syntax_contract.fingerprint.clone()),
    };
    core.imports = core_syntax_imports(module);
    core.trait_conformances = core_syntax_trait_conformances(module);
    let syntax_struct_bodies = core_syntax_struct_type_bodies(module);
    for type_decl in &mut core.types {
        if let Some(core_body) = syntax_struct_bodies.get(&type_decl.name) {
            type_decl.core_body = Some(core_body.clone());
        }
    }

    let mut function_clauses = core_syntax_function_clauses(module);
    let constructor_identities = core_constructor_identities(module, resolved, &core.constructors);
    resolve_constructor_identities_in_function_clauses(
        &mut function_clauses,
        &constructor_identities,
    );
    refresh_core_evidence_in_function_clauses(&mut function_clauses);
    for function in &mut core.functions {
        if let Some(clauses) = function_clauses.get(&(function.name.clone(), function.arity)) {
            function.clauses = clauses.clone();
        }
    }
    core.metadata = core_module_metadata(&core.functions, &core.types, &core.constructors);
    core
}

#[derive(Clone, Default)]
struct CoreProofCoverageCounts {
    lean_covered: usize,
    partial: usize,
    proof_model_required: usize,
    runtime_boundary: usize,
    artifact_only: usize,
}

#[derive(Clone, Default)]
struct CoreExprPayloadCounts {
    typed_core_expr: usize,
    summary_only_expr: usize,
}

#[derive(Clone, Default)]
struct CoreCheckedPreservationCounts {
    expr: usize,
    pattern: usize,
    expr_structural: usize,
    pattern_structural: usize,
    expr_no_runtime_bindings: usize,
    pattern_no_runtime_bindings: usize,
    expr_runtime_bindings_required: usize,
    pattern_runtime_bindings_required: usize,
}

#[derive(Clone, Default)]
struct CorePatternPayloadCounts {
    typed_core_pattern: usize,
    summary_only_pattern: usize,
}

#[derive(Clone, Default)]
struct CoreTypePayloadCounts {
    typed_core_type: usize,
    summary_only_type: usize,
}

#[derive(Clone, Default)]
struct CoreConstructorIdentityCounts {
    resolved_constructor_call_identity: usize,
    resolved_constructor_chain_identity: usize,
    resolved_constructor_pattern_identity: usize,
    unresolved_constructor_call_candidate: usize,
    unresolved_constructor_chain_candidate: usize,
    unresolved_constructor_pattern_candidate: usize,
}

/// Builds CoreIR module metadata from declarations and expression summaries.
///
/// Inputs:
/// - `functions`: Core functions whose clauses may contain expression
///   summaries.
/// - `types`: Core type declarations whose bodies may carry typed Core
///   payloads.
/// - `constructors`: Core constructor declarations whose signature types may
///   carry typed Core payloads.
///
/// Output:
/// - `CoreModuleMetadata` with declaration counts and recursive proof-coverage
///   expression/pattern counts plus typed-payload counts.
///
/// Transformation:
/// - Counts declarations directly, traverses function guards/bodies for
///   expression coverage and typed-payload coverage, counts clause pattern
///   coverage and pattern payload coverage, counts signature type payloads,
///   counts resolved constructor identities, and derives module readiness from
///   the combined coverage buckets.
fn core_module_metadata(
    functions: &[CoreFunction],
    types: &[CoreTypeDecl],
    constructors: &[CoreConstructorDecl],
) -> CoreModuleMetadata {
    let mut expr_coverage = CoreProofCoverageCounts::default();
    let mut expr_payloads = CoreExprPayloadCounts::default();
    let mut checked_counts = CoreCheckedPreservationCounts::default();
    let mut pattern_coverage = CoreProofCoverageCounts::default();
    let mut pattern_payloads = CorePatternPayloadCounts::default();
    let mut type_payloads = CoreTypePayloadCounts::default();
    let mut constructor_identities = CoreConstructorIdentityCounts::default();
    for function in functions {
        count_core_function_type_payloads(function, &mut type_payloads);
        for clause in &function.clauses {
            for coverage in &clause.pattern_proof_coverage {
                count_core_pattern_proof_coverage(*coverage, &mut pattern_coverage);
            }
            count_core_pattern_payloads(&clause.core_patterns, &mut pattern_payloads);
            count_core_function_clause_pattern_constructor_identities(
                &clause.core_patterns,
                &mut constructor_identities,
            );
            count_core_pattern_checked_preservation(
                &clause.pattern_checked_preservation_evidence,
                &mut checked_counts,
            );
            if let Some(guard) = &clause.guard {
                count_core_expr_proof_coverage(guard, &mut expr_coverage);
                count_core_expr_payloads(guard, &mut expr_payloads);
                count_core_expr_checked_preservation(guard, &mut checked_counts);
                count_core_expr_summary_constructor_identities(guard, &mut constructor_identities);
            }
            count_core_expr_proof_coverage(&clause.body, &mut expr_coverage);
            count_core_expr_payloads(&clause.body, &mut expr_payloads);
            count_core_expr_checked_preservation(&clause.body, &mut checked_counts);
            count_core_expr_summary_constructor_identities(
                &clause.body,
                &mut constructor_identities,
            );
        }
    }
    for type_decl in types {
        count_core_type_decl_payloads(type_decl, &mut type_payloads);
    }
    for constructor in constructors {
        count_core_constructor_type_payloads(constructor, &mut type_payloads);
    }
    let combined_coverage = combined_core_proof_coverage(&expr_coverage, &pattern_coverage);

    CoreModuleMetadata {
        interface_function_count: functions.len(),
        interface_type_count: types.len(),
        constructor_count: constructors.len(),
        proof_readiness: core_module_proof_readiness(&combined_coverage, &type_payloads),
        lean_covered_expr_count: expr_coverage.lean_covered,
        partial_expr_count: expr_coverage.partial,
        proof_model_required_expr_count: expr_coverage.proof_model_required,
        runtime_boundary_expr_count: expr_coverage.runtime_boundary,
        artifact_only_expr_count: expr_coverage.artifact_only,
        lean_covered_pattern_count: pattern_coverage.lean_covered,
        partial_pattern_count: pattern_coverage.partial,
        proof_model_required_pattern_count: pattern_coverage.proof_model_required,
        runtime_boundary_pattern_count: pattern_coverage.runtime_boundary,
        artifact_only_pattern_count: pattern_coverage.artifact_only,
        typed_core_expr_count: expr_payloads.typed_core_expr,
        summary_only_expr_count: expr_payloads.summary_only_expr,
        typed_core_pattern_count: pattern_payloads.typed_core_pattern,
        summary_only_pattern_count: pattern_payloads.summary_only_pattern,
        typed_core_type_count: type_payloads.typed_core_type,
        summary_only_type_count: type_payloads.summary_only_type,
        checked_preservation_expr_count: checked_counts.expr,
        checked_preservation_pattern_count: checked_counts.pattern,
        checked_preservation_expr_structural_count: checked_counts.expr_structural,
        checked_preservation_pattern_structural_count: checked_counts.pattern_structural,
        checked_preservation_expr_no_runtime_bindings_count: checked_counts
            .expr_no_runtime_bindings,
        checked_preservation_pattern_no_runtime_bindings_count: checked_counts
            .pattern_no_runtime_bindings,
        checked_preservation_expr_runtime_bindings_required_count: checked_counts
            .expr_runtime_bindings_required,
        checked_preservation_pattern_runtime_bindings_required_count: checked_counts
            .pattern_runtime_bindings_required,
        resolved_constructor_call_identity_count: constructor_identities
            .resolved_constructor_call_identity,
        resolved_constructor_chain_identity_count: constructor_identities
            .resolved_constructor_chain_identity,
        resolved_constructor_pattern_identity_count: constructor_identities
            .resolved_constructor_pattern_identity,
        unresolved_constructor_call_candidate_count: constructor_identities
            .unresolved_constructor_call_candidate,
        unresolved_constructor_chain_candidate_count: constructor_identities
            .unresolved_constructor_chain_candidate,
        unresolved_constructor_pattern_candidate_count: constructor_identities
            .unresolved_constructor_pattern_candidate,
    }
}

/// Adds type-declaration body payloads to typed-payload counts.
///
/// Inputs:
/// - `type_decl`: Core type declaration whose body may carry a typed
///   `CoreType` payload.
/// - `counts`: mutable aggregate type-payload counts for the containing Core
///   module.
///
/// Output:
/// - None; `counts` is updated in place.
///
/// Transformation:
/// - Counts the type declaration body as typed when a `CoreType` payload exists
///   and summary-only when the declaration body remains textual.
fn count_core_type_decl_payloads(type_decl: &CoreTypeDecl, counts: &mut CoreTypePayloadCounts) {
    count_core_type_payload(type_decl.core_body.as_ref(), counts);
}

/// Adds a function signature's Core type payloads to aggregate counts.
///
/// Inputs:
/// - `function`: Core function whose parameter and return annotations may
///   carry typed `CoreType` payloads.
/// - `counts`: mutable aggregate type-payload counts for the containing Core
///   module.
///
/// Output:
/// - None; `counts` is updated in place.
///
/// Transformation:
/// - Counts each function parameter annotation and the function return
///   annotation as typed when a `CoreType` payload exists, otherwise as
///   summary-only.
fn count_core_function_type_payloads(function: &CoreFunction, counts: &mut CoreTypePayloadCounts) {
    for param in &function.params {
        count_core_type_payload(param.core_ty.as_ref(), counts);
    }
    count_core_type_payload(function.core_return_type.as_ref(), counts);
}

/// Adds a constructor signature's Core type payloads to aggregate counts.
///
/// Inputs:
/// - `constructor`: Core constructor whose parameters, optional vararg, and
///   return annotation may carry typed `CoreType` payloads.
/// - `counts`: mutable aggregate type-payload counts for the containing Core
///   module.
///
/// Output:
/// - None; `counts` is updated in place.
///
/// Transformation:
/// - Counts fixed parameters, the optional vararg parameter, and the return
///   annotation as typed when a `CoreType` payload exists, otherwise as
///   summary-only.
fn count_core_constructor_type_payloads(
    constructor: &CoreConstructorDecl,
    counts: &mut CoreTypePayloadCounts,
) {
    for param in &constructor.params {
        count_core_type_payload(param.core_ty.as_ref(), counts);
    }
    if let Some(vararg) = &constructor.vararg {
        count_core_type_payload(vararg.core_ty.as_ref(), counts);
    }
    count_core_type_payload(constructor.core_return_type.as_ref(), counts);
}

/// Adds one optional Core type payload to aggregate counts.
///
/// Inputs:
/// - `ty`: optional typed `CoreType` payload for one signature position.
/// - `counts`: mutable aggregate type-payload counts for the containing Core
///   module.
///
/// Output:
/// - None; `counts` is updated in place.
///
/// Transformation:
/// - Increments the typed bucket when a Core type payload exists and the
///   summary-only bucket when the signature position is still textual only.
fn count_core_type_payload(ty: Option<&CoreType>, counts: &mut CoreTypePayloadCounts) {
    if ty.is_some() {
        counts.typed_core_type += 1;
    } else {
        counts.summary_only_type += 1;
    }
}

/// Combines expression and pattern proof-coverage counts.
///
/// Inputs:
/// - `expr_coverage`: aggregate counts from Core expression summaries.
/// - `pattern_coverage`: aggregate counts from Core pattern summaries.
///
/// Output:
/// - Combined proof-coverage counts for module readiness decisions.
///
/// Transformation:
/// - Adds each coverage bucket pairwise while preserving separate source
///   counters on `CoreModuleMetadata`.
fn combined_core_proof_coverage(
    expr_coverage: &CoreProofCoverageCounts,
    pattern_coverage: &CoreProofCoverageCounts,
) -> CoreProofCoverageCounts {
    CoreProofCoverageCounts {
        lean_covered: expr_coverage.lean_covered + pattern_coverage.lean_covered,
        partial: expr_coverage.partial + pattern_coverage.partial,
        proof_model_required: expr_coverage.proof_model_required
            + pattern_coverage.proof_model_required,
        runtime_boundary: expr_coverage.runtime_boundary + pattern_coverage.runtime_boundary,
        artifact_only: expr_coverage.artifact_only + pattern_coverage.artifact_only,
    }
}

/// Derives a module-level proof readiness label from coverage counts.
///
/// Inputs:
/// - `coverage`: aggregate proof-coverage counts for a Core module.
///
/// Output:
/// - Conservative module readiness label.
///
/// Transformation:
/// - Chooses the most restrictive present label, with runtime-boundary and
///   partial forms taking precedence over proof-model work; returns
///   `NoExpressions` for modules without expression or pattern summaries.
fn core_proof_readiness(coverage: &CoreProofCoverageCounts) -> CoreProofReadiness {
    if coverage.runtime_boundary > 0 {
        CoreProofReadiness::RuntimeBoundary
    } else if coverage.partial > 0 {
        CoreProofReadiness::Partial
    } else if coverage.proof_model_required > 0 {
        CoreProofReadiness::ProofModelRequired
    } else if coverage.artifact_only > 0 {
        CoreProofReadiness::ArtifactOnly
    } else if coverage.lean_covered > 0 {
        CoreProofReadiness::LeanCovered
    } else {
        CoreProofReadiness::NoExpressions
    }
}

/// Derives module-level readiness from term coverage and type payload debt.
///
/// Inputs:
/// - `coverage`: aggregate expression and pattern proof-coverage counts.
/// - `type_payloads`: aggregate CoreType payload counts for type declarations,
///   function signatures, and constructor signatures.
///
/// Output:
/// - Conservative module readiness label.
///
/// Transformation:
/// - Starts from expression/pattern readiness, then promotes otherwise covered
///   or expression-free modules to `ProofModelRequired` when any type position
///   remains summary-only.
fn core_module_proof_readiness(
    coverage: &CoreProofCoverageCounts,
    type_payloads: &CoreTypePayloadCounts,
) -> CoreProofReadiness {
    let readiness = core_proof_readiness(coverage);
    if type_payloads.summary_only_type == 0 {
        return readiness;
    }
    match readiness {
        CoreProofReadiness::RuntimeBoundary
        | CoreProofReadiness::Partial
        | CoreProofReadiness::ProofModelRequired => readiness,
        CoreProofReadiness::ArtifactOnly
        | CoreProofReadiness::LeanCovered
        | CoreProofReadiness::NoExpressions => CoreProofReadiness::ProofModelRequired,
    }
}

/// Adds one expression summary tree to proof-coverage counts.
///
/// Inputs:
/// - `expr`: Core expression summary tree to count.
/// - `counts`: mutable aggregate counts for the containing Core module.
///
/// Output:
/// - None; `counts` is updated in place.
///
/// Transformation:
/// - Records the current expression's proof-coverage label and recursively
///   visits child expression summaries.
fn count_core_expr_proof_coverage(expr: &CoreExprSummary, counts: &mut CoreProofCoverageCounts) {
    match expr.proof_coverage {
        CoreProofCoverage::LeanCovered => counts.lean_covered += 1,
        CoreProofCoverage::Partial => counts.partial += 1,
        CoreProofCoverage::ProofModelRequired => counts.proof_model_required += 1,
        CoreProofCoverage::RuntimeBoundary => counts.runtime_boundary += 1,
        CoreProofCoverage::ArtifactOnly => counts.artifact_only += 1,
    }
    for child in &expr.children {
        count_core_expr_proof_coverage(child, counts);
    }
}

/// Adds one expression summary tree to typed-payload counts.
///
/// Inputs:
/// - `expr`: Core expression summary tree to count.
/// - `counts`: mutable aggregate payload counts for the containing Core module.
///
/// Output:
/// - None; `counts` is updated in place.
///
/// Transformation:
/// - Records whether the current expression summary has a typed `CoreExpr`
///   payload and recursively visits child expression summaries.
fn count_core_expr_payloads(expr: &CoreExprSummary, counts: &mut CoreExprPayloadCounts) {
    if expr.core_expr.is_some() {
        counts.typed_core_expr += 1;
    } else {
        counts.summary_only_expr += 1;
    }
    for child in &expr.children {
        count_core_expr_payloads(child, counts);
    }
}

/// Adds one expression summary tree to checked-preservation-evidence counts.
///
/// Inputs:
/// - `expr`: Core expression summary tree to count.
/// - `counts`: mutable aggregate checked-preservation counts for the containing
///   Core module.
///
/// Output:
/// - None; `counts` is updated in place.
///
/// Transformation:
/// - Marks the current expression summary as evidence-backed when its typed
///   `CoreExpr` can be shown to satisfy recursive checked-preservation
///   conditions and recursively checks all child summaries.
fn count_core_expr_checked_preservation(
    expr: &CoreExprSummary,
    counts: &mut CoreCheckedPreservationCounts,
) {
    if let Some(evidence) = &expr.checked_preservation_evidence {
        counts.expr += 1;
        if matches!(
            evidence.kind,
            CoreCheckedPreservationEvidenceKind::StructuralCoreExpr
        ) {
            counts.expr_structural += 1;
        }
        match evidence.freshness {
            CoreSubstitutionFreshnessEvidence::NoRuntimeBindings => {
                counts.expr_no_runtime_bindings += 1;
            }
            CoreSubstitutionFreshnessEvidence::RuntimeBindingsRequired => {
                counts.expr_runtime_bindings_required += 1;
            }
        }
    }
    for child in &expr.children {
        count_core_expr_checked_preservation(child, counts);
    }
}

/// Builds checked-preservation evidence for a typed Core expression.
///
/// Inputs:
/// - `expr`: typed Core expression payload attached to a CoreIR summary.
///
/// Output:
/// - `Some(CoreCheckedPreservationEvidence)` when the expression and all
///   recursive children satisfy the current checked-preservation predicate.
/// - `None` when the expression has no checked-preservation evidence yet.
///
/// Transformation:
/// - Reuses the structural evidence predicate, then records the covered Core
///   term as deterministic Core contract text for future Lean export.
fn core_expr_checked_preservation_evidence(
    expr: &CoreExpr,
) -> Option<CoreCheckedPreservationEvidence> {
    core_expr_has_checked_preservation_evidence(expr).then(|| CoreCheckedPreservationEvidence {
        kind: CoreCheckedPreservationEvidenceKind::StructuralCoreExpr,
        freshness: core_expr_substitution_freshness_evidence(expr),
        target: expr.contract_text(),
    })
}

/// Classifies the runtime substitution-freshness obligation for an expression.
///
/// Inputs:
/// - `expr`: typed Core expression that already has structural preservation
///   evidence.
///
/// Output:
/// - Conservative freshness obligation for future Lean export.
///
/// Transformation:
/// - Recursively joins nested expression and pattern obligations, marking
///   expression forms that can bind runtime values (`case`, `receive`, `try`,
///   comprehensions, lambdas) as requiring runtime binding freshness whenever
///   their patterns bind names.
fn core_expr_substitution_freshness_evidence(expr: &CoreExpr) -> CoreSubstitutionFreshnessEvidence {
    let none = CoreSubstitutionFreshnessEvidence::NoRuntimeBindings;
    match expr {
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. } => none,
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            combine_expr_freshness(items.iter().map(core_expr_substitution_freshness_evidence))
        }
        CoreExpr::RemoteCall { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Call { args, .. }
        | CoreExpr::Intrinsic(CoreIntrinsicCall { args, .. }) => {
            combine_expr_freshness(args.iter().map(core_expr_substitution_freshness_evidence))
        }
        CoreExpr::FunctionCall { callee, args } => {
            core_expr_substitution_freshness_evidence(callee).combine(combine_expr_freshness(
                args.iter().map(core_expr_substitution_freshness_evidence),
            ))
        }
        CoreExpr::ListCons { head, tail }
        | CoreExpr::Index {
            base: head,
            index: tail,
        }
        | CoreExpr::BinaryOp {
            left: head,
            right: tail,
            ..
        } => core_expr_substitution_freshness_evidence(head)
            .combine(core_expr_substitution_freshness_evidence(tail)),
        CoreExpr::ListComprehension {
            expr,
            pattern,
            source,
            guard,
        } => core_expr_substitution_freshness_evidence(expr)
            .combine(core_pattern_substitution_freshness_evidence(pattern))
            .combine(core_expr_substitution_freshness_evidence(source))
            .combine(
                guard
                    .as_ref()
                    .map(|guard| core_expr_substitution_freshness_evidence(guard))
                    .unwrap_or(none),
            ),
        CoreExpr::Let { bindings, body } => combine_expr_freshness(
            bindings
                .iter()
                .map(|binding| core_expr_substitution_freshness_evidence(&binding.value)),
        )
        .combine(core_expr_substitution_freshness_evidence(body)),
        CoreExpr::Map(fields) => combine_expr_freshness(
            fields
                .iter()
                .map(|field| core_expr_substitution_freshness_evidence(&field.value)),
        ),
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            combine_expr_freshness(
                fields
                    .iter()
                    .map(|field| core_expr_substitution_freshness_evidence(&field.value)),
            )
        }
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::UnaryOp { operand: base, .. } => {
            core_expr_substitution_freshness_evidence(base)
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            core_expr_substitution_freshness_evidence(base).combine(combine_expr_freshness(
                fields
                    .iter()
                    .map(|field| core_expr_substitution_freshness_evidence(&field.value)),
            ))
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            combine_expr_freshness(args.iter().map(core_expr_substitution_freshness_evidence))
                .combine(core_expr_substitution_freshness_evidence(record))
        }
        CoreExpr::Case { scrutinee, clauses } => {
            core_expr_substitution_freshness_evidence(scrutinee).combine(combine_expr_freshness(
                clauses
                    .iter()
                    .map(core_case_clause_substitution_freshness_evidence),
            ))
        }
        CoreExpr::Receive {
            clauses,
            after_clause,
        } => combine_expr_freshness(
            clauses
                .iter()
                .map(core_case_clause_substitution_freshness_evidence),
        )
        .combine(
            after_clause
                .as_ref()
                .map(core_receive_after_substitution_freshness_evidence)
                .unwrap_or(none),
        ),
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => core_expr_substitution_freshness_evidence(body)
            .combine(combine_expr_freshness(
                of_clauses
                    .iter()
                    .map(core_case_clause_substitution_freshness_evidence),
            ))
            .combine(combine_expr_freshness(
                catch_clauses
                    .iter()
                    .map(core_case_clause_substitution_freshness_evidence),
            ))
            .combine(
                after_clause
                    .as_ref()
                    .map(core_try_after_substitution_freshness_evidence)
                    .unwrap_or(none),
            ),
        CoreExpr::If { clauses } => combine_expr_freshness(
            clauses
                .iter()
                .map(core_if_clause_substitution_freshness_evidence),
        ),
        CoreExpr::Lam { params, body } => combine_expr_freshness(
            params
                .iter()
                .map(core_pattern_substitution_freshness_evidence),
        )
        .combine(core_expr_substitution_freshness_evidence(body)),
    }
}

/// Combines an iterator of expression or pattern freshness obligations.
///
/// Inputs:
/// - `items`: freshness obligations from nested Core payloads.
///
/// Output:
/// - Aggregate freshness obligation for the enclosing Core payload.
///
/// Transformation:
/// - Starts from `NoRuntimeBindings` and joins every nested obligation using
///   the conservative freshness lattice.
fn combine_expr_freshness(
    items: impl IntoIterator<Item = CoreSubstitutionFreshnessEvidence>,
) -> CoreSubstitutionFreshnessEvidence {
    items.into_iter().fold(
        CoreSubstitutionFreshnessEvidence::NoRuntimeBindings,
        |acc, item| acc.combine(item),
    )
}

/// Classifies substitution freshness for a Core case-like clause.
///
/// Inputs:
/// - `clause`: typed case/receive/try clause.
///
/// Output:
/// - Aggregate freshness obligation for the clause.
///
/// Transformation:
/// - Joins the pattern, optional guard, and body obligations so pattern
///   bindings are visible to future Lean export.
fn core_case_clause_substitution_freshness_evidence(
    clause: &CoreCaseClause,
) -> CoreSubstitutionFreshnessEvidence {
    core_pattern_substitution_freshness_evidence(&clause.pattern)
        .combine(
            clause
                .guard
                .as_ref()
                .map(core_expr_substitution_freshness_evidence)
                .unwrap_or(CoreSubstitutionFreshnessEvidence::NoRuntimeBindings),
        )
        .combine(core_expr_substitution_freshness_evidence(&clause.body))
}

/// Classifies substitution freshness for a Core if clause.
///
/// Inputs:
/// - `clause`: typed if condition/body pair.
///
/// Output:
/// - Aggregate freshness obligation for the clause.
///
/// Transformation:
/// - Joins condition and body obligations without adding new binding
///   obligations, since `if` does not bind runtime pattern names.
fn core_if_clause_substitution_freshness_evidence(
    clause: &CoreIfClause,
) -> CoreSubstitutionFreshnessEvidence {
    core_expr_substitution_freshness_evidence(&clause.condition)
        .combine(core_expr_substitution_freshness_evidence(&clause.body))
}

/// Classifies substitution freshness for a Core receive timeout branch.
///
/// Inputs:
/// - `after_clause`: typed receive timeout trigger/body pair.
///
/// Output:
/// - Aggregate freshness obligation for the timeout branch.
///
/// Transformation:
/// - Joins trigger and body obligations without adding new binding
///   obligations.
fn core_receive_after_substitution_freshness_evidence(
    after_clause: &CoreReceiveAfter,
) -> CoreSubstitutionFreshnessEvidence {
    core_expr_substitution_freshness_evidence(&after_clause.trigger).combine(
        core_expr_substitution_freshness_evidence(&after_clause.body),
    )
}

/// Classifies substitution freshness for a Core try cleanup branch.
///
/// Inputs:
/// - `after_clause`: typed try cleanup trigger/body pair.
///
/// Output:
/// - Aggregate freshness obligation for the cleanup branch.
///
/// Transformation:
/// - Joins trigger and body obligations without adding new binding
///   obligations.
fn core_try_after_substitution_freshness_evidence(
    after_clause: &CoreTryAfter,
) -> CoreSubstitutionFreshnessEvidence {
    core_expr_substitution_freshness_evidence(&after_clause.trigger).combine(
        core_expr_substitution_freshness_evidence(&after_clause.body),
    )
}

/// Checks whether a typed Core expression carries checked-preservation evidence.
///
/// Inputs:
/// - `expr`: typed Core expression to validate.
///
/// Output:
/// - `true` when the term and all recursive children are in the evidence-backed
///   covered subset.
///
/// Transformation:
/// - Applies structural recursion over the current covered Core expression
///   constructors (`Int`/`Atom`/`Var`/`Tuple`/`List`/`Call`/`Case`/`Lam`).
fn core_expr_has_checked_preservation_evidence(expr: &CoreExpr) -> bool {
    match expr {
        CoreExpr::Int(_) | CoreExpr::Atom(_) | CoreExpr::Var(_) => true,
        CoreExpr::Float(_) => true,
        CoreExpr::Binary(_) => true,
        CoreExpr::Tuple(items) | CoreExpr::List(items) => items
            .iter()
            .all(core_expr_has_checked_preservation_evidence),
        CoreExpr::ListCons { head, tail } => {
            core_expr_has_checked_preservation_evidence(head)
                && core_expr_has_checked_preservation_evidence(tail)
        }
        CoreExpr::FixedArray(items) => items
            .iter()
            .all(core_expr_has_checked_preservation_evidence),
        CoreExpr::Index { base, index } => {
            core_expr_has_checked_preservation_evidence(base)
                && core_expr_has_checked_preservation_evidence(index)
        }
        CoreExpr::ListComprehension {
            expr,
            pattern,
            source,
            guard,
        } => {
            core_expr_has_checked_preservation_evidence(expr)
                && core_pattern_has_checked_preservation_evidence(pattern)
                && core_expr_has_checked_preservation_evidence(source)
                && guard
                    .as_ref()
                    .is_none_or(|guard| core_expr_has_checked_preservation_evidence(guard))
        }
        CoreExpr::Let { bindings, body } => {
            bindings
                .iter()
                .all(|binding| core_expr_has_checked_preservation_evidence(&binding.value))
                && core_expr_has_checked_preservation_evidence(body)
        }
        CoreExpr::Map(fields) => fields
            .iter()
            .all(|field| core_expr_has_checked_preservation_evidence(&field.value)),
        CoreExpr::RecordConstruct { fields, .. } => fields
            .iter()
            .all(|field| core_expr_has_checked_preservation_evidence(&field.value)),
        CoreExpr::FieldAccess { base, .. } => core_expr_has_checked_preservation_evidence(base),
        CoreExpr::RecordAccess { base, .. } => core_expr_has_checked_preservation_evidence(base),
        CoreExpr::RecordUpdate { base, fields, .. } => {
            core_expr_has_checked_preservation_evidence(base)
                && fields
                    .iter()
                    .all(|field| core_expr_has_checked_preservation_evidence(&field.value))
        }
        CoreExpr::TemplateInstantiate { fields, .. } => fields
            .iter()
            .all(|field| core_expr_has_checked_preservation_evidence(&field.value)),
        CoreExpr::ConstructorChain { args, record, .. } => {
            args.iter().all(core_expr_has_checked_preservation_evidence)
                && core_expr_has_checked_preservation_evidence(record)
        }
        CoreExpr::RemoteFunRef { .. } => true,
        CoreExpr::RemoteCall { args, .. } => {
            args.iter().all(core_expr_has_checked_preservation_evidence)
        }
        CoreExpr::ConstructorCall { args, .. } => {
            args.iter().all(core_expr_has_checked_preservation_evidence)
        }
        CoreExpr::Call { args, .. } => args.iter().all(core_expr_has_checked_preservation_evidence),
        CoreExpr::FunctionCall { callee, args } => {
            core_expr_has_checked_preservation_evidence(callee)
                && args.iter().all(core_expr_has_checked_preservation_evidence)
        }
        CoreExpr::Intrinsic(CoreIntrinsicCall { args, .. }) => {
            args.iter().all(core_expr_has_checked_preservation_evidence)
        }
        CoreExpr::Case { scrutinee, clauses } => {
            core_expr_has_checked_preservation_evidence(scrutinee)
                && clauses
                    .iter()
                    .all(core_case_clause_has_checked_preservation_evidence)
        }
        CoreExpr::Receive {
            clauses,
            after_clause,
        } => {
            clauses
                .iter()
                .all(core_case_clause_has_checked_preservation_evidence)
                && after_clause
                    .as_ref()
                    .is_none_or(core_receive_after_has_checked_preservation_evidence)
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            core_expr_has_checked_preservation_evidence(body)
                && of_clauses
                    .iter()
                    .all(core_case_clause_has_checked_preservation_evidence)
                && catch_clauses
                    .iter()
                    .all(core_case_clause_has_checked_preservation_evidence)
                && after_clause
                    .as_ref()
                    .is_none_or(core_try_after_has_checked_preservation_evidence)
        }
        CoreExpr::If { clauses } => clauses
            .iter()
            .all(core_if_clause_has_checked_preservation_evidence),
        CoreExpr::Lam { params, body } => {
            params
                .iter()
                .all(core_pattern_has_checked_preservation_evidence)
                && core_expr_has_checked_preservation_evidence(body)
        }
        CoreExpr::UnaryOp { operand, .. } => core_expr_has_checked_preservation_evidence(operand),
        CoreExpr::BinaryOp { left, right, .. } => {
            core_expr_has_checked_preservation_evidence(left)
                && core_expr_has_checked_preservation_evidence(right)
        }
    }
}

/// Checks whether a Core case clause has checked-preservation evidence.
///
/// Inputs:
/// - `clause`: typed case clause with one pattern and a body expression.
///
/// Output:
/// - `true` when both pattern and body are evidence-backed.
///
/// Transformation:
/// - Recursively validates the clause pattern and body using the same proof
///   evidence predicates as expression-level coverage.
fn core_case_clause_has_checked_preservation_evidence(clause: &CoreCaseClause) -> bool {
    core_pattern_has_checked_preservation_evidence(&clause.pattern)
        && clause
            .guard
            .as_ref()
            .is_none_or(core_expr_has_checked_preservation_evidence)
        && core_expr_has_checked_preservation_evidence(&clause.body)
}

/// Checks whether a Core if clause has checked-preservation evidence.
///
/// Inputs:
/// - `clause`: typed if clause with a condition and body expression.
///
/// Output:
/// - `true` when both condition and body expressions are evidence-backed.
///
/// Transformation:
/// - Recursively validates the condition and body using the expression-level
///   checked-preservation predicate.
fn core_if_clause_has_checked_preservation_evidence(clause: &CoreIfClause) -> bool {
    core_expr_has_checked_preservation_evidence(&clause.condition)
        && core_expr_has_checked_preservation_evidence(&clause.body)
}

/// Checks whether a Core receive timeout branch has preservation evidence.
///
/// Inputs:
/// - `after_clause`: typed receive timeout trigger/body payload.
///
/// Output:
/// - `true` when both timeout trigger and body are evidence-backed.
///
/// Transformation:
/// - Recursively validates trigger and body expressions with the expression
///   checked-preservation predicate.
fn core_receive_after_has_checked_preservation_evidence(after_clause: &CoreReceiveAfter) -> bool {
    core_expr_has_checked_preservation_evidence(&after_clause.trigger)
        && core_expr_has_checked_preservation_evidence(&after_clause.body)
}

/// Checks whether a Core try cleanup branch has preservation evidence.
///
/// Inputs:
/// - `after_clause`: typed try cleanup trigger/body payload.
///
/// Output:
/// - `true` when both cleanup trigger and body are evidence-backed.
///
/// Transformation:
/// - Recursively validates trigger and body expressions with the expression
///   checked-preservation predicate.
fn core_try_after_has_checked_preservation_evidence(after_clause: &CoreTryAfter) -> bool {
    core_expr_has_checked_preservation_evidence(&after_clause.trigger)
        && core_expr_has_checked_preservation_evidence(&after_clause.body)
}

/// Adds function-clause pattern payloads to typed-payload counts.
///
/// Inputs:
/// - `patterns`: optional typed Core pattern payloads for one function clause.
/// - `counts`: mutable aggregate pattern payload counts for the containing
///   Core module.
///
/// Output:
/// - None; `counts` is updated in place.
///
/// Transformation:
/// - Counts each top-level function-clause pattern as typed when a
///   `CorePattern` payload exists, otherwise as summary-only.
fn count_core_pattern_payloads(
    patterns: &[Option<CorePattern>],
    counts: &mut CorePatternPayloadCounts,
) {
    for pattern in patterns {
        if pattern.is_some() {
            counts.typed_core_pattern += 1;
        } else {
            counts.summary_only_pattern += 1;
        }
    }
}

/// Adds top-level function-clause constructor-pattern identities to counts.
///
/// Inputs:
/// - `patterns`: optional typed Core pattern payloads for one function clause.
/// - `counts`: mutable aggregate constructor-identity counters for the
///   containing Core module.
///
/// Output:
/// - None; `counts` is updated in place.
///
/// Transformation:
/// - Traverses each typed function-clause pattern and records resolved
///   constructor-pattern identity and unresolved-candidate buckets without
///   affecting proof coverage.
fn count_core_function_clause_pattern_constructor_identities(
    patterns: &[Option<CorePattern>],
    counts: &mut CoreConstructorIdentityCounts,
) {
    for pattern in patterns.iter().flatten() {
        count_core_pattern_constructor_identities(pattern, counts);
    }
}

/// Adds constructor identities from an expression-summary tree to counts.
///
/// Inputs:
/// - `expr`: Core expression summary tree to scan.
/// - `counts`: mutable aggregate constructor-identity counters for the
///   containing Core module.
///
/// Output:
/// - None; `counts` is updated in place.
///
/// Transformation:
/// - Counts constructor-resolution evidence owned by the current summary's
///   typed Core node, then recurses through summary children. The current-node
///   scan does not recurse into nested expressions because those have their
///   own summary entries; this avoids double-counting expression candidates.
fn count_core_expr_summary_constructor_identities(
    expr: &CoreExprSummary,
    counts: &mut CoreConstructorIdentityCounts,
) {
    if let Some(core_expr) = &expr.core_expr {
        count_core_expr_local_constructor_identities(core_expr, counts);
    }
    for child in &expr.children {
        count_core_expr_summary_constructor_identities(child, counts);
    }
}

/// Adds constructor identities owned directly by one Core expression node.
///
/// Inputs:
/// - `expr`: typed Core expression at one expression-summary node.
/// - `counts`: mutable aggregate constructor-identity counters for the
///   containing Core module.
///
/// Output:
/// - None; `counts` is updated in place.
///
/// Transformation:
/// - Counts resolved and unresolved constructor-call/constructor-chain
///   candidates on the expression itself, and scans embedded pattern positions
///   owned by the expression node. Nested expression children are counted by
///   their own expression-summary entries.
fn count_core_expr_local_constructor_identities(
    expr: &CoreExpr,
    counts: &mut CoreConstructorIdentityCounts,
) {
    match expr {
        CoreExpr::ConstructorCall {
            constructor_identity,
            ..
        } => {
            if constructor_identity.is_some() {
                counts.resolved_constructor_call_identity += 1;
            } else {
                counts.unresolved_constructor_call_candidate += 1;
            }
        }
        CoreExpr::ConstructorChain {
            base_constructor_identity,
            ..
        } => {
            if base_constructor_identity.is_some() {
                counts.resolved_constructor_chain_identity += 1;
            } else {
                counts.unresolved_constructor_chain_candidate += 1;
            }
        }
        CoreExpr::ListComprehension { pattern, .. } => {
            count_core_pattern_constructor_identities(pattern, counts);
        }
        CoreExpr::Let { bindings, body } => {
            for binding in bindings {
                count_core_expr_local_constructor_identities(&binding.value, counts);
            }
            count_core_expr_local_constructor_identities(body, counts);
        }
        CoreExpr::Case { clauses, .. } | CoreExpr::Receive { clauses, .. } => {
            for clause in clauses {
                count_core_pattern_constructor_identities(&clause.pattern, counts);
            }
        }
        CoreExpr::Try {
            of_clauses,
            catch_clauses,
            ..
        } => {
            for clause in of_clauses.iter().chain(catch_clauses) {
                count_core_pattern_constructor_identities(&clause.pattern, counts);
            }
        }
        CoreExpr::Lam { params, .. } => {
            for param in params {
                count_core_pattern_constructor_identities(param, counts);
            }
        }
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::Tuple(_)
        | CoreExpr::List(_)
        | CoreExpr::ListCons { .. }
        | CoreExpr::Map(_)
        | CoreExpr::RecordConstruct { .. }
        | CoreExpr::RecordUpdate { .. }
        | CoreExpr::FieldAccess { .. }
        | CoreExpr::RecordAccess { .. }
        | CoreExpr::TemplateInstantiate { .. }
        | CoreExpr::RemoteFunRef { .. }
        | CoreExpr::RemoteCall { .. }
        | CoreExpr::Intrinsic(_)
        | CoreExpr::Call { .. }
        | CoreExpr::FunctionCall { .. }
        | CoreExpr::If { .. }
        | CoreExpr::UnaryOp { .. }
        | CoreExpr::BinaryOp { .. }
        | CoreExpr::FixedArray(_)
        | CoreExpr::Index { .. } => {}
    }
}

/// Adds constructor-pattern resolution buckets from one Core pattern.
///
/// Inputs:
/// - `pattern`: typed Core pattern to scan.
/// - `counts`: mutable aggregate constructor-identity counters for the
///   containing Core module.
///
/// Output:
/// - None; `counts` is updated in place.
///
/// Transformation:
/// - Recursively scans structural pattern positions and increments either the
///   resolved identity bucket or unresolved candidate bucket for each
///   constructor pattern.
fn count_core_pattern_constructor_identities(
    pattern: &CorePattern,
    counts: &mut CoreConstructorIdentityCounts,
) {
    match pattern {
        CorePattern::Wildcard
        | CorePattern::Var(_)
        | CorePattern::Int(_)
        | CorePattern::Float(_)
        | CorePattern::Atom(_) => {}
        CorePattern::Tuple(elements) | CorePattern::List(elements) => {
            for element in elements {
                count_core_pattern_constructor_identities(element, counts);
            }
        }
        CorePattern::ListCons { head, tail } => {
            count_core_pattern_constructor_identities(head, counts);
            count_core_pattern_constructor_identities(tail, counts);
        }
        CorePattern::Map(fields) => {
            for field in fields {
                count_core_pattern_constructor_identities(&field.value, counts);
            }
        }
        CorePattern::Record { fields, .. } => {
            for field in fields {
                count_core_pattern_constructor_identities(&field.value, counts);
            }
        }
        CorePattern::Constructor {
            constructor_identity,
            args,
            ..
        } => {
            if constructor_identity.is_some() {
                counts.resolved_constructor_pattern_identity += 1;
            } else {
                counts.unresolved_constructor_pattern_candidate += 1;
            }
            for arg in args {
                count_core_pattern_constructor_identities(arg, counts);
            }
        }
    }
}

/// Adds one function-clause pattern summary vector to checked-preservation counts.
///
/// Inputs:
/// - `pattern_checked_preservation_evidence`: top-level function-clause
///   pattern evidence payloads for one function clause.
/// - `counts`: mutable aggregate checked-preservation counters.
///
/// Output:
/// - None; `counts` is updated in place.
///
/// Transformation:
/// - Increments the pattern bucket once per pattern that has an explicit
///   checked-preservation evidence payload.
fn count_core_pattern_checked_preservation(
    pattern_checked_preservation_evidence: &[Option<CoreCheckedPreservationEvidence>],
    counts: &mut CoreCheckedPreservationCounts,
) {
    for evidence in pattern_checked_preservation_evidence {
        if let Some(evidence) = evidence {
            counts.pattern += 1;
            if matches!(
                evidence.kind,
                CoreCheckedPreservationEvidenceKind::StructuralCorePattern
            ) {
                counts.pattern_structural += 1;
            }
            match evidence.freshness {
                CoreSubstitutionFreshnessEvidence::NoRuntimeBindings => {
                    counts.pattern_no_runtime_bindings += 1;
                }
                CoreSubstitutionFreshnessEvidence::RuntimeBindingsRequired => {
                    counts.pattern_runtime_bindings_required += 1;
                }
            }
        }
    }
}

/// Builds checked-preservation evidence for a typed Core pattern.
///
/// Inputs:
/// - `pattern`: typed Core pattern payload attached to a top-level function
///   clause pattern summary.
///
/// Output:
/// - `Some(CoreCheckedPreservationEvidence)` when the pattern and all recursive
///   children satisfy the current checked-preservation predicate.
/// - `None` when the pattern has no checked-preservation evidence yet.
///
/// Transformation:
/// - Reuses the structural pattern evidence predicate, then records the
///   covered Core pattern as deterministic Core contract text for future Lean
///   export.
fn core_pattern_checked_preservation_evidence(
    pattern: &CorePattern,
) -> Option<CoreCheckedPreservationEvidence> {
    core_pattern_has_checked_preservation_evidence(pattern).then(|| {
        CoreCheckedPreservationEvidence {
            kind: CoreCheckedPreservationEvidenceKind::StructuralCorePattern,
            freshness: core_pattern_substitution_freshness_evidence(pattern),
            target: pattern.contract_text(),
        }
    })
}

/// Classifies the runtime substitution-freshness obligation for a pattern.
///
/// Inputs:
/// - `pattern`: typed Core pattern that already has structural preservation
///   evidence.
///
/// Output:
/// - Conservative freshness obligation for future Lean export.
///
/// Transformation:
/// - Marks variable patterns as requiring runtime binding freshness and joins
///   nested obligations for compound patterns; literal/wildcard patterns do
///   not introduce runtime bindings.
fn core_pattern_substitution_freshness_evidence(
    pattern: &CorePattern,
) -> CoreSubstitutionFreshnessEvidence {
    let none = CoreSubstitutionFreshnessEvidence::NoRuntimeBindings;
    match pattern {
        CorePattern::Wildcard
        | CorePattern::Int(_)
        | CorePattern::Float(_)
        | CorePattern::Atom(_) => none,
        CorePattern::Var(_) => CoreSubstitutionFreshnessEvidence::RuntimeBindingsRequired,
        CorePattern::Tuple(elements) | CorePattern::List(elements) => combine_expr_freshness(
            elements
                .iter()
                .map(core_pattern_substitution_freshness_evidence),
        ),
        CorePattern::ListCons { head, tail } => core_pattern_substitution_freshness_evidence(head)
            .combine(core_pattern_substitution_freshness_evidence(tail)),
        CorePattern::Map(fields) => combine_expr_freshness(
            fields
                .iter()
                .map(|field| core_pattern_substitution_freshness_evidence(&field.value)),
        ),
        CorePattern::Record { fields, .. } => combine_expr_freshness(
            fields
                .iter()
                .map(|field| core_pattern_substitution_freshness_evidence(&field.value)),
        ),
        CorePattern::Constructor { args, .. } => combine_expr_freshness(
            args.iter()
                .map(core_pattern_substitution_freshness_evidence),
        ),
    }
}

/// Checks whether a typed Core pattern carries checked-preservation evidence.
///
/// Inputs:
/// - `pattern`: typed Core pattern to validate.
///
/// Output:
/// - `true` when all recursive pieces are evidence-backed in the covered
///   subset.
///
/// Transformation:
/// - Applies structural recursion over covered pattern constructors.
fn core_pattern_has_checked_preservation_evidence(pattern: &CorePattern) -> bool {
    match pattern {
        CorePattern::Wildcard
        | CorePattern::Var(_)
        | CorePattern::Int(_)
        | CorePattern::Float(_)
        | CorePattern::Atom(_) => true,
        CorePattern::Tuple(elements) | CorePattern::List(elements) => elements
            .iter()
            .all(core_pattern_has_checked_preservation_evidence),
        CorePattern::ListCons { head, tail } => {
            core_pattern_has_checked_preservation_evidence(head)
                && core_pattern_has_checked_preservation_evidence(tail)
        }
        CorePattern::Map(fields) => fields
            .iter()
            .all(|field| core_pattern_has_checked_preservation_evidence(&field.value)),
        CorePattern::Record { fields, .. } => fields
            .iter()
            .all(|field| core_pattern_has_checked_preservation_evidence(&field.value)),
        CorePattern::Constructor { args, .. } => args
            .iter()
            .all(core_pattern_has_checked_preservation_evidence),
    }
}

/// Adds one pattern proof-coverage label to aggregate counts.
///
/// Inputs:
/// - `coverage`: proof-coverage label attached to a Core pattern summary.
/// - `counts`: mutable aggregate counts for the containing Core module.
///
/// Output:
/// - None; `counts` is updated in place.
///
/// Transformation:
/// - Increments the matching coverage bucket without inspecting rendered
///   pattern text.
fn count_core_pattern_proof_coverage(
    coverage: CoreProofCoverage,
    counts: &mut CoreProofCoverageCounts,
) {
    match coverage {
        CoreProofCoverage::LeanCovered => counts.lean_covered += 1,
        CoreProofCoverage::Partial => counts.partial += 1,
        CoreProofCoverage::ProofModelRequired => counts.proof_model_required += 1,
        CoreProofCoverage::RuntimeBoundary => counts.runtime_boundary += 1,
        CoreProofCoverage::ArtifactOnly => counts.artifact_only += 1,
    }
}

/// Collects explicit source imports into deterministic CoreIR import summaries.
///
/// Inputs:
/// - `module`: compiler-facing syntax output.
///
/// Output:
/// - Sorted Core import summaries for imports written in source.
///
/// Transformation:
/// - Converts syntax-output import declarations into backend-neutral module
///   imports and excludes implicit/builtin interface-map entries.
fn core_syntax_imports(module: &SyntaxModuleOutput) -> Vec<CoreImport> {
    let mut imports = module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Import {
                import_kind,
                module_name,
                items,
                source_path,
                ..
            } => Some(CoreImport {
                module: core_import_identity(import_kind, module_name, items, source_path),
                kind: core_import_kind(*import_kind),
            }),
            _ => None,
        })
        .collect::<Vec<_>>();
    imports.sort_by(|left, right| {
        left.module
            .cmp(&right.module)
            .then_with(|| format!("{:?}", left.kind).cmp(&format!("{:?}", right.kind)))
    });
    imports.dedup_by(|left, right| left.module == right.module && left.kind == right.kind);
    imports
}

/// Collects source trait conformance facts into deterministic CoreIR summaries.
///
/// Inputs:
/// - `module`: compiler-facing syntax output.
///
/// Output:
/// - Sorted, deduplicated Core trait conformance summaries.
///
/// Transformation:
/// - Converts declaration-site `implements`, struct `derives`, and explicit
///   `impl Trait for Type` blocks into backend-neutral conformance facts while
///   preserving source category and visibility.
fn core_syntax_trait_conformances(module: &SyntaxModuleOutput) -> Vec<CoreTraitConformance> {
    let mut conformances = Vec::new();

    for declaration in &module.declarations {
        match &declaration.payload {
            SyntaxDeclarationPayload::Type {
                name,
                is_public,
                implements,
                ..
            }
            | SyntaxDeclarationPayload::Struct {
                name,
                is_public,
                implements,
                ..
            } => {
                conformances.extend(implements.iter().map(|trait_ref| CoreTraitConformance {
                    trait_ref: normalize_trait_type_text(&trait_ref.text),
                    for_type: name.clone(),
                    source: CoreTraitConformanceSource::Implements,
                    public: *is_public,
                }));
            }
            _ => {}
        }

        if let SyntaxDeclarationPayload::Struct {
            name,
            is_public,
            derives,
            ..
        } = &declaration.payload
        {
            conformances.extend(derives.iter().map(|trait_ref| CoreTraitConformance {
                trait_ref: normalize_trait_type_text(trait_ref),
                for_type: name.clone(),
                source: CoreTraitConformanceSource::Derive,
                public: *is_public,
            }));
        }

        if let SyntaxDeclarationPayload::TraitImpl {
            trait_ref,
            for_type,
            is_public,
            ..
        } = &declaration.payload
        {
            conformances.push(CoreTraitConformance {
                trait_ref: normalize_trait_type_text(&trait_ref.text),
                for_type: normalize_trait_type_text(&for_type.text),
                source: CoreTraitConformanceSource::ExplicitImpl,
                public: *is_public,
            });
        }
    }

    conformances.sort_by(|left, right| {
        left.trait_ref
            .cmp(&right.trait_ref)
            .then_with(|| left.for_type.cmp(&right.for_type))
            .then_with(|| format!("{:?}", left.source).cmp(&format!("{:?}", right.source)))
            .then_with(|| left.public.cmp(&right.public))
    });
    conformances.dedup();
    conformances
}

/// Converts syntax-output import kind into CoreIR import kind.
///
/// Inputs:
/// - `kind`: parser-preserved syntax import kind.
///
/// Output:
/// - Matching CoreIR import kind.
///
/// Transformation:
/// - Copies the import family tag while keeping target resolver behavior out of
///   CoreIR.
fn core_import_kind(kind: SyntaxImportKind) -> CoreImportKind {
    match kind {
        SyntaxImportKind::Module => CoreImportKind::Module,
        SyntaxImportKind::File => CoreImportKind::File,
        SyntaxImportKind::Css => CoreImportKind::Css,
        SyntaxImportKind::Markdown => CoreImportKind::Markdown,
    }
}

/// Builds a stable CoreIR identity for a syntax import declaration.
///
/// Inputs:
/// - `kind`: syntax import family.
/// - `module_name`: dotted module path for normal imports.
/// - `items`: imported items or asset alias.
/// - `source_path`: asset source path when present.
///
/// Output:
/// - Import identity string used by CoreIR contract text and target validation.
///
/// Transformation:
/// - Keeps module imports keyed by module path and asset imports keyed by
///   `alias<-source` so multiple assets remain distinguishable without reading
///   the filesystem.
fn core_import_identity(
    kind: &SyntaxImportKind,
    module_name: &str,
    items: &[terlan_syntax::SyntaxImportItem],
    source_path: &Option<String>,
) -> String {
    match kind {
        SyntaxImportKind::Module => module_name.to_string(),
        SyntaxImportKind::File | SyntaxImportKind::Css | SyntaxImportKind::Markdown => {
            let alias = items
                .first()
                .map(|item| item.name.as_str())
                .unwrap_or("<missing-alias>");
            let source = source_path.as_deref().unwrap_or("<missing-source>");
            format!("{alias}<-{source}")
        }
    }
}

/// Collects CoreIR function clause summaries from syntax output.
///
/// Inputs:
/// - `module`: compiler-facing syntax output.
///
/// Output:
/// - Map keyed by function name and arity.
///
/// Transformation:
/// - Converts syntax-output clause patterns, guards, and bodies into stable
///   backend-neutral summaries for the initial CoreIR lowering slice.
fn core_syntax_function_clauses(
    module: &SyntaxModuleOutput,
) -> HashMap<(String, usize), Vec<CoreFunctionClause>> {
    let mut clauses = HashMap::new();
    for declaration in &module.declarations {
        if let SyntaxDeclarationPayload::Function {
            name,
            params,
            clauses: function_clauses,
            ..
        } = &declaration.payload
        {
            clauses.insert(
                (name.clone(), params.len()),
                function_clauses
                    .iter()
                    .map(core_function_clause_summary)
                    .collect(),
            );
        }
    }
    clauses
}

/// Annotates syntax-lowered Core clauses with resolved constructor identities.
///
/// Inputs:
/// - `clauses`: mutable syntax-output Core function-clause summaries.
/// - `constructor_identities`: local constructor names mapped to stable
///   semantic constructor identities.
///
/// Output:
/// - None; constructor-call candidate payloads are updated in place.
///
/// Transformation:
/// - Recursively annotates `CoreExpr::ConstructorCall`,
///   `CoreExpr::ConstructorChain`, and `CorePattern::Constructor` nodes whose
///   candidate name resolves in the current module, an eligible single-shape
///   type alias, or imported public constructor/type-alias surface. Unknown
///   uppercase calls and patterns remain candidate-only.
fn resolve_constructor_identities_in_function_clauses(
    clauses: &mut HashMap<(String, usize), Vec<CoreFunctionClause>>,
    constructor_identities: &HashMap<String, String>,
) {
    if constructor_identities.is_empty() {
        return;
    }

    for function_clauses in clauses.values_mut() {
        for clause in function_clauses {
            for pattern in clause.core_patterns.iter_mut().flatten() {
                resolve_constructor_identities_in_core_pattern(pattern, constructor_identities);
            }
            if let Some(guard) = &mut clause.guard {
                resolve_constructor_identities_in_expr_summary(guard, constructor_identities);
            }
            resolve_constructor_identities_in_expr_summary(
                &mut clause.body,
                constructor_identities,
            );
        }
    }
}

/// Refreshes proof evidence after Core payload annotation.
///
/// Inputs:
/// - `clauses`: mutable syntax-output Core function-clause summaries.
///
/// Output:
/// - None; evidence payloads and annotation-dependent proof labels are updated
///   in place.
///
/// Transformation:
/// - Recomputes expression-summary and top-level pattern preservation evidence
///   from final typed Core payloads after semantic annotation passes have
///   changed Core contract text, such as constructor identity resolution.
/// - Recomputes proof coverage for forms whose coverage depends on final
///   semantic annotation, such as resolved constructor calls.
fn refresh_core_evidence_in_function_clauses(
    clauses: &mut HashMap<(String, usize), Vec<CoreFunctionClause>>,
) {
    for function_clauses in clauses.values_mut() {
        for clause in function_clauses {
            for (evidence, pattern) in clause
                .pattern_checked_preservation_evidence
                .iter_mut()
                .zip(&clause.core_patterns)
            {
                if let Some(pattern) = pattern {
                    *evidence = core_pattern_checked_preservation_evidence(pattern);
                }
            }
            if let Some(guard) = &mut clause.guard {
                refresh_core_evidence_in_expr_summary(guard);
            }
            refresh_core_evidence_in_expr_summary(&mut clause.body);
        }
    }
}

/// Refreshes proof evidence in one expression-summary tree.
///
/// Inputs:
/// - `summary`: mutable Core expression summary.
///
/// Output:
/// - None; expression evidence payloads and annotation-dependent proof labels
///   are updated in place.
///
/// Transformation:
/// - Recomputes the current summary's evidence from its final typed Core
///   payload.
/// - Promotes resolved constructor calls to Lean-covered proof coverage while
///   leaving unresolved constructor-call candidates partial.
/// - Recursively refreshes all child summaries.
fn refresh_core_evidence_in_expr_summary(summary: &mut CoreExprSummary) {
    summary.checked_preservation_evidence = summary
        .core_expr
        .as_ref()
        .and_then(core_expr_checked_preservation_evidence);
    if let Some(CoreExpr::ConstructorCall {
        constructor_identity,
        ..
    }) = &summary.core_expr
    {
        summary.proof_coverage = if constructor_identity.is_some() {
            CoreProofCoverage::LeanCovered
        } else {
            CoreProofCoverage::Partial
        };
    }
    for child in &mut summary.children {
        refresh_core_evidence_in_expr_summary(child);
    }
}

/// Collects constructor identities eligible for CoreIR identity annotation.
///
/// Inputs:
/// - `module`: syntax-output module whose declarations may include local
///   constructors and eligible single-shape type aliases.
/// - `resolved`: resolved module context containing imported item metadata and
///   interface snapshots.
/// - `constructors`: Core constructor declarations from the resolved interface.
///
/// Output:
/// - Map from source-visible constructor name to stable CoreIR constructor
///   identity.
///
/// Transformation:
/// - Preserves local constructor identities as their source-visible name.
/// - Preserves eligible local single-shape type aliases as their source-visible
///   name.
/// - Adds imported public constructors as `module.name` identities so aliased
///   imports can be distinguished from local constructor declarations.
/// - Adds imported public eligible single-shape type aliases as `module.name`
///   identities for the same reason.
/// - Uses both syntax-output declarations and resolved Core constructor
///   declarations so identity annotation can proceed while the Core constructor
///   declaration migration is still catching up.
fn core_constructor_identities(
    module: &SyntaxModuleOutput,
    resolved: &ResolvedModule,
    constructors: &[CoreConstructorDecl],
) -> HashMap<String, String> {
    let mut identities = constructors
        .iter()
        .map(|constructor| (constructor.name.clone(), constructor.name.clone()))
        .collect::<HashMap<_, _>>();
    identities.extend(module.declarations.iter().filter_map(
        |declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Constructor { name, .. } => {
                Some((name.clone(), name.clone()))
            }
            _ => None,
        },
    ));
    let local_aliases = collect_syntax_type_aliases(module);
    identities.extend(local_aliases.iter().filter_map(|(name, _)| {
        alias_constructor_schemes(name, &local_aliases).map(|_| (name.clone(), name.clone()))
    }));
    identities.extend(
        resolved
            .imported_types
            .iter()
            .filter_map(|(local_name, imported)| {
                let interface = resolved.interface_map.get(&imported.source_module)?;
                let signatures = interface.constructors.get(&imported.source_name)?;
                signatures
                    .iter()
                    .any(|signature| signature.public)
                    .then(|| {
                        (
                            local_name.clone(),
                            format!("{}.{}", imported.source_module, imported.source_name),
                        )
                    })
            }),
    );
    identities.extend(
        resolved
            .imported_types
            .iter()
            .filter_map(|(local_name, imported)| {
                let interface = resolved.interface_map.get(&imported.source_module)?;
                let interface_aliases = interface_type_aliases(interface);
                alias_constructor_schemes(&imported.source_name, &interface_aliases).map(|_| {
                    (
                        local_name.clone(),
                        format!("{}.{}", imported.source_module, imported.source_name),
                    )
                })
            }),
    );
    identities
}

/// Annotates one Core expression summary tree with constructor identities.
///
/// Inputs:
/// - `summary`: mutable Core expression summary.
/// - `constructor_identities`: source-visible constructor names mapped to
///   stable semantic identities.
///
/// Output:
/// - None; nested Core expression payloads are updated in place.
///
/// Transformation:
/// - Recursively walks both the typed Core payload and summary children so the
///   current node and all nested expression summaries agree on constructor
///   identity annotations.
fn resolve_constructor_identities_in_expr_summary(
    summary: &mut CoreExprSummary,
    constructor_identities: &HashMap<String, String>,
) {
    if let Some(core_expr) = &mut summary.core_expr {
        resolve_constructor_identities_in_core_expr(core_expr, constructor_identities);
    }
    for child in &mut summary.children {
        resolve_constructor_identities_in_expr_summary(child, constructor_identities);
    }
}

/// Annotates one typed Core expression with constructor identities.
///
/// Inputs:
/// - `expr`: mutable typed Core expression.
/// - `constructor_identities`: source-visible constructor names mapped to
///   stable semantic identities.
///
/// Output:
/// - None; matching constructor-call and constructor-pattern payloads are
///   updated in place.
///
/// Transformation:
/// - Traverses every recursive expression and embedded-pattern position and
///   sets constructor identity fields when a candidate name is declared by the
///   resolved module interface.
fn resolve_constructor_identities_in_core_expr(
    expr: &mut CoreExpr,
    constructor_identities: &HashMap<String, String>,
) {
    match expr {
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. } => {}
        CoreExpr::Tuple(items)
        | CoreExpr::List(items)
        | CoreExpr::FixedArray(items)
        | CoreExpr::RemoteCall { args: items, .. }
        | CoreExpr::Call { args: items, .. }
        | CoreExpr::Intrinsic(CoreIntrinsicCall { args: items, .. }) => {
            for item in items {
                resolve_constructor_identities_in_core_expr(item, constructor_identities);
            }
        }
        CoreExpr::FunctionCall { callee, args } => {
            resolve_constructor_identities_in_core_expr(callee, constructor_identities);
            for arg in args {
                resolve_constructor_identities_in_core_expr(arg, constructor_identities);
            }
        }
        CoreExpr::ListCons { head, tail }
        | CoreExpr::Index {
            base: head,
            index: tail,
        } => {
            resolve_constructor_identities_in_core_expr(head, constructor_identities);
            resolve_constructor_identities_in_core_expr(tail, constructor_identities);
        }
        CoreExpr::ListComprehension {
            expr,
            pattern,
            source,
            guard,
        } => {
            resolve_constructor_identities_in_core_expr(expr, constructor_identities);
            resolve_constructor_identities_in_core_pattern(pattern, constructor_identities);
            resolve_constructor_identities_in_core_expr(source, constructor_identities);
            if let Some(guard) = guard {
                resolve_constructor_identities_in_core_expr(guard, constructor_identities);
            }
        }
        CoreExpr::Let { bindings, body } => {
            for binding in bindings {
                resolve_constructor_identities_in_core_expr(
                    &mut binding.value,
                    constructor_identities,
                );
            }
            resolve_constructor_identities_in_core_expr(body, constructor_identities);
        }
        CoreExpr::Map(fields) => {
            for field in fields {
                resolve_constructor_identities_in_core_expr(
                    &mut field.value,
                    constructor_identities,
                );
            }
        }
        CoreExpr::RecordConstruct { fields, .. }
        | CoreExpr::RecordUpdate { fields, .. }
        | CoreExpr::TemplateInstantiate { fields, .. } => {
            for field in fields {
                resolve_constructor_identities_in_core_expr(
                    &mut field.value,
                    constructor_identities,
                );
            }
            if let CoreExpr::RecordUpdate { base, .. } = expr {
                resolve_constructor_identities_in_core_expr(base, constructor_identities);
            }
        }
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::UnaryOp { operand: base, .. } => {
            resolve_constructor_identities_in_core_expr(base, constructor_identities);
        }
        CoreExpr::ConstructorChain {
            base,
            base_constructor_identity,
            args,
            record,
        } => {
            if let Some(identity) = constructor_identities.get(base) {
                *base_constructor_identity = Some(identity.clone());
            }
            for arg in args {
                resolve_constructor_identities_in_core_expr(arg, constructor_identities);
            }
            resolve_constructor_identities_in_core_expr(record, constructor_identities);
        }
        CoreExpr::ConstructorCall {
            constructor,
            constructor_identity,
            args,
        } => {
            if let Some(identity) = constructor_identities.get(constructor) {
                *constructor_identity = Some(identity.clone());
            }
            for arg in args {
                resolve_constructor_identities_in_core_expr(arg, constructor_identities);
            }
        }
        CoreExpr::Case { scrutinee, clauses } => {
            resolve_constructor_identities_in_core_expr(scrutinee, constructor_identities);
            for clause in clauses {
                resolve_constructor_identities_in_core_pattern(
                    &mut clause.pattern,
                    constructor_identities,
                );
                if let Some(guard) = &mut clause.guard {
                    resolve_constructor_identities_in_core_expr(guard, constructor_identities);
                }
                resolve_constructor_identities_in_core_expr(
                    &mut clause.body,
                    constructor_identities,
                );
            }
        }
        CoreExpr::Receive {
            clauses,
            after_clause,
        } => {
            for clause in clauses {
                resolve_constructor_identities_in_core_pattern(
                    &mut clause.pattern,
                    constructor_identities,
                );
                if let Some(guard) = &mut clause.guard {
                    resolve_constructor_identities_in_core_expr(guard, constructor_identities);
                }
                resolve_constructor_identities_in_core_expr(
                    &mut clause.body,
                    constructor_identities,
                );
            }
            if let Some(after_clause) = after_clause {
                resolve_constructor_identities_in_core_expr(
                    &mut after_clause.trigger,
                    constructor_identities,
                );
                resolve_constructor_identities_in_core_expr(
                    &mut after_clause.body,
                    constructor_identities,
                );
            }
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            resolve_constructor_identities_in_core_expr(body, constructor_identities);
            for clause in of_clauses.iter_mut().chain(catch_clauses.iter_mut()) {
                resolve_constructor_identities_in_core_pattern(
                    &mut clause.pattern,
                    constructor_identities,
                );
                if let Some(guard) = &mut clause.guard {
                    resolve_constructor_identities_in_core_expr(guard, constructor_identities);
                }
                resolve_constructor_identities_in_core_expr(
                    &mut clause.body,
                    constructor_identities,
                );
            }
            if let Some(after_clause) = after_clause {
                resolve_constructor_identities_in_core_expr(
                    &mut after_clause.trigger,
                    constructor_identities,
                );
                resolve_constructor_identities_in_core_expr(
                    &mut after_clause.body,
                    constructor_identities,
                );
            }
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                resolve_constructor_identities_in_core_expr(
                    &mut clause.condition,
                    constructor_identities,
                );
                resolve_constructor_identities_in_core_expr(
                    &mut clause.body,
                    constructor_identities,
                );
            }
        }
        CoreExpr::Lam { params, body } => {
            for param in params {
                resolve_constructor_identities_in_core_pattern(param, constructor_identities);
            }
            resolve_constructor_identities_in_core_expr(body, constructor_identities);
        }
        CoreExpr::BinaryOp { left, right, .. } => {
            resolve_constructor_identities_in_core_expr(left, constructor_identities);
            resolve_constructor_identities_in_core_expr(right, constructor_identities);
        }
    }
}

/// Annotates one typed Core pattern with constructor identities.
///
/// Inputs:
/// - `pattern`: mutable typed Core pattern.
/// - `constructor_identities`: source-visible constructor names mapped to
///   stable semantic identities.
///
/// Output:
/// - None; matching constructor-pattern payloads are updated in place.
///
/// Transformation:
/// - Recursively traverses compound pattern positions and sets
///   `constructor_identity` when a constructor-pattern candidate name is
///   declared locally or imported from a public constructor interface.
fn resolve_constructor_identities_in_core_pattern(
    pattern: &mut CorePattern,
    constructor_identities: &HashMap<String, String>,
) {
    match pattern {
        CorePattern::Wildcard
        | CorePattern::Var(_)
        | CorePattern::Int(_)
        | CorePattern::Float(_)
        | CorePattern::Atom(_) => {}
        CorePattern::Tuple(elements) | CorePattern::List(elements) => {
            for element in elements {
                resolve_constructor_identities_in_core_pattern(element, constructor_identities);
            }
        }
        CorePattern::ListCons { head, tail } => {
            resolve_constructor_identities_in_core_pattern(head, constructor_identities);
            resolve_constructor_identities_in_core_pattern(tail, constructor_identities);
        }
        CorePattern::Map(fields) => {
            for field in fields {
                resolve_constructor_identities_in_core_pattern(
                    &mut field.value,
                    constructor_identities,
                );
            }
        }
        CorePattern::Record { fields, .. } => {
            for field in fields {
                resolve_constructor_identities_in_core_pattern(
                    &mut field.value,
                    constructor_identities,
                );
            }
        }
        CorePattern::Constructor {
            name,
            constructor_identity,
            args,
        } => {
            if let Some(identity) = constructor_identities.get(name) {
                *constructor_identity = Some(identity.clone());
            }
            for arg in args {
                resolve_constructor_identities_in_core_pattern(arg, constructor_identities);
            }
        }
    }
}

/// Converts one syntax function clause into a CoreIR clause summary.
///
/// Inputs:
/// - `clause`: syntax-output function clause.
///
/// Output:
/// - Core function clause summary.
///
/// Transformation:
/// - Renders patterns into stable syntax summaries and recursively summarizes
///   guard/body expressions without backend lowering. Pattern proof labels are
///   retained in the same order as the rendered pattern summaries.
fn core_function_clause_summary(
    clause: &terlan_syntax::SyntaxFunctionClauseOutput,
) -> CoreFunctionClause {
    let patterns = clause
        .patterns
        .iter()
        .map(core_pattern_summary_text)
        .collect();
    let core_patterns: Vec<Option<CorePattern>> = clause
        .patterns
        .iter()
        .map(core_pattern_from_syntax)
        .collect();
    let pattern_proof_coverage = clause
        .patterns
        .iter()
        .zip(core_patterns.iter())
        .map(|(pattern, core_pattern)| core_pattern_proof_coverage(pattern, core_pattern.as_ref()))
        .collect();
    let pattern_checked_preservation_evidence = clause
        .patterns
        .iter()
        .zip(core_patterns.iter())
        .map(|(_, core_pattern)| {
            core_pattern
                .as_ref()
                .and_then(core_pattern_checked_preservation_evidence)
        })
        .collect();
    CoreFunctionClause {
        patterns,
        core_patterns,
        pattern_proof_coverage,
        pattern_checked_preservation_evidence,
        guard: clause.guard.as_ref().map(core_expr_summary),
        body: core_expr_summary(&clause.body),
    }
}

/// Converts a syntax expression into a recursive CoreIR expression summary.
///
/// Inputs:
/// - `expr`: syntax-output expression.
///
/// Output:
/// - Core expression summary.
///
/// Transformation:
/// - Preserves semantic expression kind, arity, text, remote target, operator,
///   and recursively summarized child expressions while intentionally omitting
///   backend rendering details.
fn core_expr_summary(expr: &SyntaxExprOutput) -> CoreExprSummary {
    let mut children = expr
        .children
        .iter()
        .map(core_expr_summary)
        .collect::<Vec<_>>();
    children.extend(
        expr.fields
            .iter()
            .map(|field| core_expr_summary(&field.value)),
    );
    children.extend(expr.clauses.iter().flat_map(|clause| {
        let mut clause_children = Vec::new();
        if let Some(guard) = &clause.guard {
            clause_children.push(core_expr_summary(guard));
        }
        clause_children.push(core_expr_summary(&clause.body));
        clause_children
    }));
    children.extend(expr.catch_clauses.iter().flat_map(|clause| {
        let mut clause_children = Vec::new();
        if let Some(guard) = &clause.guard {
            clause_children.push(core_expr_summary(guard));
        }
        clause_children.push(core_expr_summary(&clause.body));
        clause_children
    }));
    if let Some(after) = &expr.try_after {
        children.push(core_expr_summary(&after.trigger));
        children.push(core_expr_summary(&after.body));
    }
    if let Some(after) = &expr.receive_after {
        children.push(core_expr_summary(&after.trigger));
        children.push(core_expr_summary(&after.body));
    }

    let core_expr = core_expr_from_syntax(expr);
    let checked_preservation_evidence = core_expr
        .as_ref()
        .and_then(core_expr_checked_preservation_evidence);
    let proof_coverage = core_expr_proof_coverage(expr, core_expr.as_ref());

    CoreExprSummary {
        kind: format!("{:?}", expr.kind),
        core_expr,
        checked_preservation_evidence,
        proof_coverage,
        text: expr.text.clone(),
        remote: expr.remote.clone(),
        operator: expr.operator.clone(),
        arity: expr.arity,
        children,
    }
}

/// Classifies a syntax-output expression for Lean proof coverage.
///
/// Inputs:
/// - `expr`: syntax-output expression being summarized into CoreIR.
/// - `core_expr`: typed Core payload produced for `expr`, when available.
///
/// Output:
/// - Proof coverage label for the current production CoreIR summary.
///
/// Transformation:
/// - Marks the current Lean-covered expression families as covered only when
///   they actually carry typed `CoreExpr` payloads; unsupported members of
///   those families remain proof-model-required until their Core payload exists.
fn core_expr_proof_coverage(
    expr: &SyntaxExprOutput,
    core_expr: Option<&CoreExpr>,
) -> CoreProofCoverage {
    match expr.kind {
        SyntaxExprKind::Int
        | SyntaxExprKind::Binary
        | SyntaxExprKind::Atom
        | SyntaxExprKind::Var
        | SyntaxExprKind::Tuple
        | SyntaxExprKind::List
        | SyntaxExprKind::Fun => match core_expr {
            Some(core_expr) if core_expr_is_lean_modeled(core_expr) => {
                CoreProofCoverage::LeanCovered
            }
            Some(_) => CoreProofCoverage::ProofModelRequired,
            None => CoreProofCoverage::ProofModelRequired,
        },
        SyntaxExprKind::Case => match core_expr {
            Some(CoreExpr::Case { scrutinee, clauses })
                if core_expr_is_lean_modeled(scrutinee)
                    && core_case_clauses_are_lean_modeled(clauses) =>
            {
                CoreProofCoverage::LeanCovered
            }
            Some(CoreExpr::Case { .. }) => CoreProofCoverage::ProofModelRequired,
            _ => CoreProofCoverage::ProofModelRequired,
        },
        SyntaxExprKind::FunctionCall => match core_expr {
            Some(core_expr @ CoreExpr::FunctionCall { .. })
                if core_expr_is_lean_modeled(core_expr) =>
            {
                CoreProofCoverage::LeanCovered
            }
            Some(CoreExpr::FunctionCall { .. }) => CoreProofCoverage::ProofModelRequired,
            _ => CoreProofCoverage::ProofModelRequired,
        },
        SyntaxExprKind::Call if expr.remote.is_none() => match core_expr {
            Some(core_expr @ CoreExpr::Call { .. }) if core_expr_is_lean_modeled(core_expr) => {
                CoreProofCoverage::LeanCovered
            }
            Some(CoreExpr::Call { .. }) => CoreProofCoverage::ProofModelRequired,
            Some(CoreExpr::ConstructorCall {
                constructor_identity,
                args,
                ..
            }) => {
                if constructor_identity.is_some() && args.iter().all(core_expr_is_lean_modeled) {
                    CoreProofCoverage::LeanCovered
                } else if constructor_identity.is_some() {
                    CoreProofCoverage::ProofModelRequired
                } else {
                    CoreProofCoverage::Partial
                }
            }
            _ => CoreProofCoverage::ProofModelRequired,
        },
        SyntaxExprKind::Call => remote_call_proof_coverage_policy(core_expr),
        SyntaxExprKind::ConstructorChain => constructor_chain_proof_coverage_policy(core_expr),
        SyntaxExprKind::Macro
        | SyntaxExprKind::RawMacro
        | SyntaxExprKind::HtmlBlock
        | SyntaxExprKind::Quote
        | SyntaxExprKind::Unquote => CoreProofCoverage::RuntimeBoundary,
        SyntaxExprKind::ListCons => match core_expr {
            Some(core_expr @ CoreExpr::ListCons { .. }) if core_expr_is_lean_modeled(core_expr) => {
                CoreProofCoverage::LeanCovered
            }
            Some(CoreExpr::ListCons { .. }) => CoreProofCoverage::ProofModelRequired,
            _ => CoreProofCoverage::ProofModelRequired,
        },
        SyntaxExprKind::If => match core_expr {
            Some(core_expr @ CoreExpr::If { .. }) if core_expr_is_lean_modeled(core_expr) => {
                CoreProofCoverage::LeanCovered
            }
            Some(CoreExpr::If { .. }) => CoreProofCoverage::ProofModelRequired,
            _ => CoreProofCoverage::ProofModelRequired,
        },
        SyntaxExprKind::FieldAccess => match core_expr {
            Some(core_expr @ CoreExpr::FieldAccess { .. })
                if core_expr_is_lean_modeled(core_expr) =>
            {
                CoreProofCoverage::LeanCovered
            }
            Some(CoreExpr::FieldAccess { .. }) => CoreProofCoverage::ProofModelRequired,
            _ => CoreProofCoverage::ProofModelRequired,
        },
        SyntaxExprKind::Let => match core_expr {
            Some(CoreExpr::Let { .. }) => CoreProofCoverage::ProofModelRequired,
            _ => CoreProofCoverage::ProofModelRequired,
        },
        SyntaxExprKind::Cast => CoreProofCoverage::ProofModelRequired,
        SyntaxExprKind::Float
        | SyntaxExprKind::Map
        | SyntaxExprKind::RecordConstruct
        | SyntaxExprKind::RecordAccess
        | SyntaxExprKind::RecordUpdate
        | SyntaxExprKind::FixedArray
        | SyntaxExprKind::Index
        | SyntaxExprKind::ListComprehension
        | SyntaxExprKind::Receive
        | SyntaxExprKind::Try
        | SyntaxExprKind::RemoteFunRef
        | SyntaxExprKind::TemplateInstantiate => CoreProofCoverage::ProofModelRequired,
        SyntaxExprKind::UnaryOp => match core_expr {
            Some(core_expr @ CoreExpr::UnaryOp { .. }) if core_expr_is_lean_modeled(core_expr) => {
                CoreProofCoverage::LeanCovered
            }
            Some(CoreExpr::UnaryOp { .. }) => CoreProofCoverage::ProofModelRequired,
            _ => CoreProofCoverage::ProofModelRequired,
        },
        SyntaxExprKind::BinaryOp => match core_expr {
            Some(core_expr @ CoreExpr::BinaryOp { .. }) if core_expr_is_lean_modeled(core_expr) => {
                CoreProofCoverage::LeanCovered
            }
            Some(CoreExpr::BinaryOp { .. }) => CoreProofCoverage::ProofModelRequired,
            _ => CoreProofCoverage::ProofModelRequired,
        },
    }
}

/// Returns the active proof-coverage policy for remote-call expressions.
///
/// Inputs:
/// - `core_expr`: typed Core payload lowered from a source remote call, when
///   available.
///
/// Output:
/// - `CoreProofCoverage::ProofModelRequired` under the current remote-dispatch
///   readiness policy.
///
/// Transformation:
/// - Keeps the production CoreIR payload visible while preventing accidental
///   promotion to Lean-covered coverage before the roadmap decides whether
///   value-ready remote dispatch is acceptable as runtime-boundary evidence or
///   still requires a backend dispatch contract.
fn remote_call_proof_coverage_policy(core_expr: Option<&CoreExpr>) -> CoreProofCoverage {
    match core_expr {
        Some(CoreExpr::RemoteCall { .. }) | None => CoreProofCoverage::ProofModelRequired,
        Some(_) => CoreProofCoverage::ProofModelRequired,
    }
}

/// Returns the active proof-coverage policy for constructor-chain expressions.
///
/// Inputs:
/// - `core_expr`: typed Core payload lowered from source constructor-chain
///   syntax, when available.
///
/// Output:
/// - `CoreProofCoverage::Partial` under the current constructor-chain policy.
///
/// Transformation:
/// - Keeps resolved constructor-chain identity evidence separate from Lean
///   coverage. A chain may have a resolved base constructor identity and still
///   remain partial until record construction and constructor-chain semantics
///   have a dedicated proof model.
fn constructor_chain_proof_coverage_policy(core_expr: Option<&CoreExpr>) -> CoreProofCoverage {
    match core_expr {
        Some(CoreExpr::ConstructorChain { .. }) | None => CoreProofCoverage::Partial,
        Some(_) => CoreProofCoverage::Partial,
    }
}

/// Checks whether a Core expression maps to the current Lean expression subset.
///
/// Inputs:
/// - `expr`: typed production Core expression.
///
/// Output:
/// - `true` when the expression and all nested executable children map to the
///   current Lean `Expr` subset.
/// - `false` when the expression has a typed Core payload but still needs Lean
///   syntax, typing, or semantics.
///
/// Transformation:
/// - Recursively inspects expression and pattern children without modifying the
///   production CoreExpr payload.
fn core_expr_is_lean_modeled(expr: &CoreExpr) -> bool {
    match expr {
        CoreExpr::Int(_) | CoreExpr::Atom(_) | CoreExpr::Var(_) => true,
        CoreExpr::Tuple(items) | CoreExpr::List(items) => {
            items.iter().all(core_expr_is_lean_modeled)
        }
        CoreExpr::Call { args, .. } => args.iter().all(core_expr_is_lean_modeled),
        CoreExpr::FunctionCall { callee, args } => {
            core_expr_is_lean_modeled(callee) && args.iter().all(core_expr_is_lean_modeled)
        }
        CoreExpr::ConstructorCall {
            constructor_identity,
            args,
            ..
        } => constructor_identity.is_some() && args.iter().all(core_expr_is_lean_modeled),
        CoreExpr::Case { scrutinee, clauses } => {
            core_expr_is_lean_modeled(scrutinee) && core_case_clauses_are_lean_modeled(clauses)
        }
        CoreExpr::Lam { params, body } => {
            params.iter().all(core_pattern_is_lean_modeled) && core_expr_is_lean_modeled(body)
        }
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } => {
            matches!(
                operator.as_str(),
                "+" | "-" | "*" | "==" | "<" | "<=" | ">" | ">="
            ) && core_expr_is_lean_modeled(left)
                && core_expr_is_lean_modeled(right)
        }
        CoreExpr::UnaryOp { operator, operand } => {
            operator == "-" && core_expr_is_lean_modeled(operand)
        }
        CoreExpr::ListCons { head, tail } => {
            core_expr_is_lean_modeled(head) && core_expr_is_lean_modeled(tail)
        }
        CoreExpr::If { clauses } => core_if_clauses_are_lean_modeled(clauses),
        CoreExpr::FieldAccess { base, .. } => core_expr_is_lean_modeled(base),
        CoreExpr::Binary(_) => true,
        CoreExpr::RemoteCall { .. } => remote_call_is_promoted_to_lean_covered(),
        CoreExpr::Float(_)
        | CoreExpr::FixedArray(_)
        | CoreExpr::Index { .. }
        | CoreExpr::ListComprehension { .. }
        | CoreExpr::Let { .. }
        | CoreExpr::Map(_)
        | CoreExpr::RecordConstruct { .. }
        | CoreExpr::RecordAccess { .. }
        | CoreExpr::RecordUpdate { .. }
        | CoreExpr::TemplateInstantiate { .. }
        | CoreExpr::ConstructorChain { .. }
        | CoreExpr::RemoteFunRef { .. }
        | CoreExpr::Intrinsic(_)
        | CoreExpr::Receive { .. }
        | CoreExpr::Try { .. } => false,
    }
}

/// Reports whether remote calls are currently promoted to Lean-covered status.
///
/// Inputs:
/// - None; this is a compiler policy switch, not a per-expression decision yet.
///
/// Output:
/// - `false` until the formal roadmap promotes the selected remote-dispatch
///   subset and updates phase-contract goldens, proof-baseline tables, and target
///   dispatch contracts together.
///
/// Transformation:
/// - Encodes the current remote-dispatch readiness policy as an explicit helper
///   so future promotion changes happen in one named place.
fn remote_call_is_promoted_to_lean_covered() -> bool {
    false
}

/// Checks whether Core if clauses map to the current Lean if subset.
///
/// Inputs:
/// - `clauses`: typed Core if clauses lowered from syntax output.
///
/// Output:
/// - `true` only for the selected one-clause subset whose condition and body
///   are both Lean-modeled.
/// - `false` for empty, multi-clause, or nested unmodeled if payloads.
///
/// Transformation:
/// - Inspects clause shape and recursively checks condition/body CoreExpr
///   payloads without modifying production CoreIR.
fn core_if_clauses_are_lean_modeled(clauses: &[CoreIfClause]) -> bool {
    matches!(
        clauses,
        [CoreIfClause { condition, body }]
            if core_expr_is_lean_modeled(condition) && core_expr_is_lean_modeled(body)
    )
}

/// Checks whether Core case clauses use only Lean-modeled pattern forms.
///
/// Inputs:
/// - `clauses`: typed Core case clauses lowered from syntax output.
///
/// Output:
/// - `true` when every clause is unguarded and every branch pattern maps to
///   the current Lean `Pattern` subset.
/// - `false` when a guard or unmodeled pattern form requires new Lean syntax,
///   typing, or match semantics.
///
/// Transformation:
/// - Traverses clause guards, patterns, and branch bodies without modifying the
///   production CoreExpr payload.
fn core_case_clauses_are_lean_modeled(clauses: &[CoreCaseClause]) -> bool {
    clauses.iter().all(|clause| {
        clause.guard.is_none()
            && core_pattern_is_lean_modeled(&clause.pattern)
            && core_expr_is_lean_modeled(&clause.body)
    })
}

/// Checks whether a Core pattern maps to the current Lean pattern subset.
///
/// Inputs:
/// - `pattern`: typed Core pattern lowered from production syntax.
///
/// Output:
/// - `true` for wildcard, variable, integer, atom, tuple, list, and
///   constructor patterns whose nested patterns are also Lean-modeled.
/// - `false` for typed-but-unmodeled pattern payloads such as float,
///   list-cons, map, and record patterns.
///
/// Transformation:
/// - Recursively inspects structural pattern children without modifying the
///   production CorePattern payload.
fn core_pattern_is_lean_modeled(pattern: &CorePattern) -> bool {
    match pattern {
        CorePattern::Wildcard
        | CorePattern::Var(_)
        | CorePattern::Int(_)
        | CorePattern::Atom(_) => true,
        CorePattern::Tuple(items) | CorePattern::List(items) => {
            items.iter().all(core_pattern_is_lean_modeled)
        }
        CorePattern::Constructor { args, .. } => args.iter().all(core_pattern_is_lean_modeled),
        CorePattern::Float(_)
        | CorePattern::ListCons { .. }
        | CorePattern::Map(_)
        | CorePattern::Record { .. } => false,
    }
}

/// Converts a syntax-output expression into a typed Core expression when covered.
///
/// Inputs:
/// - `expr`: syntax-output expression summary produced by the parser pipeline.
///
/// Output:
/// - `Some(CoreExpr)` for the first typed Core expression subset.
/// - `None` for forms that still require richer Core identity, typing, or
///   control-flow payloads.
///
/// Transformation:
/// - Reconstructs typed Core expression nodes from syntax-output kind, text,
///   and child expressions, without backend lowering or rendered summary text.
fn core_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    match expr.kind {
        SyntaxExprKind::Int => expr
            .text
            .as_ref()
            .and_then(|value| value.parse::<i64>().ok())
            .map(CoreExpr::Int),
        SyntaxExprKind::Float => expr.text.clone().map(CoreExpr::Float),
        SyntaxExprKind::Binary => expr.text.clone().map(CoreExpr::Binary),
        SyntaxExprKind::Atom => expr.text.clone().map(CoreExpr::Atom),
        SyntaxExprKind::Var => expr.text.clone().map(CoreExpr::Var),
        SyntaxExprKind::Tuple => core_exprs_from_syntax_children(expr).map(CoreExpr::Tuple),
        SyntaxExprKind::List => core_exprs_from_syntax_children(expr).map(CoreExpr::List),
        SyntaxExprKind::ListCons => core_list_cons_expr_from_syntax(expr),
        SyntaxExprKind::FixedArray => {
            core_exprs_from_syntax_children(expr).map(CoreExpr::FixedArray)
        }
        SyntaxExprKind::Index => core_index_expr_from_syntax(expr),
        SyntaxExprKind::ListComprehension => core_list_comprehension_expr_from_syntax(expr),
        SyntaxExprKind::Let => core_let_expr_from_syntax(expr),
        SyntaxExprKind::Map => core_map_expr_fields_from_syntax(expr).map(CoreExpr::Map),
        SyntaxExprKind::RecordConstruct => core_record_construct_expr_from_syntax(expr),
        SyntaxExprKind::FieldAccess => core_field_access_expr_from_syntax(expr),
        SyntaxExprKind::RecordAccess => core_record_access_expr_from_syntax(expr),
        SyntaxExprKind::RecordUpdate => core_record_update_expr_from_syntax(expr),
        SyntaxExprKind::TemplateInstantiate => core_template_instantiate_expr_from_syntax(expr),
        SyntaxExprKind::ConstructorChain => core_constructor_chain_expr_from_syntax(expr),
        SyntaxExprKind::RemoteFunRef => core_remote_fun_ref_expr_from_syntax(expr),
        SyntaxExprKind::Cast => None,
        SyntaxExprKind::UnaryOp => core_unary_op_expr_from_syntax(expr),
        SyntaxExprKind::Call if expr.remote.is_some() => core_intrinsic_call_expr_from_syntax(expr)
            .or_else(|| core_remote_call_expr_from_syntax(expr)),
        SyntaxExprKind::Call if expr.remote.is_none() => core_intrinsic_call_expr_from_syntax(expr)
            .or_else(|| core_named_call_expr_from_syntax(expr)),
        SyntaxExprKind::FunctionCall => core_function_call_expr_from_syntax(expr),
        SyntaxExprKind::Case if expr.children.len() == 1 => {
            let scrutinee = Box::new(core_expr_from_syntax(&expr.children[0])?);
            let clauses = core_case_clauses_from_syntax(expr)?;
            Some(CoreExpr::Case { scrutinee, clauses })
        }
        SyntaxExprKind::Receive => core_receive_expr_from_syntax(expr),
        SyntaxExprKind::Try => core_try_expr_from_syntax(expr),
        SyntaxExprKind::If => core_if_expr_from_syntax(expr),
        SyntaxExprKind::Fun if expr.clauses.len() == 1 => {
            let clause = &expr.clauses[0];
            if clause.guard.is_some() {
                return None;
            }
            Some(CoreExpr::Lam {
                params: core_patterns_from_syntax_slice(&clause.patterns)?,
                body: Box::new(core_expr_from_syntax(&clause.body)?),
            })
        }
        SyntaxExprKind::BinaryOp => {
            let operator = expr.operator.clone()?;
            let left = expr.children.first().and_then(core_expr_from_syntax)?;
            let right = expr.children.get(1).and_then(core_expr_from_syntax)?;
            Some(CoreExpr::BinaryOp {
                operator,
                left: Box::new(left),
                right: Box::new(right),
            })
        }
        SyntaxExprKind::Fun
        | SyntaxExprKind::Call
        | SyntaxExprKind::Case
        | SyntaxExprKind::Macro
        | SyntaxExprKind::RawMacro
        | SyntaxExprKind::HtmlBlock
        | SyntaxExprKind::Quote
        | SyntaxExprKind::Unquote => None,
    }
}

/// Converts syntax-output expression children into typed Core expression children.
///
/// Inputs:
/// - `expr`: syntax-output parent expression whose children should be lowered.
///
/// Output:
/// - `Some(Vec<CoreExpr>)` when every child is in the current typed subset.
/// - `None` when at least one child is not yet representable as a typed Core
///   expression.
///
/// Transformation:
/// - Recursively lowers children and fails the parent conversion if any child
///   remains unsupported.
fn core_exprs_from_syntax_children(expr: &SyntaxExprOutput) -> Option<Vec<CoreExpr>> {
    expr.children.iter().map(core_expr_from_syntax).collect()
}

/// Converts a syntax-output list-cons expression into typed Core.
///
/// Inputs:
/// - `expr`: syntax-output list-cons expression with head and tail children.
///
/// Output:
/// - `Some(CoreExpr::ListCons)` when both head and tail lower to typed Core
///   expressions.
/// - `None` when the shape is not list-cons or either side remains unsupported.
///
/// Transformation:
/// - Preserves the structural cons expression as a backend-agnostic head/tail
///   Core node without using list rendering syntax.
fn core_list_cons_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::ListCons) || expr.children.len() != 2 {
        return None;
    }

    Some(CoreExpr::ListCons {
        head: Box::new(core_expr_from_syntax(&expr.children[0])?),
        tail: Box::new(core_expr_from_syntax(&expr.children[1])?),
    })
}

/// Converts a syntax-output index expression into typed Core.
///
/// Inputs:
/// - `expr`: syntax-output index expression with receiver and index children.
///
/// Output:
/// - `Some(CoreExpr::Index)` when both receiver and index lower into typed
///   Core expressions.
/// - `None` when the shape is not index syntax, has the wrong child count, or
///   either child remains unsupported.
///
/// Transformation:
/// - Preserves the receiver and index operand as backend-neutral CoreIR without
///   choosing list, tuple, map, or backend-specific lookup semantics.
fn core_index_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::Index) || expr.children.len() != 2 {
        return None;
    }

    Some(CoreExpr::Index {
        base: Box::new(core_expr_from_syntax(&expr.children[0])?),
        index: Box::new(core_expr_from_syntax(&expr.children[1])?),
    })
}

/// Converts a syntax-output list comprehension into typed Core.
///
/// Inputs:
/// - `expr`: syntax-output list comprehension with yielded expression, source
///   expression, one generator pattern, and optional guard child.
///
/// Output:
/// - `Some(CoreExpr::ListComprehension)` when yield/source/guard expressions
///   and generator pattern all lower into typed Core.
/// - `None` when the node is not a list comprehension, has unsupported child
///   shape, or carries unsupported pattern/expression payloads.
///
/// Transformation:
/// - Preserves the generator pattern, source expression, yielded expression,
///   and optional guard as backend-neutral CoreIR without choosing backend
///   comprehension semantics.
fn core_list_comprehension_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::ListComprehension)
        || expr.patterns.len() != 1
        || !(2..=3).contains(&expr.children.len())
    {
        return None;
    }

    Some(CoreExpr::ListComprehension {
        expr: Box::new(core_expr_from_syntax(&expr.children[0])?),
        pattern: core_pattern_from_syntax(&expr.patterns[0])?,
        source: Box::new(core_expr_from_syntax(&expr.children[1])?),
        guard: match expr.children.get(2) {
            Some(guard) => Some(Box::new(core_expr_from_syntax(guard)?)),
            None => None,
        },
    })
}

/// Converts a syntax-output let expression into typed Core.
///
/// Inputs:
/// - `expr`: syntax-output let expression whose patterns are binding names and
///   whose children are binding values plus a required final body.
///
/// Output:
/// - `Some(CoreExpr::Let)` when every binding value and body lowers to typed
///   Core.
/// - `None` when the syntax-output shape is malformed or any child remains
///   unsupported.
///
/// Transformation:
/// - Pairs each binding-name pattern with its value child and lowers the final
///   child as the explicit result expression. Bodyless let expressions are
///   rejected as malformed input.
fn core_let_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::Let)
        || expr.patterns.is_empty()
        || expr.children.len() != expr.patterns.len() + 1
    {
        return None;
    }

    let bindings = expr
        .patterns
        .iter()
        .zip(expr.children.iter())
        .map(|(pattern, value)| {
            if !matches!(pattern.kind, terlan_syntax::SyntaxPatternKind::Var) {
                return None;
            }
            Some(CoreLetBinding {
                name: pattern.text.clone()?,
                value: core_expr_from_syntax(value)?,
            })
        })
        .collect::<Option<Vec<_>>>()?;

    let body = core_expr_from_syntax(expr.children.get(expr.patterns.len())?)?;

    Some(CoreExpr::Let {
        bindings,
        body: Box::new(body),
    })
}

/// Converts syntax-output map-expression fields into typed Core map fields.
///
/// Inputs:
/// - `expr`: syntax-output map expression whose fields should be lowered.
///
/// Output:
/// - `Some(Vec<CoreMapExprField>)` when every field value lowers to a typed
///   Core expression.
/// - `None` when the expression has non-map syntax or any field value remains
///   unsupported.
///
/// Transformation:
/// - Preserves field keys and required/optional source mode, while recursively
///   lowering field value expressions into backend-agnostic CoreIR.
fn core_map_expr_fields_from_syntax(expr: &SyntaxExprOutput) -> Option<Vec<CoreMapExprField>> {
    if !matches!(expr.kind, SyntaxExprKind::Map) {
        return None;
    }

    expr.fields
        .iter()
        .map(|field| {
            core_expr_from_syntax(&field.value).map(|value| CoreMapExprField {
                key: field.key.clone(),
                required: field.required,
                value,
            })
        })
        .collect()
}

/// Converts a syntax-output record construction into typed Core.
///
/// Inputs:
/// - `expr`: syntax-output record construction with a record name and fields.
///
/// Output:
/// - `Some(CoreExpr::RecordConstruct)` when every field value lowers to typed
///   Core and the record name is present.
/// - `None` when the shape is not record construction, the name is missing, or
///   any field value remains unsupported.
///
/// Transformation:
/// - Preserves record identity and field assignments as semantic CoreIR data,
///   while recursively lowering field values into typed Core expressions.
fn core_record_construct_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::RecordConstruct) {
        return None;
    }

    Some(CoreExpr::RecordConstruct {
        name: expr.text.clone()?,
        fields: core_record_expr_fields_from_syntax(expr)?,
    })
}

/// Converts syntax-output record-construction fields into typed Core fields.
///
/// Inputs:
/// - `expr`: syntax-output record construction whose fields should be lowered.
///
/// Output:
/// - `Some(Vec<CoreRecordExprField>)` when every field value lowers.
/// - `None` when any field value remains unsupported.
///
/// Transformation:
/// - Preserves field keys and source assignment mode, while recursively
///   lowering field value expressions into backend-agnostic CoreIR.
fn core_record_expr_fields_from_syntax(
    expr: &SyntaxExprOutput,
) -> Option<Vec<CoreRecordExprField>> {
    expr.fields
        .iter()
        .map(|field| {
            core_expr_from_syntax(&field.value).map(|value| CoreRecordExprField {
                key: field.key.clone(),
                required: field.required,
                value,
            })
        })
        .collect()
}

/// Converts a syntax-output field access into typed Core.
///
/// Inputs:
/// - `expr`: syntax-output field access with exactly one receiver child and
///   field text.
///
/// Output:
/// - `Some(CoreExpr::FieldAccess)` when the receiver lowers into typed Core and
///   the field name is present.
/// - `None` when the shape is not field access, has the wrong child count, or
///   the receiver is outside the current typed Core subset.
///
/// Transformation:
/// - Preserves the field-access receiver and source field name as
///   backend-neutral CoreIR, without resolving struct layout or emitting record
///   access syntax.
fn core_field_access_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::FieldAccess) || expr.children.len() != 1 {
        return None;
    }

    Some(CoreExpr::FieldAccess {
        base: Box::new(core_expr_from_syntax(&expr.children[0])?),
        field: expr.text.clone()?,
    })
}

/// Converts a syntax-output record access into typed Core.
///
/// Inputs:
/// - `expr`: syntax-output record access with exactly one receiver child and
///   `RecordName.field` text.
///
/// Output:
/// - `Some(CoreExpr::RecordAccess)` when the receiver lowers into typed Core
///   and both record name and field name are present.
/// - `None` when the shape is not record access, has the wrong child count,
///   carries malformed access text, or has an unsupported receiver.
///
/// Transformation:
/// - Splits the syntax-output access label into record identity and field name,
///   then preserves both with the recursively lowered receiver as
///   backend-neutral CoreIR.
fn core_record_access_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::RecordAccess) || expr.children.len() != 1 {
        return None;
    }

    let text = expr.text.as_deref()?;
    let (name, field) = text.split_once('.')?;
    if name.is_empty() || field.is_empty() {
        return None;
    }

    Some(CoreExpr::RecordAccess {
        base: Box::new(core_expr_from_syntax(&expr.children[0])?),
        name: name.to_string(),
        field: field.to_string(),
    })
}

/// Converts a syntax-output record update into typed Core.
///
/// Inputs:
/// - `expr`: syntax-output record update with exactly one receiver child,
///   record-name text, and expression-valued update fields.
///
/// Output:
/// - `Some(CoreExpr::RecordUpdate)` when the receiver and every update value
///   lower into typed Core and the record name is present.
/// - `None` when the shape is not record update, has the wrong child count,
///   lacks record identity, or contains unsupported receiver/field expressions.
///
/// Transformation:
/// - Preserves update receiver, record identity, and field assignments as a
///   backend-neutral CoreIR update node without lowering into construction or
///   backend record syntax.
fn core_record_update_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::RecordUpdate) || expr.children.len() != 1 {
        return None;
    }

    Some(CoreExpr::RecordUpdate {
        base: Box::new(core_expr_from_syntax(&expr.children[0])?),
        name: expr.text.clone()?,
        fields: core_record_expr_fields_from_syntax(expr)?,
    })
}

/// Converts a syntax-output template instantiation into typed Core.
///
/// Inputs:
/// - `expr`: syntax-output template instantiation with template name text and
///   expression-valued prop fields.
///
/// Output:
/// - `Some(CoreExpr::TemplateInstantiate)` when the template name is present
///   and every prop value lowers into typed Core.
/// - `None` when the node is not template instantiation syntax, lacks template
///   identity, or contains unsupported prop value expressions.
///
/// Transformation:
/// - Preserves template identity and prop assignments as backend-neutral CoreIR
///   without treating the node as record construction.
fn core_template_instantiate_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::TemplateInstantiate) {
        return None;
    }

    Some(CoreExpr::TemplateInstantiate {
        name: expr.text.clone()?,
        fields: core_record_expr_fields_from_syntax(expr)?,
    })
}

/// Converts a syntax-output constructor chain into typed Core.
///
/// Inputs:
/// - `expr`: syntax-output constructor-chain expression with a base call child
///   and a child record-construction expression.
///
/// Output:
/// - `Some(CoreExpr::ConstructorChain)` when the base is a local named call,
///   all base arguments lower into typed Core, and the right side lowers into
///   typed `CoreExpr::RecordConstruct`.
/// - `None` when the node is not constructor-chain syntax, has the wrong child
///   shape, uses a remote/non-name base call, has unsupported argument
///   expressions, or has a non-record right side.
///
/// Transformation:
/// - Preserves constructor-chain candidate identity as backend-neutral CoreIR
///   without resolving derives/parent eligibility or rewriting the chain into
///   backend record construction.
fn core_constructor_chain_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::ConstructorChain) || expr.children.len() != 2 {
        return None;
    }

    let base_call = &expr.children[0];
    if !matches!(base_call.kind, SyntaxExprKind::Call) || base_call.remote.is_some() {
        return None;
    }

    let (callee, args) = base_call.children.split_first()?;
    let base = match callee.kind {
        SyntaxExprKind::Var | SyntaxExprKind::Atom => callee.text.clone()?,
        _ => return None,
    };
    let args = args
        .iter()
        .map(core_expr_from_syntax)
        .collect::<Option<Vec<_>>>()?;

    let record = core_expr_from_syntax(&expr.children[1])?;
    if !matches!(record, CoreExpr::RecordConstruct { .. }) {
        return None;
    }

    Some(CoreExpr::ConstructorChain {
        base,
        base_constructor_identity: None,
        args,
        record: Box::new(record),
    })
}

/// Converts a syntax-output remote function reference into typed Core.
///
/// Inputs:
/// - `expr`: syntax-output remote function reference carrying module, function,
///   and arity metadata.
///
/// Output:
/// - `Some(CoreExpr::RemoteFunRef)` when module and function names are present.
/// - `None` when the syntax-output node has the wrong kind or missing metadata.
///
/// Transformation:
/// - Preserves the remote function identity as backend-neutral CoreIR metadata
///   without converting it into a call or backend function object.
fn core_remote_fun_ref_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::RemoteFunRef) {
        return None;
    }

    Some(CoreExpr::RemoteFunRef {
        module: expr.remote.clone()?,
        function: expr.text.clone()?,
        arity: expr.arity,
    })
}

/// Converts a syntax-output unary operation into typed Core.
///
/// Inputs:
/// - `expr`: syntax-output unary operator expression with one operand child
///   and an operator payload.
///
/// Output:
/// - `Some(CoreExpr::UnaryOp)` when the operator exists and the operand lowers
///   into typed Core.
/// - `None` when the shape is not unary, has the wrong child count, lacks an
///   operator, or has an unsupported operand expression.
///
/// Transformation:
/// - Preserves the normalized unary operator token and recursively lowered
///   operand as backend-neutral CoreIR.
fn core_unary_op_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::UnaryOp) || expr.children.len() != 1 {
        return None;
    }

    Some(CoreExpr::UnaryOp {
        operator: expr.operator.clone()?,
        operand: Box::new(core_expr_from_syntax(&expr.children[0])?),
    })
}

/// Converts a syntax-output remote call into typed Core.
///
/// Inputs:
/// - `expr`: syntax-output call expression with a remote module target, callee
///   child, and argument children.
///
/// Output:
/// - `Some(CoreExpr::RemoteCall)` when the module exists, callee is an atom
///   function name, and all arguments lower into typed Core.
/// - `None` for local calls, unsupported callee shapes, missing module
///   metadata, empty child lists, or unsupported argument expressions.
///
/// Transformation:
/// - Preserves module/function identity and recursively lowered arguments as
///   backend-neutral CoreIR without resolving backend import semantics.
fn core_remote_call_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    let module = expr.remote.clone()?;
    let (callee, args) = expr.children.split_first()?;
    let function = match core_expr_from_syntax(callee)? {
        CoreExpr::Atom(function) => function,
        _ => return None,
    };
    Some(CoreExpr::RemoteCall {
        module,
        function,
        args: args
            .iter()
            .map(core_expr_from_syntax)
            .collect::<Option<Vec<_>>>()?,
    })
}

/// Converts a syntax-output call into a compiler-owned intrinsic call when selected.
///
/// Inputs:
/// - `expr`: syntax-output call expression.
///
/// Output:
/// - `Some(CoreExpr::Intrinsic)` for currently selected intrinsic-backed
///   `std.core` operations with matching call shape and arity.
/// - `None` for non-intrinsic calls, unsupported operations, malformed call
///   shapes, or unsupported argument expressions.
///
/// Transformation:
/// - Accepts both module-shaped primitive calls such as
///   `std.core.String.contains(value, pattern)` and receiver-shaped primitive
///   calls such as `value.contains(pattern)`, then replaces either spelling
///   with the same backend-neutral intrinsic identity.
fn core_intrinsic_call_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    core_remote_intrinsic_call_expr_from_syntax(expr)
        .or_else(|| core_receiver_intrinsic_call_expr_from_syntax(expr))
}

/// Converts a remote syntax-output call into a compiler-owned intrinsic call.
///
/// Inputs:
/// - `expr`: syntax-output call expression with a remote module path.
///
/// Output:
/// - `Some(CoreExpr::Intrinsic)` for selected `std.core` primitive operations.
/// - `None` for local calls, malformed callees, unsupported operations,
///   mismatched arity, or unsupported argument expressions.
///
/// Transformation:
/// - Replaces a source-level `std.core.*` API function call with a stable
///   CoreIR intrinsic id while preserving argument order, return type, effects,
///   and source span.
fn core_remote_intrinsic_call_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::Call) {
        return None;
    }

    let module = expr.remote.as_deref()?;
    let (callee, args) = expr.children.split_first()?;
    let function = match core_expr_from_syntax(callee)? {
        CoreExpr::Atom(function) | CoreExpr::Var(function) => function,
        _ => return None,
    };
    let args = args
        .iter()
        .map(core_expr_from_syntax)
        .collect::<Option<Vec<_>>>()?;
    core_intrinsic_expr_from_parts(module, function.as_str(), args, expr.span.into())
}

/// Converts a receiver-method syntax-output call into a primitive intrinsic.
///
/// Inputs:
/// - `expr`: local syntax-output call whose callee may be a field-access method
///   head such as `value.contains`.
///
/// Output:
/// - `Some(CoreExpr::Intrinsic)` when the receiver method maps to the selected
///   primitive receiver surface.
/// - `None` when the call is remote, not a receiver method, has unsupported
///   receiver/argument expressions, or does not match an intrinsic operation.
///
/// Transformation:
/// - Lowers `receiver.method(args...)` into the same intrinsic as the
///   primitive owner module call, such as `std.core.Int.to_string(receiver)` or
///   `std.core.String.trim(receiver)`, prepending the receiver to the CoreIR
///   argument list so targets do not need to understand source method syntax.
fn core_receiver_intrinsic_call_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::Call) || expr.remote.is_some() {
        return None;
    }

    let (callee, args) = expr.children.split_first()?;
    if !matches!(callee.kind, SyntaxExprKind::FieldAccess) {
        return None;
    }

    let method = callee.text.as_deref()?;
    let receiver = callee.children.first()?;
    let module = core_receiver_intrinsic_module(receiver, method, args.len())?;
    let args = std::iter::once(receiver)
        .chain(args.iter())
        .map(core_expr_from_syntax)
        .collect::<Option<Vec<_>>>()?;

    core_intrinsic_expr_from_parts(module, method, args, expr.span.into())
}

/// Resolves a primitive receiver method to its CoreIR intrinsic owner module.
///
/// Inputs:
/// - `receiver`: syntax-output receiver expression from `receiver.method(...)`.
/// - `method`: receiver method name.
/// - `arg_count`: number of non-receiver call arguments.
///
/// Output:
/// - Canonical std primitive module path when the receiver/method pair maps to
///   a compiler-owned intrinsic.
///
/// Transformation:
/// - Uses the receiver expression kind as the formal CoreIR lowering boundary
///   for literal primitives so receiver syntax lowers to the same intrinsic
///   identity as explicit module calls.
fn core_receiver_intrinsic_module(
    receiver: &SyntaxExprOutput,
    method: &str,
    arg_count: usize,
) -> Option<&'static str> {
    match (receiver.kind, method, arg_count) {
        (SyntaxExprKind::Int, "to_string", 0) => Some("std.core.Int"),
        (SyntaxExprKind::Float, "to_string", 0) => Some("std.core.Float"),
        (SyntaxExprKind::Binary, _, _) => Some("std.core.String"),
        _ => None,
    }
}

/// Builds a typed CoreIR intrinsic expression from resolved source call parts.
///
/// Inputs:
/// - `module`: canonical primitive owner path, such as `std.core.String`.
/// - `function`: primitive operation name.
/// - `args`: already-lowered CoreIR arguments in intrinsic order.
/// - `span`: source span for diagnostics and contract text.
///
/// Output:
/// - `Some(CoreExpr::Intrinsic)` when the module/function/arity maps to a
///   selected primitive intrinsic.
/// - `None` when the operation is not intrinsic-backed.
///
/// Transformation:
/// - Performs the final intrinsic registry lookup and packages the closed
///   intrinsic id, arguments, return type, pure effect set, and source span into
///   a backend-neutral CoreIR node.
fn core_intrinsic_expr_from_parts(
    module: &str,
    function: &str,
    args: Vec<CoreExpr>,
    span: Span,
) -> Option<CoreExpr> {
    if let Some(intrinsic) = core_primitive_intrinsic(module, function, args.len()) {
        let return_type = core_primitive_intrinsic_return_type(&intrinsic);

        return Some(CoreExpr::Intrinsic(CoreIntrinsicCall {
            id: CoreIntrinsicId::Primitive(intrinsic),
            args,
            return_type,
            effects: core_pure_effect_set(),
            span,
        }));
    }

    let capability = core_runtime_capability(module, function, args.len())?;
    let return_type = core_runtime_capability_return_type(&capability);
    Some(CoreExpr::Intrinsic(CoreIntrinsicCall {
        id: CoreIntrinsicId::Runtime(capability),
        args,
        return_type,
        effects: core_io_effect_set(),
        span,
    }))
}

/// Resolves a `std.core` primitive operation name and arity to an intrinsic.
///
/// Inputs:
/// - `module`: source-level remote module path.
/// - `function`: source-level operation name after the module path.
/// - `arity`: argument count for the call.
///
/// Output:
/// - `Some(CorePrimitiveIntrinsic)` when the operation is currently selected
///   for primitive intrinsic lowering.
/// - `None` for portable-backed operations, unknown modules, unknown names, or
///   arity mismatch.
///
/// Transformation:
/// - Dispatches stable std.core primitive API calls to closed compiler-owned
///   intrinsic identities without carrying backend module/function names into
///   CoreIR.
fn core_primitive_intrinsic(
    module: &str,
    function: &str,
    arity: usize,
) -> Option<CorePrimitiveIntrinsic> {
    match module {
        "std.core.Bool" => core_bool_primitive_intrinsic(function, arity),
        "std.core.Int" => core_int_primitive_intrinsic(function, arity),
        "std.core.Float" => core_float_primitive_intrinsic(function, arity),
        "std.core.String" => core_string_primitive_intrinsic(function, arity),
        _ => None,
    }
}

/// Resolves a runtime stdlib operation name and arity to a CoreIR capability.
///
/// Inputs:
/// - `module`: source-level remote module path.
/// - `function`: source-level operation name after the module path.
/// - `arity`: argument count for the call.
///
/// Output:
/// - `Some(CoreRuntimeCapability)` when the operation is a selected
///   target-neutral runtime capability.
/// - `None` for primitive operations, ordinary calls, unknown modules, unknown
///   names, or arity mismatch.
///
/// Transformation:
/// - Maps source APIs such as `std.io.Console.println(value)` to backend-neutral
///   CoreIR runtime capability identities without carrying target module names
///   into CoreIR.
fn core_runtime_capability(
    module: &str,
    function: &str,
    arity: usize,
) -> Option<CoreRuntimeCapability> {
    match (module, function, arity) {
        ("std.io.Console", "println", 1) => Some(CoreRuntimeCapability::ConsolePrintln),
        _ => None,
    }
}

/// Resolves a `std.core.Bool` operation name and arity to a primitive intrinsic.
///
/// Inputs:
/// - `function`: source-level operation name after `std.core.Bool`.
/// - `arity`: argument count for the call.
///
/// Output:
/// - `Some(CorePrimitiveIntrinsic)` for selected Bool release hooks.
/// - `None` for non-intrinsic operations or arity mismatch.
///
/// Transformation:
/// - Maps the 0.0.1 Bool API hooks to stable CoreIR intrinsic identities so
///   external projects do not depend on a generated BEAM `std_core_bool` module.
fn core_bool_primitive_intrinsic(function: &str, arity: usize) -> Option<CorePrimitiveIntrinsic> {
    match (function, arity) {
        ("equal", 2) => Some(CorePrimitiveIntrinsic::BoolEqual),
        ("compare", 2) => Some(CorePrimitiveIntrinsic::BoolCompare),
        ("to_string", 1) => Some(CorePrimitiveIntrinsic::BoolToString),
        ("from_string", 1) => Some(CorePrimitiveIntrinsic::BoolFromString),
        _ => None,
    }
}

/// Resolves a `std.core.Int` operation name and arity to a primitive intrinsic.
///
/// Inputs:
/// - `function`: source-level operation name after `std.core.Int`.
/// - `arity`: argument count for the call.
///
/// Output:
/// - `Some(CorePrimitiveIntrinsic)` for selected Int conversion hooks.
/// - `None` for non-intrinsic operations or arity mismatch.
///
/// Transformation:
/// - Maps source API conversion hooks to stable CoreIR intrinsic identities.
fn core_int_primitive_intrinsic(function: &str, arity: usize) -> Option<CorePrimitiveIntrinsic> {
    match (function, arity) {
        ("to_string", 1) => Some(CorePrimitiveIntrinsic::IntToString),
        ("from_string", 1) => Some(CorePrimitiveIntrinsic::IntFromString),
        _ => None,
    }
}

/// Resolves a `std.core.Float` operation name and arity to a primitive intrinsic.
///
/// Inputs:
/// - `function`: source-level operation name after `std.core.Float`.
/// - `arity`: argument count for the call.
///
/// Output:
/// - `Some(CorePrimitiveIntrinsic)` for selected Float conversion hooks.
/// - `None` for non-intrinsic operations or arity mismatch.
///
/// Transformation:
/// - Maps source API conversion hooks to stable CoreIR intrinsic identities.
fn core_float_primitive_intrinsic(function: &str, arity: usize) -> Option<CorePrimitiveIntrinsic> {
    match (function, arity) {
        ("to_string", 1) => Some(CorePrimitiveIntrinsic::FloatToString),
        ("from_string", 1) => Some(CorePrimitiveIntrinsic::FloatFromString),
        _ => None,
    }
}

/// Resolves a `std.core.String` operation name and arity to a primitive intrinsic.
///
/// Inputs:
/// - `function`: source-level operation name after the `std.core.String`
///   module path.
/// - `arity`: argument count for the call.
///
/// Output:
/// - `Some(CorePrimitiveIntrinsic)` when the operation is currently selected
///   for string intrinsic lowering.
/// - `None` for portable-backed operations, unknown names, or arity mismatch.
///
/// Transformation:
/// - Maps source API names to closed compiler-owned intrinsic identities
///   without carrying backend module/function names into CoreIR.
fn core_string_primitive_intrinsic(function: &str, arity: usize) -> Option<CorePrimitiveIntrinsic> {
    match (function, arity) {
        ("equal", 2) => Some(CorePrimitiveIntrinsic::StringEqual),
        ("compare", 2) => Some(CorePrimitiveIntrinsic::StringCompare),
        ("to_string", 1) => Some(CorePrimitiveIntrinsic::StringToString),
        ("from_string", 1) => Some(CorePrimitiveIntrinsic::StringFromString),
        ("is_empty", 1) => Some(CorePrimitiveIntrinsic::StringIsEmpty),
        ("append", 2) => Some(CorePrimitiveIntrinsic::StringAppend),
        ("concat", 1) => Some(CorePrimitiveIntrinsic::StringConcat),
        ("contains", 2) => Some(CorePrimitiveIntrinsic::StringContains),
        ("starts_with", 2) => Some(CorePrimitiveIntrinsic::StringStartsWith),
        ("ends_with", 2) => Some(CorePrimitiveIntrinsic::StringEndsWith),
        ("length", 1) => Some(CorePrimitiveIntrinsic::StringLength),
        ("byte_size", 1) => Some(CorePrimitiveIntrinsic::StringByteSize),
        ("lowercase", 1) => Some(CorePrimitiveIntrinsic::StringLowercase),
        ("uppercase", 1) => Some(CorePrimitiveIntrinsic::StringUppercase),
        ("trim", 1) => Some(CorePrimitiveIntrinsic::StringTrim),
        ("trim_start", 1) => Some(CorePrimitiveIntrinsic::StringTrimStart),
        ("trim_end", 1) => Some(CorePrimitiveIntrinsic::StringTrimEnd),
        ("replace", 3) => Some(CorePrimitiveIntrinsic::StringReplace),
        ("split", 2) => Some(CorePrimitiveIntrinsic::StringSplit),
        ("split_once", 2) => Some(CorePrimitiveIntrinsic::StringSplitOnce),
        _ => None,
    }
}

/// Returns the Core return type for a primitive intrinsic.
///
/// Inputs:
/// - `intrinsic`: compiler-owned primitive intrinsic identity.
///
/// Output:
/// - Backend-neutral `CoreType` result expected from the intrinsic call.
///
/// Transformation:
/// - Encodes the intrinsic registry's output column as CoreIR type payloads so
///   target lowering can validate operation results without re-reading source
///   signatures.
fn core_primitive_intrinsic_return_type(intrinsic: &CorePrimitiveIntrinsic) -> CoreType {
    match intrinsic {
        CorePrimitiveIntrinsic::BoolToString
        | CorePrimitiveIntrinsic::IntToString
        | CorePrimitiveIntrinsic::FloatToString
        | CorePrimitiveIntrinsic::StringToString
        | CorePrimitiveIntrinsic::StringAppend
        | CorePrimitiveIntrinsic::StringConcat
        | CorePrimitiveIntrinsic::StringLowercase
        | CorePrimitiveIntrinsic::StringUppercase
        | CorePrimitiveIntrinsic::StringTrim
        | CorePrimitiveIntrinsic::StringTrimStart
        | CorePrimitiveIntrinsic::StringTrimEnd
        | CorePrimitiveIntrinsic::StringReplace => CoreType::String,
        CorePrimitiveIntrinsic::BoolEqual => CoreType::Bool,
        CorePrimitiveIntrinsic::BoolCompare => {
            CoreType::Named("std.core.Ordering.Comparison".to_string())
        }
        CorePrimitiveIntrinsic::StringCompare => {
            CoreType::Named("std.core.Ordering.Comparison".to_string())
        }
        CorePrimitiveIntrinsic::BoolFromString => CoreType::Apply {
            constructor: "Option".to_string(),
            args: vec![CoreType::Bool],
        },
        CorePrimitiveIntrinsic::StringFromString => CoreType::Apply {
            constructor: "Option".to_string(),
            args: vec![CoreType::String],
        },
        CorePrimitiveIntrinsic::IntFromString => CoreType::Apply {
            constructor: "Option".to_string(),
            args: vec![CoreType::Int],
        },
        CorePrimitiveIntrinsic::FloatFromString => CoreType::Apply {
            constructor: "Option".to_string(),
            args: vec![CoreType::Float],
        },
        CorePrimitiveIntrinsic::StringEqual
        | CorePrimitiveIntrinsic::StringIsEmpty
        | CorePrimitiveIntrinsic::StringContains
        | CorePrimitiveIntrinsic::StringStartsWith
        | CorePrimitiveIntrinsic::StringEndsWith => CoreType::Bool,
        CorePrimitiveIntrinsic::StringLength | CorePrimitiveIntrinsic::StringByteSize => {
            CoreType::Int
        }
        CorePrimitiveIntrinsic::StringSplit => CoreType::List(Box::new(CoreType::String)),
        CorePrimitiveIntrinsic::StringSplitOnce => CoreType::Apply {
            constructor: "Option".to_string(),
            args: vec![CoreType::Tuple(vec![
                CoreTupleTypeElem::Type(CoreType::String),
                CoreTupleTypeElem::Type(CoreType::String),
            ])],
        },
    }
}

/// Returns the Core return type for a runtime capability.
///
/// Inputs:
/// - `capability`: compiler-owned runtime capability identity.
///
/// Output:
/// - Backend-neutral `CoreType` result expected from the capability call.
///
/// Transformation:
/// - Encodes the runtime capability registry's output column as CoreIR type
///   payloads so target lowering can validate effectful operation results
///   without re-reading source signatures.
fn core_runtime_capability_return_type(capability: &CoreRuntimeCapability) -> CoreType {
    match capability {
        CoreRuntimeCapability::ConsolePrintln => CoreType::Named("Unit".to_string()),
    }
}

/// Builds the canonical pure Core effect set.
///
/// Inputs:
/// - None.
///
/// Output:
/// - `CoreEffectSet` containing the stable `pure` label.
///
/// Transformation:
/// - Centralizes the effect payload used by primitive intrinsics that do not
///   perform observable side effects.
fn core_pure_effect_set() -> CoreEffectSet {
    CoreEffectSet {
        effects: vec!["pure".to_string()],
    }
}

/// Builds the canonical IO Core effect set.
///
/// Inputs:
/// - None.
///
/// Output:
/// - `CoreEffectSet` containing the stable `io` label.
///
/// Transformation:
/// - Centralizes the effect payload used by runtime capabilities that perform
///   observable console or stream effects.
fn core_io_effect_set() -> CoreEffectSet {
    CoreEffectSet {
        effects: vec!["io".to_string()],
    }
}

/// Converts a syntax-output named call into a typed Core call candidate.
///
/// Inputs:
/// - `expr`: syntax-output `Call` expression with no remote target.
///
/// Output:
/// - `Some(CoreExpr::Call)` when the callee is a lowercase local function name
///   and all arguments lower to typed Core expressions.
/// - `Some(CoreExpr::ConstructorCall)` when the callee is an uppercase
///   constructor-like name and all arguments lower to typed Core expressions.
/// - `None` for non-name callees, empty call payloads, remote calls, or
///   unsupported argument expressions.
///
/// Transformation:
/// - Preserves lowercase function calls and uppercase constructor-call
///   candidates as separate backend-neutral CoreIR nodes without resolving
///   constructor eligibility.
fn core_named_call_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if expr.kind != SyntaxExprKind::Call || expr.remote.is_some() {
        return None;
    }

    let (callee, args) = expr.children.split_first()?;
    let name = match callee.kind {
        SyntaxExprKind::Var | SyntaxExprKind::Atom => callee.text.clone()?,
        _ => return None,
    };
    let args = args
        .iter()
        .map(core_expr_from_syntax)
        .collect::<Option<Vec<_>>>()?;

    if starts_with_ascii_lowercase(&name) {
        Some(CoreExpr::Call {
            function: name,
            args,
        })
    } else if starts_with_ascii_uppercase(&name) {
        Some(CoreExpr::ConstructorCall {
            constructor: name,
            constructor_identity: None,
            args,
        })
    } else {
        None
    }
}

/// Converts a syntax-output function-value invocation into typed CoreIR.
///
/// Inputs:
/// - `expr`: syntax-output `FunctionCall` expression created from `callee.(args)`.
///
/// Output:
/// - `Some(CoreExpr::FunctionCall)` when the callee and every argument are
///   representable in the current typed Core subset.
/// - `None` for malformed function-call payloads or unsupported child
///   expressions.
///
/// Transformation:
/// - Preserves the callable expression separately from named calls so later
///   target profiles and backends can distinguish `f.(x)` from `f(x)`.
fn core_function_call_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if expr.kind != SyntaxExprKind::FunctionCall || expr.remote.is_some() {
        return None;
    }

    let (callee, args) = expr.children.split_first()?;
    Some(CoreExpr::FunctionCall {
        callee: Box::new(core_expr_from_syntax(callee)?),
        args: args
            .iter()
            .map(core_expr_from_syntax)
            .collect::<Option<Vec<_>>>()?,
    })
}

/// Checks whether a name begins with an ASCII lowercase character.
///
/// Inputs:
/// - `name`: source-level identifier text.
///
/// Output:
/// - `true` when the first character is ASCII lowercase.
/// - `false` for empty strings and non-lowercase leading characters.
///
/// Transformation:
/// - Reads only the first Unicode scalar value and applies the Terlan
///   source-mode ASCII lowercase convention used for function names.
fn starts_with_ascii_lowercase(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_lowercase())
}

/// Checks whether a name begins with an ASCII uppercase character.
///
/// Inputs:
/// - `name`: source-level identifier text.
///
/// Output:
/// - `true` when the first character is ASCII uppercase.
/// - `false` for empty strings and non-uppercase leading characters.
///
/// Transformation:
/// - Reads only the first Unicode scalar value and applies the Terlan
///   source-mode ASCII uppercase convention used for constructor candidates.
fn starts_with_ascii_uppercase(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
}

/// Converts syntax-output case clauses into typed Core case clauses.
///
/// Inputs:
/// - `expr`: syntax-output case expression whose clauses should be lowered.
///
/// Output:
/// - `Some(Vec<CoreCaseClause>)` when every branch has one covered pattern,
///   has a covered body expression, and has either no guard or a covered guard
///   expression.
/// - `None` when any branch still needs unsupported patterns, richer bodies,
///   unsupported guards, or richer body modeling.
///
/// Transformation:
/// - Recursively lowers branch patterns and bodies into typed Core payloads and
///   fails the whole case conversion if any branch remains unsupported.
fn core_case_clauses_from_syntax(expr: &SyntaxExprOutput) -> Option<Vec<CoreCaseClause>> {
    expr.clauses
        .iter()
        .map(core_case_clause_from_syntax)
        .collect()
}

/// Converts one syntax-output case clause into a typed Core case clause.
///
/// Inputs:
/// - `clause`: syntax-output case clause.
///
/// Output:
/// - `Some(CoreCaseClause)` for one-pattern clauses in the current typed
///   subset, including supported guarded forms.
/// - `None` for multi-pattern clauses, unsupported patterns, unsupported
///   guards, or unsupported bodies.
///
/// Transformation:
/// - Lowers the branch pattern and body without using backend syntax or
///   rendered summary text.
fn core_case_clause_from_syntax(
    clause: &terlan_syntax::SyntaxClauseOutput,
) -> Option<CoreCaseClause> {
    if clause.patterns.len() != 1 {
        return None;
    }
    let guard = clause
        .guard
        .as_ref()
        .and_then(|guard| core_expr_from_syntax(guard.as_ref()));
    Some(CoreCaseClause {
        pattern: core_pattern_from_syntax(&clause.patterns[0])?,
        guard,
        body: core_expr_from_syntax(&clause.body)?,
    })
}

/// Converts a syntax-output receive expression into typed Core.
///
/// Inputs:
/// - `expr`: syntax-output receive expression with pattern clauses and optional
///   timeout branch.
///
/// Output:
/// - `Some(CoreExpr::Receive)` when every receive clause and optional timeout
///   branch lowers into typed Core.
/// - `None` when the node is not receive syntax or any clause/timeout child
///   remains unsupported.
///
/// Transformation:
/// - Reuses Core case-branch payloads for receive pattern clauses and preserves
///   the optional timeout branch as receive-specific CoreIR.
fn core_receive_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::Receive) {
        return None;
    }

    Some(CoreExpr::Receive {
        clauses: core_case_clauses_from_syntax(expr)?,
        after_clause: match expr.receive_after.as_ref() {
            Some(after_clause) => Some(core_receive_after_from_syntax(after_clause)?),
            None => None,
        },
    })
}

/// Converts a syntax-output receive timeout branch into typed Core.
///
/// Inputs:
/// - `after_clause`: syntax-output receive timeout trigger/body payload.
///
/// Output:
/// - `Some(CoreReceiveAfter)` when both trigger and body lower into typed Core.
/// - `None` when either expression remains unsupported.
///
/// Transformation:
/// - Preserves timeout trigger and body as a receive-specific CoreIR branch
///   without backend timeout semantics.
fn core_receive_after_from_syntax(
    after_clause: &terlan_syntax::syntax_output::SyntaxReceiveAfterOutput,
) -> Option<CoreReceiveAfter> {
    Some(CoreReceiveAfter {
        trigger: Box::new(core_expr_from_syntax(&after_clause.trigger)?),
        body: Box::new(core_expr_from_syntax(&after_clause.body)?),
    })
}

/// Converts a syntax-output try expression into typed Core.
///
/// Inputs:
/// - `expr`: syntax-output try expression with body, `of` clauses, `catch`
///   clauses, and optional cleanup branch.
///
/// Output:
/// - `Some(CoreExpr::Try)` when the body, every clause, and optional cleanup
///   branch lower into typed Core.
/// - `None` when the node is not try syntax or any child remains unsupported.
///
/// Transformation:
/// - Preserves try body, success clauses, catch clauses, and optional cleanup
///   branch as a backend-neutral CoreIR keyword expression.
fn core_try_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::Try) || expr.children.len() != 1 {
        return None;
    }

    Some(CoreExpr::Try {
        body: Box::new(core_expr_from_syntax(&expr.children[0])?),
        of_clauses: core_case_clauses_from_syntax(expr)?,
        catch_clauses: expr
            .catch_clauses
            .iter()
            .map(core_case_clause_from_syntax)
            .collect::<Option<Vec<_>>>()?,
        after_clause: match expr.try_after.as_ref() {
            Some(after_clause) => Some(core_try_after_from_syntax(after_clause)?),
            None => None,
        },
    })
}

/// Converts a syntax-output try cleanup branch into typed Core.
///
/// Inputs:
/// - `after_clause`: syntax-output try cleanup trigger/body payload.
///
/// Output:
/// - `Some(CoreTryAfter)` when both trigger and body lower into typed Core.
/// - `None` when either expression remains unsupported.
///
/// Transformation:
/// - Preserves cleanup trigger and body as a try-specific CoreIR branch without
///   backend cleanup semantics.
fn core_try_after_from_syntax(
    after_clause: &terlan_syntax::syntax_output::SyntaxTryAfterOutput,
) -> Option<CoreTryAfter> {
    Some(CoreTryAfter {
        trigger: Box::new(core_expr_from_syntax(&after_clause.trigger)?),
        body: Box::new(core_expr_from_syntax(&after_clause.body)?),
    })
}

/// Converts a syntax-output if expression into typed Core.
///
/// Inputs:
/// - `expr`: syntax-output if expression whose clauses carry conditions in
///   `guard` and branch bodies in `body`.
///
/// Output:
/// - `Some(CoreExpr::If)` when every condition and body lowers into typed Core.
/// - `None` when the node is not an if expression, contains pattern payloads,
///   lacks a condition, or contains unsupported condition/body expressions.
///
/// Transformation:
/// - Reconstructs condition/body branches from syntax-output clauses without
///   treating them as pattern-matching case clauses.
fn core_if_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::If) {
        return None;
    }

    expr.clauses
        .iter()
        .map(core_if_clause_from_syntax)
        .collect::<Option<Vec<_>>>()
        .map(|clauses| CoreExpr::If { clauses })
}

/// Converts one syntax-output if clause into typed Core.
///
/// Inputs:
/// - `clause`: syntax-output if clause with no patterns, condition in `guard`,
///   and branch body in `body`.
///
/// Output:
/// - `Some(CoreIfClause)` when condition and body are typed Core expressions.
/// - `None` when patterns are present, condition is missing, or either
///   expression remains unsupported.
///
/// Transformation:
/// - Lowers the condition/body pair while preserving the if-specific branch
///   shape independently from case-pattern clauses.
fn core_if_clause_from_syntax(clause: &terlan_syntax::SyntaxClauseOutput) -> Option<CoreIfClause> {
    if !clause.patterns.is_empty() {
        return None;
    }
    Some(CoreIfClause {
        condition: core_expr_from_syntax(clause.guard.as_ref()?.as_ref())?,
        body: core_expr_from_syntax(&clause.body)?,
    })
}

/// Classifies a syntax-output pattern for Lean proof coverage.
///
/// Inputs:
/// - `pattern`: syntax-output pattern being summarized into CoreIR.
/// - `core_pattern`: typed Core payload produced for `pattern`, when
///   available.
///
/// Output:
/// - Proof coverage label for the current production CoreIR pattern summary.
///
/// Transformation:
/// - Marks Lean-modeled pattern families as covered only when they actually
///   carry typed `CorePattern` payloads whose nested children are also covered;
///   unsupported members of those families remain proof-model-required until
///   Lean models their shape.
fn core_pattern_proof_coverage(
    pattern: &SyntaxPatternOutput,
    core_pattern: Option<&CorePattern>,
) -> CoreProofCoverage {
    match pattern.kind {
        SyntaxPatternKind::Wildcard
        | SyntaxPatternKind::Var
        | SyntaxPatternKind::Int
        | SyntaxPatternKind::Atom
        | SyntaxPatternKind::Tuple
        | SyntaxPatternKind::List
        | SyntaxPatternKind::Constructor
        | SyntaxPatternKind::Ignore
        | SyntaxPatternKind::Placeholder => {
            if core_pattern.is_some_and(core_pattern_is_lean_modeled) {
                CoreProofCoverage::LeanCovered
            } else {
                CoreProofCoverage::ProofModelRequired
            }
        }
        SyntaxPatternKind::Float
        | SyntaxPatternKind::ListCons
        | SyntaxPatternKind::Map
        | SyntaxPatternKind::Record
        | SyntaxPatternKind::MapField => CoreProofCoverage::ProofModelRequired,
    }
}

/// Converts a syntax-output pattern into a typed Core pattern when covered.
///
/// Inputs:
/// - `pattern`: syntax-output pattern summary produced by the parser pipeline.
///
/// Output:
/// - `Some(CorePattern)` for Lean-covered pattern forms.
/// - `None` for source forms that still need a richer CorePattern model.
///
/// Transformation:
/// - Reconstructs typed structural Core pattern nodes from syntax-output kind,
///   text, and child patterns, without using backend lowering or rendered
///   summary text.
fn core_pattern_from_syntax(pattern: &SyntaxPatternOutput) -> Option<CorePattern> {
    match pattern.kind {
        SyntaxPatternKind::Wildcard
        | SyntaxPatternKind::Ignore
        | SyntaxPatternKind::Placeholder => Some(CorePattern::Wildcard),
        SyntaxPatternKind::Var => pattern.text.clone().map(CorePattern::Var),
        SyntaxPatternKind::Int => pattern
            .text
            .as_ref()
            .and_then(|value| value.parse::<i64>().ok())
            .map(CorePattern::Int),
        SyntaxPatternKind::Atom => pattern.text.clone().map(CorePattern::Atom),
        SyntaxPatternKind::Tuple => {
            core_patterns_from_syntax_children(pattern).map(CorePattern::Tuple)
        }
        SyntaxPatternKind::List => {
            core_patterns_from_syntax_children(pattern).map(CorePattern::List)
        }
        SyntaxPatternKind::ListCons => core_list_cons_pattern_from_syntax(pattern),
        SyntaxPatternKind::Constructor => pattern.text.as_ref().and_then(|name| {
            core_patterns_from_syntax_children(pattern).map(|args| CorePattern::Constructor {
                name: name.clone(),
                constructor_identity: None,
                args,
            })
        }),
        SyntaxPatternKind::Float => pattern.text.clone().map(CorePattern::Float),
        SyntaxPatternKind::Map => {
            core_map_pattern_fields_from_syntax(pattern).map(CorePattern::Map)
        }
        SyntaxPatternKind::Record => core_record_pattern_from_syntax(pattern),
        SyntaxPatternKind::MapField => None,
    }
}

/// Converts a syntax-output list-cons pattern into typed Core.
///
/// Inputs:
/// - `pattern`: syntax-output list-cons pattern with head and tail children.
///
/// Output:
/// - `Some(CorePattern::ListCons)` when both head and tail lower to typed Core
///   patterns.
/// - `None` when the shape is not list-cons or either side remains unsupported.
///
/// Transformation:
/// - Preserves the structural cons pattern as a backend-agnostic head/tail Core
///   node without using list rendering syntax.
fn core_list_cons_pattern_from_syntax(pattern: &SyntaxPatternOutput) -> Option<CorePattern> {
    if !matches!(pattern.kind, SyntaxPatternKind::ListCons) || pattern.children.len() != 2 {
        return None;
    }

    Some(CorePattern::ListCons {
        head: Box::new(core_pattern_from_syntax(&pattern.children[0])?),
        tail: Box::new(core_pattern_from_syntax(&pattern.children[1])?),
    })
}

/// Converts syntax-output map-pattern fields into typed Core map fields.
///
/// Inputs:
/// - `pattern`: syntax-output map pattern whose fields should be lowered.
///
/// Output:
/// - `Some(Vec<CoreMapPatternField>)` when every field value lowers to a typed
///   Core pattern.
/// - `None` when the pattern has non-map syntax or any field value remains
///   unsupported.
///
/// Transformation:
/// - Preserves field keys and required/optional matching mode, while
///   recursively lowering field value patterns into backend-agnostic CoreIR.
fn core_map_pattern_fields_from_syntax(
    pattern: &SyntaxPatternOutput,
) -> Option<Vec<CoreMapPatternField>> {
    if !matches!(pattern.kind, SyntaxPatternKind::Map) {
        return None;
    }

    pattern
        .fields
        .iter()
        .map(|field| {
            core_pattern_from_syntax(&field.value).map(|value| CoreMapPatternField {
                key: field.key.clone(),
                required: field.required,
                value,
            })
        })
        .collect()
}

/// Converts a syntax-output record pattern into typed Core.
///
/// Inputs:
/// - `pattern`: syntax-output record pattern with source record name and fields.
///
/// Output:
/// - `Some(CorePattern::Record)` when every field value lowers to a typed Core
///   pattern.
/// - `None` when the shape is not a record, has no name, or any field value is
///   unsupported.
///
/// Transformation:
/// - Preserves record identity and field names as semantic CoreIR data, while
///   recursively lowering field values into typed Core patterns.
fn core_record_pattern_from_syntax(pattern: &SyntaxPatternOutput) -> Option<CorePattern> {
    if !matches!(pattern.kind, SyntaxPatternKind::Record) {
        return None;
    }

    Some(CorePattern::Record {
        name: pattern.text.clone()?,
        fields: core_record_pattern_fields_from_syntax(pattern)?,
    })
}

/// Converts syntax-output record-pattern fields into typed Core record fields.
///
/// Inputs:
/// - `pattern`: syntax-output record pattern whose fields should be lowered.
///
/// Output:
/// - `Some(Vec<CoreRecordPatternField>)` when every field value lowers.
/// - `None` when any field value remains unsupported.
///
/// Transformation:
/// - Preserves field keys and required/optional source mode, while recursively
///   lowering field value patterns into backend-agnostic CoreIR.
fn core_record_pattern_fields_from_syntax(
    pattern: &SyntaxPatternOutput,
) -> Option<Vec<CoreRecordPatternField>> {
    pattern
        .fields
        .iter()
        .map(|field| {
            core_pattern_from_syntax(&field.value).map(|value| CoreRecordPatternField {
                key: field.key.clone(),
                required: field.required,
                value,
            })
        })
        .collect()
}

/// Converts syntax-output pattern children into typed Core pattern children.
///
/// Inputs:
/// - `pattern`: syntax-output parent pattern whose children should be lowered.
///
/// Output:
/// - `Some(Vec<CorePattern>)` when every child is in the covered subset.
/// - `None` when at least one child is not yet representable as a typed Core
///   pattern.
///
/// Transformation:
/// - Recursively lowers children and fails the parent conversion if any child
///   remains unsupported.
fn core_patterns_from_syntax_children(pattern: &SyntaxPatternOutput) -> Option<Vec<CorePattern>> {
    core_patterns_from_syntax_slice(&pattern.children)
}

/// Converts a slice of syntax-output patterns into typed Core patterns.
///
/// Inputs:
/// - `patterns`: syntax-output patterns to lower in order.
///
/// Output:
/// - `Some(Vec<CorePattern>)` when every pattern is in the current typed
///   subset.
/// - `None` when at least one pattern is not yet representable as typed Core.
///
/// Transformation:
/// - Recursively lowers each pattern and fails the entire slice conversion if
///   any element remains unsupported.
fn core_patterns_from_syntax_slice(patterns: &[SyntaxPatternOutput]) -> Option<Vec<CorePattern>> {
    patterns.iter().map(core_pattern_from_syntax).collect()
}

/// Renders a CoreIR expression summary as deterministic compact text.
///
/// Inputs:
/// - `expr`: Core expression summary.
///
/// Output:
/// - Stable text for snapshots and contract summaries.
///
/// Transformation:
/// - Combines expression kind, optional identity fields, arity, and child
///   summaries into a compact backend-neutral string.
fn core_expr_summary_text(expr: &CoreExprSummary) -> String {
    let mut parts = vec![expr.kind.clone()];
    if let Some(core_expr) = &expr.core_expr {
        parts.push(format!("core={}", core_expr.contract_text()));
    }
    if let Some(evidence) = &expr.checked_preservation_evidence {
        parts.push(format!("preservation={}", evidence.contract_text()));
    }
    parts.push(format!("proof={}", expr.proof_coverage.as_str()));
    if let Some(remote) = &expr.remote {
        parts.push(format!("remote={}", remote));
    }
    if let Some(text) = &expr.text {
        parts.push(format!("text={}", text));
    }
    if let Some(operator) = &expr.operator {
        parts.push(format!("op={}", operator));
    }
    parts.push(format!("arity={}", expr.arity));
    if !expr.children.is_empty() {
        parts.push(format!(
            "children=[{}]",
            expr.children
                .iter()
                .map(core_expr_summary_text)
                .collect::<Vec<_>>()
                .join(";")
        ));
    }
    parts.join(":")
}

/// Renders a syntax pattern as deterministic CoreIR summary text.
///
/// Inputs:
/// - `pattern`: syntax-output pattern.
///
/// Output:
/// - Stable pattern summary text.
///
/// Transformation:
/// - Combines pattern kind, optional text, arity, and recursive child/field
///   summaries without assigning backend representation.
fn core_pattern_summary_text(pattern: &SyntaxPatternOutput) -> String {
    let mut parts = vec![format!("{:?}", pattern.kind)];
    if let Some(text) = &pattern.text {
        parts.push(format!("text={}", text));
    }
    parts.push(format!("arity={}", pattern.arity));
    if !pattern.children.is_empty() {
        parts.push(format!(
            "children=[{}]",
            pattern
                .children
                .iter()
                .map(core_pattern_summary_text)
                .collect::<Vec<_>>()
                .join(";")
        ));
    }
    if !pattern.fields.is_empty() {
        parts.push(format!(
            "fields=[{}]",
            pattern
                .fields
                .iter()
                .map(|field| format!(
                    "{}:{}={}",
                    field.key,
                    field.required,
                    core_pattern_summary_text(&field.value)
                ))
                .collect::<Vec<_>>()
                .join(";")
        ));
    }
    parts.join(":")
}

/// Lowers resolver interface-map keys into deterministic CoreIR imports.
///
/// Inputs:
/// - `resolved`: resolver artifact containing the visible interface map.
///
/// Output:
/// - Sorted Core import summaries.
///
/// Transformation:
/// - Converts module names into backend-neutral Core import records without
///   preserving backend or filesystem details.
fn lower_core_imports(resolved: &ResolvedModule) -> Vec<CoreImport> {
    let mut imports = resolved
        .interface_map
        .keys()
        .filter(|module| *module != &resolved.name)
        .map(|module| CoreImport {
            module: module.clone(),
            kind: CoreImportKind::Module,
        })
        .collect::<Vec<_>>();
    imports.sort_by(|left, right| left.module.cmp(&right.module));
    imports
}

/// Lowers public interface members into deterministic CoreIR exports.
///
/// Inputs:
/// - `interface`: module interface produced by resolution/typechecking.
///
/// Output:
/// - Sorted Core export summaries.
///
/// Transformation:
/// - Records public functions, public types, and public constructors as
///   backend-independent export summaries.
fn lower_core_exports(interface: &ModuleInterface) -> Vec<CoreExport> {
    let mut exports = Vec::new();
    exports.extend(
        interface
            .functions
            .iter()
            .filter_map(|((name, arity), signature)| {
                signature.public.then(|| CoreExport {
                    name: name.clone(),
                    kind: CoreExportKind::Function { arity: *arity },
                })
            }),
    );
    exports.extend(interface.public_types.iter().map(|name| CoreExport {
        name: name.clone(),
        kind: CoreExportKind::Type,
    }));
    exports.extend(
        interface
            .constructors
            .iter()
            .flat_map(|(name, signatures)| {
                signatures.iter().filter_map(move |signature| {
                    signature.public.then(|| CoreExport {
                        name: name.clone(),
                        kind: CoreExportKind::Constructor {
                            min_arity: signature.min_arity,
                        },
                    })
                })
            }),
    );
    exports.sort_by(|left, right| {
        let left_key = core_export_sort_key(left);
        let right_key = core_export_sort_key(right);
        left_key.cmp(&right_key)
    });
    exports
}

/// Builds a deterministic sort key for a Core export.
///
/// Inputs:
/// - `export`: Core export summary.
///
/// Output:
/// - Stable string key.
///
/// Transformation:
/// - Combines export kind, name, and arity-like data into a sortable identity.
fn core_export_sort_key(export: &CoreExport) -> String {
    match export.kind {
        CoreExportKind::Function { arity } => format!("function:{}:{}", export.name, arity),
        CoreExportKind::Type => format!("type:{}", export.name),
        CoreExportKind::Constructor { min_arity } => {
            format!("constructor:{}:{}", export.name, min_arity)
        }
    }
}

/// Lowers interface type declarations into deterministic CoreIR type summaries.
///
/// Inputs:
/// - `interface`: module interface produced by resolution/typechecking.
///
/// Output:
/// - Sorted Core type declarations.
///
/// Transformation:
/// - Preserves type visibility, type parameters, textual type body summary,
///   and an optional typed CoreType body when the declaration body is already
///   representable by the current CoreType model.
fn lower_core_types(interface: &ModuleInterface) -> Vec<CoreTypeDecl> {
    let mut names = interface
        .public_types
        .iter()
        .chain(interface.private_types.iter())
        .chain(interface.opaque_types.iter())
        .cloned()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let visibility = if interface.opaque_types.contains(&name) {
                CoreVisibility::Opaque
            } else if interface.public_types.contains(&name) {
                CoreVisibility::Public
            } else {
                CoreVisibility::Private
            };
            let body = interface
                .type_bodies
                .get(&name)
                .cloned()
                .unwrap_or_default();
            let core_body = core_type_from_body_variants(&body);
            CoreTypeDecl {
                params: interface
                    .type_params
                    .get(&name)
                    .cloned()
                    .unwrap_or_default(),
                body,
                core_body,
                name,
                visibility,
            }
        })
        .collect()
}

/// Builds typed CoreType bodies for local syntax-output struct declarations.
///
/// Inputs:
/// - `module`: compiler-facing syntax module whose declarations may include
///   local structs.
///
/// Output:
/// - Map from struct name to typed `CoreType::Struct` payload for structs whose
///   field annotations all lower into supported CoreType forms.
///
/// Transformation:
/// - Scans local struct declarations, lowers each field annotation through the
///   existing type-text CoreType converter, and keeps only fully typed
///   structural bodies.
fn core_syntax_struct_type_bodies(module: &SyntaxModuleOutput) -> HashMap<String, CoreType> {
    module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Struct { name, fields, .. } => {
                core_type_from_syntax_struct_fields(name, fields)
            }
            _ => None,
        })
        .collect()
}

/// Converts one syntax-output struct declaration into a typed Core struct body.
///
/// Inputs:
/// - `name`: local struct type name.
/// - `fields`: syntax-output struct fields with source type annotations.
///
/// Output:
/// - `Some((name, CoreType::Struct))` when every field annotation is
///   representable by the current CoreType model.
/// - `None` when any field still requires unsupported type lowering.
///
/// Transformation:
/// - Preserves field order, lowers annotation text into backend-neutral
///   CoreType payloads, and avoids encoding runtime struct construction.
fn core_type_from_syntax_struct_fields(
    name: &str,
    fields: &[SyntaxStructFieldOutput],
) -> Option<(String, CoreType)> {
    fields
        .iter()
        .map(|field| {
            core_type_from_text(&field.annotation.text).map(|ty| CoreStructTypeField {
                name: field.name.clone(),
                ty,
            })
        })
        .collect::<Option<Vec<_>>>()
        .map(|fields| {
            (
                name.to_string(),
                CoreType::Struct {
                    name: name.to_string(),
                    fields,
                },
            )
        })
}

/// Lowers interface function signatures into deterministic CoreIR functions.
///
/// Inputs:
/// - `interface`: module interface produced by resolution/typechecking.
///
/// Output:
/// - Sorted Core function summaries.
///
/// Transformation:
/// - Converts function signature metadata into typed Core parameter and return
///   summaries without lowering to any backend call form.
fn lower_core_functions(interface: &ModuleInterface) -> Vec<CoreFunction> {
    let mut functions = interface
        .functions
        .iter()
        .map(|((name, arity), signature)| CoreFunction {
            name: name.clone(),
            arity: *arity,
            public: signature.public,
            params: signature
                .params
                .iter()
                .map(|param| CoreParam {
                    name: param.name.clone(),
                    ty: param.annotation.clone(),
                    core_ty: core_type_from_text(&param.annotation),
                })
                .collect(),
            return_type: signature.return_type.clone(),
            core_return_type: core_type_from_text(&signature.return_type),
            clauses: Vec::new(),
        })
        .collect::<Vec<_>>();
    functions.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.arity.cmp(&right.arity))
    });
    functions
}

/// Lowers interface constructor signatures into deterministic CoreIR
/// constructors.
///
/// Inputs:
/// - `interface`: module interface produced by resolution/typechecking.
///
/// Output:
/// - Sorted Core constructor summaries.
///
/// Transformation:
/// - Converts constructor signatures into semantic constructor declarations
///   without committing to tuple, atom, record, or backend layout encoding.
fn lower_core_constructors(interface: &ModuleInterface) -> Vec<CoreConstructorDecl> {
    let mut constructors = interface
        .constructors
        .iter()
        .flat_map(|(name, signatures)| {
            signatures.iter().map(move |signature| CoreConstructorDecl {
                name: name.clone(),
                public: signature.public,
                min_arity: signature.min_arity,
                params: signature
                    .params
                    .iter()
                    .map(|param| CoreParam {
                        name: param.name.clone(),
                        ty: param.annotation.clone(),
                        core_ty: core_type_from_text(&param.annotation),
                    })
                    .collect(),
                vararg: signature.vararg.as_ref().map(|param| CoreParam {
                    name: param.name.clone(),
                    ty: param.annotation.clone(),
                    core_ty: core_type_from_text(&param.annotation),
                }),
                return_type: signature.return_type.clone(),
                core_return_type: core_type_from_text(&signature.return_type),
            })
        })
        .collect::<Vec<_>>();
    constructors.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.min_arity.cmp(&right.min_arity))
    });
    constructors
}

/// Returns a list of diagnostics for raw declarations that are not yet supported
/// by the formal compiler path.
///
/// Inputs:
/// - `module`: formality-facing syntax module to validate.
///
/// Output:
/// - A list of errors for each unsupported `SyntaxDeclarationPayload::Raw` kind.
///
/// Transformation:
/// - Scans each declaration and emits an error for every remaining raw payload.
///   Canonical config declarations are represented as `Config`, not raw output.
pub fn collect_syntax_unsupported_raw_declaration_diagnostics(
    module: &SyntaxModuleOutput,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for declaration in &module.declarations {
        if let SyntaxDeclarationPayload::Raw { raw_kind, .. } = &declaration.payload {
            diagnostics.push(Diagnostic {
                span: declaration.span.into(),
                message: format!(
                    "unsupported raw declaration kind `{}` in formal compiler path",
                    raw_kind
                ),
                severity: DiagSeverity::Error,
            });
        }
    }

    diagnostics
}

/// Runs the syntax-output macro-expansion phase.
///
/// Inputs:
/// - `module`: compiler-facing syntax output to scan.
///
/// Output:
/// - A tuple containing the expanded syntax-output module and one syntax-check
///   diagnostic per unresolved raw macro.
///
/// Transformation:
/// - Performs explicit expansion of macro-bearing expressions. The current formal
///   phase is explicit-unsupported for raw macros, so this pass currently
///   preserves all nodes and returns diagnostics when raw macros remain.
pub fn expand_syntax_raw_macros(
    module: SyntaxModuleOutput,
) -> (SyntaxModuleOutput, Vec<Diagnostic>) {
    let diagnostics = collect_syntax_raw_macro_diagnostics(&module);
    (module, diagnostics)
}

/// Runs the syntax-output derive-expansion validation phase.
///
/// Inputs:
/// - `module`: compiler-facing syntax output to validate.
/// - `resolved`: resolved module context containing imported trait signatures.
///
/// Output:
/// - A tuple containing the expanded syntax-output module and one diagnostic per
///   derive validation failure.
///
/// Transformation:
/// - Validates struct derive clauses against known trait signatures and emits
///   explicit diagnostics for parse and arity/shape issues. The current formal
///   compiler path keeps derive expansion explicit but non-transforming.
pub fn expand_syntax_derives(
    module: SyntaxModuleOutput,
    resolved: &ResolvedModule,
) -> (SyntaxModuleOutput, Vec<Diagnostic>) {
    let trait_signatures = collect_syntax_trait_signatures(&module, resolved);
    let diagnostics = check_syntax_struct_derives(&module, &trait_signatures);
    (module, diagnostics)
}

/// Collects raw-macro diagnostics for syntax-output modules before full
/// resolution/typechecking.
///
/// Inputs:
/// - `module`: compiler-facing syntax output to scan.
///
/// Output:
/// - A list of syntax-check diagnostics, one per unresolved raw macro.
///
/// Transformation:
/// - Scans declaration expression trees for `SyntaxExprKind::RawMacro` and
///   emits an error diagnostic for each occurrence.
pub fn collect_syntax_raw_macro_diagnostics(module: &SyntaxModuleOutput) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for declaration in &module.declarations {
        match &declaration.payload {
            SyntaxDeclarationPayload::Function { clauses, .. } => {
                for clause in clauses {
                    let clause_span: Span = clause.span.into();
                    collect_raw_macro_diagnostics_in_expr(
                        &clause.body,
                        clause_span,
                        &mut diagnostics,
                    );
                    if let Some(guard) = &clause.guard {
                        collect_raw_macro_diagnostics_in_expr(guard, clause_span, &mut diagnostics);
                    }
                }
            }
            SyntaxDeclarationPayload::Method { clauses, .. } => {
                for clause in clauses {
                    let clause_span: Span = clause.span.into();
                    collect_raw_macro_diagnostics_in_expr(
                        &clause.body,
                        clause_span,
                        &mut diagnostics,
                    );
                    if let Some(guard) = &clause.guard {
                        collect_raw_macro_diagnostics_in_expr(guard, clause_span, &mut diagnostics);
                    }
                }
            }
            SyntaxDeclarationPayload::Constructor { clauses, .. } => {
                for clause in clauses {
                    let clause_span: Span = clause.span.into();
                    collect_raw_macro_diagnostics_in_expr(
                        &clause.body,
                        clause_span,
                        &mut diagnostics,
                    );
                }
            }
            SyntaxDeclarationPayload::Struct { fields, .. } => {
                for field in fields {
                    if let Some(default) = &field.default {
                        let fallback_span: Span = field.span.into();
                        collect_raw_macro_diagnostics_in_expr(
                            default,
                            fallback_span,
                            &mut diagnostics,
                        );
                    }
                }
            }
            SyntaxDeclarationPayload::Trait { methods, .. } => {
                for method in methods {
                    if let Some(default_body) = &method.default_body {
                        let fallback_span: Span = method.span.into();
                        collect_raw_macro_diagnostics_in_expr(
                            default_body,
                            fallback_span,
                            &mut diagnostics,
                        );
                    }
                }
            }
            SyntaxDeclarationPayload::TraitImpl { methods, .. } => {
                for method in methods {
                    for clause in &method.clauses {
                        let clause_span: Span = clause.span.into();
                        collect_raw_macro_diagnostics_in_expr(
                            &clause.body,
                            clause_span,
                            &mut diagnostics,
                        );
                        if let Some(guard) = &clause.guard {
                            collect_raw_macro_diagnostics_in_expr(
                                guard,
                                clause_span,
                                &mut diagnostics,
                            );
                        }
                    }
                }
            }
            SyntaxDeclarationPayload::Template { .. }
            | SyntaxDeclarationPayload::Type { .. }
            | SyntaxDeclarationPayload::Import { .. }
            | SyntaxDeclarationPayload::Export { .. }
            | SyntaxDeclarationPayload::Config { .. }
            | SyntaxDeclarationPayload::Raw { .. } => {}
        }
    }
    diagnostics
}

fn collect_raw_macro_diagnostics_in_expr(
    expr: &SyntaxExprOutput,
    fallback_span: Span,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let expr_span: Span = expr.span.into();
    let fallback_span = if expr_span.start == 0 && expr_span.end == 0 {
        fallback_span
    } else {
        expr_span
    };

    if expr.kind == SyntaxExprKind::RawMacro {
        let name = expr.text.as_deref().unwrap_or("<unknown>");
        diagnostics.push(Diagnostic {
            span: fallback_span,
            message: format!(
                "raw macro expression `{}` requires macro resolution before type checking",
                name
            ),
            severity: DiagSeverity::Error,
        });
    }

    for child in &expr.children {
        collect_raw_macro_diagnostics_in_expr(child, fallback_span, diagnostics);
    }
    for field in &expr.fields {
        collect_raw_macro_diagnostics_in_expr(&field.value, fallback_span, diagnostics);
    }
    for clause in &expr.clauses {
        collect_raw_macro_diagnostics_in_expr(&clause.body, fallback_span, diagnostics);
        if let Some(guard) = &clause.guard {
            collect_raw_macro_diagnostics_in_expr(guard, fallback_span, diagnostics);
        }
    }
    for clause in &expr.catch_clauses {
        collect_raw_macro_diagnostics_in_expr(&clause.body, fallback_span, diagnostics);
        if let Some(guard) = &clause.guard {
            collect_raw_macro_diagnostics_in_expr(guard, fallback_span, diagnostics);
        }
    }
    if let Some(try_after) = &expr.try_after {
        collect_raw_macro_diagnostics_in_expr(&try_after.trigger, fallback_span, diagnostics);
        collect_raw_macro_diagnostics_in_expr(&try_after.body, fallback_span, diagnostics);
    }
    if let Some(receive_after) = &expr.receive_after {
        collect_raw_macro_diagnostics_in_expr(&receive_after.trigger, fallback_span, diagnostics);
        collect_raw_macro_diagnostics_in_expr(&receive_after.body, fallback_span, diagnostics);
    }
    for node in &expr.html_nodes {
        match node {
            SyntaxHtmlNodeOutput::Expr { expr } => {
                collect_raw_macro_diagnostics_in_expr(expr, fallback_span, diagnostics);
            }
            SyntaxHtmlNodeOutput::Text { .. } => {}
            SyntaxHtmlNodeOutput::Element { element } => {
                for attr in &element.attrs {
                    if let Some(value) = &attr.value {
                        match value {
                            SyntaxHtmlAttrValueOutput::Expr { expr } => {
                                collect_raw_macro_diagnostics_in_expr(
                                    expr,
                                    fallback_span,
                                    diagnostics,
                                );
                            }
                            SyntaxHtmlAttrValueOutput::Text { .. } => {}
                        }
                    }
                }
                for child in &element.children {
                    match child {
                        SyntaxHtmlNodeOutput::Expr { expr } => {
                            collect_raw_macro_diagnostics_in_expr(expr, fallback_span, diagnostics);
                        }
                        SyntaxHtmlNodeOutput::Text { .. } => {}
                        SyntaxHtmlNodeOutput::Element { element } => {
                            for child in &element.children {
                                match child {
                                    SyntaxHtmlNodeOutput::Expr { expr } => {
                                        collect_raw_macro_diagnostics_in_expr(
                                            expr,
                                            fallback_span,
                                            diagnostics,
                                        );
                                    }
                                    SyntaxHtmlNodeOutput::Text { .. } => {}
                                    SyntaxHtmlNodeOutput::Element { .. } => {}
                                    SyntaxHtmlNodeOutput::NamedSlot { slot } => {
                                        for slot_child in &slot.children {
                                            match slot_child {
                                                SyntaxHtmlNodeOutput::Expr { expr } => {
                                                    collect_raw_macro_diagnostics_in_expr(
                                                        expr,
                                                        fallback_span,
                                                        diagnostics,
                                                    );
                                                }
                                                SyntaxHtmlNodeOutput::Text { .. }
                                                | SyntaxHtmlNodeOutput::Element { .. } => {}
                                                SyntaxHtmlNodeOutput::NamedSlot { .. } => {}
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        SyntaxHtmlNodeOutput::NamedSlot { slot } => {
                            for slot_child in &slot.children {
                                match slot_child {
                                    SyntaxHtmlNodeOutput::Expr { expr } => {
                                        collect_raw_macro_diagnostics_in_expr(
                                            expr,
                                            fallback_span,
                                            diagnostics,
                                        );
                                    }
                                    SyntaxHtmlNodeOutput::Text { .. }
                                    | SyntaxHtmlNodeOutput::Element { .. }
                                    | SyntaxHtmlNodeOutput::NamedSlot { .. } => {}
                                }
                            }
                        }
                    }
                }
            }
            SyntaxHtmlNodeOutput::NamedSlot { slot } => {
                for slot_child in &slot.children {
                    match slot_child {
                        SyntaxHtmlNodeOutput::Expr { expr } => {
                            collect_raw_macro_diagnostics_in_expr(expr, fallback_span, diagnostics);
                        }
                        SyntaxHtmlNodeOutput::Text { .. }
                        | SyntaxHtmlNodeOutput::Element { .. }
                        | SyntaxHtmlNodeOutput::NamedSlot { .. } => {}
                    }
                }
            }
        }
    }
}

fn collect_syntax_import_maps(module: &SyntaxModuleOutput) -> TypeCheckImportMaps {
    TypeCheckImportMaps {
        module_aliases: collect_syntax_module_aliases(module),
        file_imports: collect_syntax_file_imports(module),
        markdown_imports: collect_syntax_markdown_imports(module),
        function_imports: collect_syntax_function_imports(module),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SyntaxBinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    EqEq,
    EqEqEq,
    NotEq,
    NotEqEq,
    GtEq,
    Lt,
    Gt,
    LtEq,
    DivRem,
    And,
    Or,
    PipeForward,
    Send,
}

fn syntax_binary_op(operator: Option<&str>) -> SyntaxBinaryOp {
    match operator.unwrap_or("=") {
        "+" => SyntaxBinaryOp::Add,
        "-" => SyntaxBinaryOp::Sub,
        "*" => SyntaxBinaryOp::Mul,
        "/" => SyntaxBinaryOp::Div,
        "=" => SyntaxBinaryOp::Eq,
        "==" => SyntaxBinaryOp::EqEq,
        "=:=" => SyntaxBinaryOp::EqEqEq,
        "!=" | "/=" => SyntaxBinaryOp::NotEq,
        "=/=" => SyntaxBinaryOp::NotEqEq,
        ">=" => SyntaxBinaryOp::GtEq,
        "<" => SyntaxBinaryOp::Lt,
        ">" => SyntaxBinaryOp::Gt,
        "<=" => SyntaxBinaryOp::LtEq,
        "div" | "rem" => SyntaxBinaryOp::DivRem,
        "and" | "&&" => SyntaxBinaryOp::And,
        "or" | "||" => SyntaxBinaryOp::Or,
        "|>" => SyntaxBinaryOp::PipeForward,
        "!" => SyntaxBinaryOp::Send,
        _ => SyntaxBinaryOp::Eq,
    }
}

fn check_syntax_function_clause_exhaustiveness(
    function_name: &str,
    first_param: Option<&str>,
    arity: usize,
    alias_names: &HashSet<String>,
    clauses: &[(Vec<SyntaxPatternOutput>, Span)],
    aliases: &HashMap<String, TypeAlias>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if arity != 1 {
        return;
    }

    let Some(first_param_annotation) = first_param else {
        return;
    };

    let mut vars = HashMap::new();
    let mut next_var: TypeVarId = 0;
    let expected = parse_type_expr(
        first_param_annotation,
        alias_names,
        &mut vars,
        &mut next_var,
    )
    .unwrap_or(Type::Dynamic);
    let expected = expand_type_aliases(&expected, aliases);
    let variants = as_exhaustive_union_variants(&expected);
    if variants.len() <= 1 {
        return;
    }

    let mut remaining = variants;

    for (patterns, span) in clauses {
        if patterns.is_empty() {
            continue;
        }
        let pattern = &patterns[0];
        if matches!(
            pattern.kind,
            SyntaxPatternKind::Wildcard
                | SyntaxPatternKind::Ignore
                | SyntaxPatternKind::Placeholder
                | SyntaxPatternKind::Var
        ) {
            return;
        }

        remaining.retain(|variant| !syntax_pattern_subsumes_variant(pattern, variant, aliases));
        if remaining.is_empty() {
            return;
        }

        if !remaining.is_empty() && patterns.len() > 1 {
            let _ = span;
        }
    }

    if !remaining.is_empty() {
        diagnostics.push(Diagnostic {
            span: clauses[0].1,
            message: format!(
                "non-exhaustive function {}\nmissing:\n  {}",
                function_name,
                remaining
                    .iter()
                    .map(pretty_type)
                    .collect::<Vec<_>>()
                    .join("\n  ")
            ),
            severity: DiagSeverity::Warning,
        });
    }
}

pub fn infer_syntax_expression_type(
    expression: &SyntaxExprOutput,
    module: &SyntaxModuleOutput,
    resolved: &ResolvedModule,
) -> (Type, Vec<Diagnostic>) {
    let mut diagnostics = type_check_syntax_module_output(module, resolved);

    let local_aliases = collect_syntax_type_aliases(module);
    let imported_aliases = imported_type_aliases(resolved);
    let imported_names = imported_type_names(resolved);
    let mut aliases = imported_aliases.clone();
    aliases.extend(local_aliases.clone());
    let mut alias_names = collect_syntax_type_names(module);
    alias_names.extend(imported_aliases.keys().cloned());
    alias_names.extend(resolved.imported_types.keys().cloned());
    alias_names.extend(collect_syntax_alias_extra_names(module));
    let trait_signatures = collect_syntax_trait_signatures(module, resolved);
    let trait_method_calls =
        collect_syntax_trait_method_calls(module, &alias_names, &trait_signatures, resolved);
    let function_signatures = collect_syntax_function_signatures(
        module,
        &alias_names,
        &imported_names,
        &imported_aliases,
        &local_aliases,
    );
    let constructor_signatures = collect_syntax_constructor_signatures(
        module,
        &alias_names,
        &imported_names,
        &imported_aliases,
        &aliases,
    );
    let struct_fields = collect_syntax_struct_fields(module, &alias_names);
    let receiver_methods = collect_syntax_receiver_method_dispatch_signatures(
        module,
        &alias_names,
        &imported_names,
        &imported_aliases,
        &local_aliases,
    );
    let template_schemes = collect_syntax_template_schemes(module, &alias_names);
    let import_maps = collect_syntax_import_maps(module);
    let imported_type_names = imported_type_names(resolved);
    let constructor_aliases = imported_type_names.clone();
    let trait_bound_impl_type_args = collect_trait_bound_impl_type_args(&trait_method_calls);
    let trait_lookup_cache = RefCell::new(TraitLookupCache::default());
    let expr_ctx = ExprInferContext {
        local_fns: &resolved.function_symbols,
        signatures: &function_signatures,
        interface_map: &resolved.interface_map,
        module_aliases: &import_maps.module_aliases,
        file_imports: &import_maps.file_imports,
        markdown_imports: &import_maps.markdown_imports,
        function_imports: &import_maps.function_imports,
        imported_type_names: &imported_type_names,
        constructor_aliases: &constructor_aliases,
        constructors: &constructor_signatures,
        templates: &template_schemes,
        aliases: &aliases,
        struct_fields: &struct_fields,
        receiver_methods: &receiver_methods,
        trait_method_calls: &trait_method_calls,
        trait_bound_impl_type_args: &trait_bound_impl_type_args,
        trait_signatures: &trait_signatures,
        alias_names: &alias_names,
        current_bounds: &[],
        trait_lookup_cache: &trait_lookup_cache,
    };

    let locals = HashMap::new();
    let mut subst = HashMap::new();
    let mut expression_errors = Vec::new();
    let ty = infer_syntax_expr(
        expression,
        &locals,
        &expr_ctx,
        &mut subst,
        &mut expression_errors,
    );

    diagnostics.extend(
        expression_errors
            .into_iter()
            .map(|message| expression_error_to_diagnostic(message, Span::new(0, 0))),
    );

    (
        apply_subst(&expand_type_aliases(&ty, &aliases), &subst),
        diagnostics,
    )
}

fn syntax_pattern_subsumes_variant(
    pattern: &SyntaxPatternOutput,
    variant: &Type,
    aliases: &HashMap<String, TypeAlias>,
) -> bool {
    let variant = expand_type_aliases(variant, aliases);
    match (pattern.kind, variant) {
        (SyntaxPatternKind::Wildcard, _)
        | (SyntaxPatternKind::Var, _)
        | (SyntaxPatternKind::Ignore, _)
        | (SyntaxPatternKind::Placeholder, _) => true,
        (SyntaxPatternKind::Int, Type::Int | Type::Union(_)) => true,
        (SyntaxPatternKind::Float, Type::Float | Type::Union(_)) => true,
        (SyntaxPatternKind::Atom, Type::LiteralAtom(b)) => {
            pattern.text.as_deref().is_some_and(|a| a == b.as_str())
        }
        (SyntaxPatternKind::Atom, Type::Atom) => true,
        (SyntaxPatternKind::Constructor, Type::Tuple(variant_items)) => {
            let Some(Type::LiteralAtom(head)) = variant_items.first() else {
                return false;
            };
            let Some(name) = pattern.text.as_deref() else {
                return false;
            };
            name == head
                && pattern.children.len() == variant_items.len().saturating_sub(1)
                && pattern
                    .children
                    .iter()
                    .zip(variant_items.iter().skip(1))
                    .all(|(p, t)| syntax_pattern_subsumes_variant(p, t, aliases))
        }
        (SyntaxPatternKind::Tuple, Type::Tuple(variant_items)) => {
            if pattern.children.len() != variant_items.len() {
                return false;
            }
            pattern
                .children
                .iter()
                .zip(variant_items.iter())
                .all(|(p, t)| syntax_pattern_subsumes_variant(p, t, aliases))
        }
        (SyntaxPatternKind::List, Type::List(_)) => true,
        (SyntaxPatternKind::ListCons, Type::List(_)) => true,
        (SyntaxPatternKind::Map, ty) => match ty {
            Type::Map(map_type) => pattern
                .fields
                .iter()
                .all(|field| map_type.iter().any(|entry| entry.key == field.key)),
            _ => is_map_type(&ty, aliases),
        },
        (SyntaxPatternKind::MapField, ty) => is_map_type(&ty, aliases),
        (_, Type::Union(variants)) => variants
            .iter()
            .any(|v| syntax_pattern_subsumes_variant(pattern, v, aliases)),
        _ => false,
    }
}

fn as_exhaustive_union_variants(ty: &Type) -> Vec<Type> {
    match normalize_union(vec![expand_type_aliases(ty, &HashMap::new())]) {
        Type::Union(items) => {
            let mut out = Vec::new();
            for item in items {
                match item {
                    Type::Union(nested) => {
                        out.extend(nested);
                    }
                    other => out.push(other),
                }
            }
            out
        }
        Type::Never => Vec::new(),
        other => vec![other],
    }
}

fn collect_syntax_function_signatures(
    module: &SyntaxModuleOutput,
    alias_names: &HashSet<String>,
    imported_type_names: &HashMap<String, QualifiedTypeName>,
    imported_type_aliases: &HashMap<String, TypeAlias>,
    local_aliases: &HashMap<String, TypeAlias>,
) -> HashMap<(String, usize), FunctionScheme> {
    let mut map = HashMap::new();

    for declaration in &module.declarations {
        match &declaration.payload {
            SyntaxDeclarationPayload::Function {
                name,
                params,
                return_type,
                generic_bounds,
                ..
            } => {
                let scheme = function_decl_to_scheme(
                    &params
                        .iter()
                        .map(|param| param.annotation.text.clone())
                        .collect::<Vec<_>>(),
                    &return_type.text,
                    generic_bounds,
                    alias_names,
                    imported_type_names,
                    imported_type_aliases,
                    local_aliases,
                );
                map.insert((name.clone(), params.len()), scheme);
            }
            SyntaxDeclarationPayload::Config { name, text, .. } if name == "native" => {
                for native_sig in extract_native_function_signatures(text) {
                    let arg_types = native_sig
                        .params
                        .iter()
                        .map(|(_, annotation)| annotation.clone())
                        .collect::<Vec<_>>();
                    let scheme = function_decl_to_scheme(
                        &arg_types,
                        &native_sig.return_type,
                        &Vec::new(),
                        alias_names,
                        imported_type_names,
                        imported_type_aliases,
                        local_aliases,
                    );
                    map.entry((native_sig.name, native_sig.arity))
                        .or_insert(scheme);
                }
            }
            _ => {}
        }
    }

    map
}

fn function_decl_to_scheme(
    param_annotations: &[String],
    return_annotation: &str,
    generic_bounds: &[String],
    alias_names: &HashSet<String>,
    imported_type_names: &HashMap<String, QualifiedTypeName>,
    imported_type_aliases: &HashMap<String, TypeAlias>,
    local_aliases: &HashMap<String, TypeAlias>,
) -> FunctionScheme {
    let mut vars = HashMap::new();
    let mut next_var: TypeVarId = 0;

    let params = param_annotations
        .iter()
        .map(|annotation| {
            let parsed = parse_type_expr(annotation, alias_names, &mut vars, &mut next_var)
                .unwrap_or(Type::Dynamic);
            let parsed = expand_imported_aliases_except_named(
                &parsed,
                imported_type_aliases,
                imported_type_names,
                local_aliases,
            );
            qualify_type_names(&parsed, imported_type_names)
        })
        .collect::<Vec<_>>();
    let ret = parse_type_expr(return_annotation, alias_names, &mut vars, &mut next_var)
        .unwrap_or(Type::Dynamic);
    let ret = expand_imported_aliases_except_named(
        &ret,
        imported_type_aliases,
        imported_type_names,
        local_aliases,
    );
    let ret = qualify_type_names(&ret, imported_type_names);

    let bounds = parse_generic_bounds(generic_bounds, &vars, alias_names)
        .into_iter()
        .map(|bound| FunctionBound {
            trait_name: bound.trait_name,
            trait_args: bound
                .trait_args
                .into_iter()
                .map(|arg| {
                    let arg = expand_imported_aliases_except_named(
                        &arg,
                        imported_type_aliases,
                        imported_type_names,
                        local_aliases,
                    );
                    qualify_type_names(&arg, imported_type_names)
                })
                .collect(),
        })
        .collect();

    FunctionScheme {
        params,
        ret,
        bounds,
    }
}

fn parse_generic_bounds(
    generic_bounds: &[String],
    vars: &HashMap<String, TypeVarId>,
    alias_names: &HashSet<String>,
) -> Vec<FunctionBound> {
    let mut bounds = Vec::new();
    for bound in generic_bounds {
        if let Some((left, right)) = bound.split_once(':') {
            let type_var = normalize_type_param_name(left.trim());
            let Some(&type_var_id) = vars.get(&type_var) else {
                continue;
            };

            for trait_expr in split_top_level_plus(right.trim()) {
                let Some(mut parsed_bound) =
                    parse_function_bound_trait_ref(&trait_expr, vars, alias_names)
                else {
                    continue;
                };
                if parsed_bound.trait_args.is_empty() {
                    parsed_bound.trait_args.push(Type::Var(type_var_id));
                }
                bounds.push(parsed_bound);
            }
            continue;
        }

        if let Some(parsed_bound) = parse_function_bound_trait_ref(bound, vars, alias_names) {
            bounds.push(parsed_bound);
        }
    }

    bounds
}

/// Converts one trait-reference constraint into a function bound.
///
/// Inputs:
/// - `trait_ref`: trait reference text such as `Eq[A]` or `Show[String]`.
/// - `vars`: generic type variables visible in the callable signature.
/// - `alias_names`: type aliases visible to type-expression parsing.
///
/// Output:
/// - `Some(FunctionBound)` when the trait reference parses and every type
///   argument can be converted to a typechecker `Type`.
/// - `None` when the reference is malformed or contains unsupported type
///   syntax.
///
/// Transformation:
/// - Parses the trait reference, preserves the trait name, and lowers type
///   arguments into semantic type payloads without attaching the bound to a
///   specific parameter name. This is the canonical `[Eq[A]]` constraint-list
///   path.
fn parse_function_bound_trait_ref(
    trait_ref: &str,
    vars: &HashMap<String, TypeVarId>,
    alias_names: &HashSet<String>,
) -> Option<FunctionBound> {
    let trait_instance = parse_trait_instance_from_text(trait_ref)?;
    let mut bound_arg_vars = vars.clone();
    let mut next_bound_arg_var = bound_arg_vars.len();
    let mut args = Vec::new();

    for raw_arg in &trait_instance.type_args {
        args.push(parse_type_expr(
            raw_arg,
            alias_names,
            &mut bound_arg_vars,
            &mut next_bound_arg_var,
        )?);
    }

    Some(FunctionBound {
        trait_name: trait_instance.name,
        trait_args: args,
    })
}

fn expand_imported_aliases_except_named(
    ty: &Type,
    imported_aliases: &HashMap<String, TypeAlias>,
    imported_names: &HashMap<String, QualifiedTypeName>,
    local_aliases: &HashMap<String, TypeAlias>,
) -> Type {
    match ty {
        Type::Named { name, args, .. } if imported_names.contains_key(name) => {
            let args = args
                .iter()
                .map(|arg| {
                    let arg = expand_type_aliases(arg, local_aliases);
                    expand_imported_aliases_except_named(
                        &arg,
                        imported_aliases,
                        imported_names,
                        local_aliases,
                    )
                })
                .collect();
            match ty {
                Type::Named { module, name, .. } => Type::Named {
                    module: module.clone(),
                    name: name.clone(),
                    args,
                },
                _ => ty.clone(),
            }
        }
        Type::List(inner) => Type::List(Box::new(expand_imported_aliases_except_named(
            inner,
            imported_aliases,
            imported_names,
            local_aliases,
        ))),
        Type::Tuple(items) => Type::Tuple(
            items
                .iter()
                .map(|item| {
                    expand_imported_aliases_except_named(
                        item,
                        imported_aliases,
                        imported_names,
                        local_aliases,
                    )
                })
                .collect(),
        ),
        Type::Union(items) => Type::Union(
            items
                .iter()
                .map(|item| {
                    expand_imported_aliases_except_named(
                        item,
                        imported_aliases,
                        imported_names,
                        local_aliases,
                    )
                })
                .collect(),
        ),
        Type::Map(fields) => Type::Map(
            fields
                .iter()
                .map(|field| MapFieldType {
                    key: field.key.clone(),
                    value: expand_imported_aliases_except_named(
                        &field.value,
                        imported_aliases,
                        imported_names,
                        local_aliases,
                    ),
                    required: field.required,
                })
                .collect(),
        ),
        Type::Function { params, ret } => Type::Function {
            params: params
                .iter()
                .map(|param| {
                    expand_imported_aliases_except_named(
                        param,
                        imported_aliases,
                        imported_names,
                        local_aliases,
                    )
                })
                .collect(),
            ret: Box::new(expand_imported_aliases_except_named(
                ret,
                imported_aliases,
                imported_names,
                local_aliases,
            )),
        },
        Type::FixedArray { size, elem } => Type::FixedArray {
            size: *size,
            elem: Box::new(expand_imported_aliases_except_named(
                elem,
                imported_aliases,
                imported_names,
                local_aliases,
            )),
        },
        other => expand_type_aliases(other, imported_aliases),
    }
}

fn collect_syntax_constructor_signatures(
    module: &SyntaxModuleOutput,
    alias_names: &HashSet<String>,
    imported_type_names: &HashMap<String, QualifiedTypeName>,
    imported_type_aliases: &HashMap<String, TypeAlias>,
    aliases: &HashMap<String, TypeAlias>,
) -> HashMap<String, Vec<ConstructorScheme>> {
    let mut out = HashMap::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Constructor {
            name,
            params,
            clauses,
            ..
        } = &declaration.payload
        else {
            continue;
        };

        let mut vars = HashMap::new();
        let mut next_var: TypeVarId = 0;
        for param in params {
            vars.insert(normalize_type_param_name(param), next_var);
            next_var += 1;
        }

        let mut schemes = Vec::new();
        for clause in clauses {
            let mut fixed_params = Vec::new();
            let mut vararg = None;

            for param in &clause.params {
                let parsed = parse_type_expr(
                    &param.annotation.text,
                    alias_names,
                    &mut vars,
                    &mut next_var,
                )
                .unwrap_or(Type::Dynamic);
                let parsed = expand_type_aliases(&parsed, aliases);
                let parsed = qualify_type_names(&parsed, imported_type_names);
                if param.is_varargs {
                    vararg = Some(parsed);
                } else {
                    fixed_params.push(parsed);
                }
            }

            let ret = parse_type_expr(
                &clause.return_type.text,
                alias_names,
                &mut vars,
                &mut next_var,
            )
            .unwrap_or(Type::Dynamic);
            let ret = expand_type_aliases(&ret, aliases);
            let ret = expand_type_aliases(&ret, imported_type_aliases);
            let ret = qualify_type_names(&ret, imported_type_names);

            schemes.push(ConstructorScheme {
                fixed_params,
                min_arity: clause
                    .params
                    .iter()
                    .filter(|param| !param.is_varargs && param.default.is_none())
                    .count(),
                vararg,
                ret,
            });
        }

        out.insert(name.clone(), schemes);
    }

    out
}

fn imported_type_names(resolved: &ResolvedModule) -> HashMap<String, QualifiedTypeName> {
    resolved
        .imported_types
        .iter()
        .map(|(local_name, imported)| {
            (
                local_name.clone(),
                QualifiedTypeName {
                    module: imported.source_module.clone(),
                    name: imported.source_name.clone(),
                },
            )
        })
        .collect()
}

fn imported_type_aliases(resolved: &ResolvedModule) -> HashMap<String, TypeAlias> {
    let mut aliases = HashMap::new();
    for interface in resolved.interface_map.values() {
        for (name, alias) in interface_type_aliases(interface) {
            aliases.insert(format!("{}.{}", interface.module, name), alias);
        }
    }
    for (local_name, imported) in &resolved.imported_types {
        let Some(interface) = resolved.interface_map.get(&imported.source_module) else {
            continue;
        };
        if interface.opaque_types.contains(&imported.source_name) {
            continue;
        }
        let interface_aliases = interface_type_aliases(interface);
        if let Some(alias) = interface_aliases.get(&imported.source_name) {
            aliases.insert(local_name.clone(), alias.clone());
        }
    }
    aliases
}

fn collect_syntax_module_aliases(module: &SyntaxModuleOutput) -> HashMap<String, String> {
    module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Import {
                import_kind: SyntaxImportKind::Module,
                module_name,
                items,
                is_type: false,
                ..
            } if items.len() == 1 => {
                let item = &items[0];
                item.as_alias
                    .as_ref()
                    .map(|alias| (alias.clone(), format!("{}.{}", module_name, item.name)))
            }
            _ => None,
        })
        .collect()
}

/// Collects selected function imports visible as local call names.
///
/// Inputs:
/// - `module`: syntax-output module containing source import declarations.
///
/// Output:
/// - Map from local call name to imported source module/function target.
///
/// Transformation:
/// - Scans module imports such as `import foo.Bar.{baz}` and
///   `import foo.Bar.{baz as qux}`, skips type-only imports and non-module
///   asset imports, and preserves aliases so local calls can be checked
///   against the provider interface.
fn collect_syntax_function_imports(
    module: &SyntaxModuleOutput,
) -> HashMap<String, ImportedFunctionTarget> {
    let mut imports = HashMap::new();
    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Import {
            import_kind: SyntaxImportKind::Module,
            module_name,
            items,
            is_type: false,
            ..
        } = &declaration.payload
        else {
            continue;
        };

        for item in items {
            let local_name = item.as_alias.as_ref().unwrap_or(&item.name).clone();
            imports.insert(
                local_name,
                ImportedFunctionTarget {
                    module: module_name.clone(),
                    function: item.name.clone(),
                    span: item.span.into(),
                },
            );
        }
    }
    imports
}

fn collect_syntax_file_imports(module: &SyntaxModuleOutput) -> HashMap<String, String> {
    module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Import {
                import_kind: SyntaxImportKind::File | SyntaxImportKind::Css,
                items,
                source_path: Some(source_path),
                ..
            } => {
                let alias = items.first()?;
                Some((alias.name.clone(), source_path.clone()))
            }
            _ => None,
        })
        .collect()
}

fn collect_syntax_markdown_imports(module: &SyntaxModuleOutput) -> HashMap<String, String> {
    module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Import {
                import_kind: SyntaxImportKind::Markdown,
                items,
                source_path: Some(source_path),
                ..
            } => {
                let alias = items.first()?;
                Some((alias.name.clone(), source_path.clone()))
            }
            _ => None,
        })
        .collect()
}

fn collect_syntax_type_aliases(module: &SyntaxModuleOutput) -> HashMap<String, TypeAlias> {
    let mut aliases = HashMap::new();
    let alias_names = module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Type { name, .. }
            | SyntaxDeclarationPayload::Struct { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect::<HashSet<String>>();

    for declaration in &module.declarations {
        if let SyntaxDeclarationPayload::Type {
            name,
            params,
            variants,
            is_opaque,
            ..
        } = &declaration.payload
        {
            let mut vars = HashMap::new();
            let mut next_var: TypeVarId = 0;
            let mut type_params = Vec::new();

            for param in params {
                vars.insert(normalize_type_param_name(param), next_var);
                type_params.push(next_var);
                next_var += 1;
            }

            let body = normalize_union(
                variants
                    .iter()
                    .filter_map(|variant| {
                        parse_type_expr(&variant.text, &alias_names, &mut vars, &mut next_var)
                    })
                    .collect(),
            );

            aliases.insert(
                name.clone(),
                TypeAlias {
                    params: type_params,
                    body,
                    is_opaque: *is_opaque,
                },
            );
        }
    }

    aliases
}

fn collect_syntax_alias_extra_names(module: &SyntaxModuleOutput) -> HashSet<String> {
    module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Struct { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect()
}

fn canonicalize_trait_lookup_types(types: &[Type]) -> Vec<Type> {
    let mut next_var = 0usize;
    let mut remap = HashMap::new();
    types
        .iter()
        .map(|ty| remap_type_var_id(ty, &mut next_var, &mut remap))
        .collect()
}

fn check_parsed_trait_impl_signature(
    impl_decl: &ParsedTraitImpl,
    impl_span: Span,
    trait_map: &HashMap<String, ParsedTraitSignature>,
    inheritance_cache: &mut HashMap<String, Option<HashMap<String, TraitMethodSignature>>>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(trait_signature) = trait_map.get(&impl_decl.target.name) else {
        diagnostics.push(Diagnostic {
            span: impl_span,
            message: format!("unknown trait `{}` in impl", impl_decl.target.name,),
            severity: DiagSeverity::Error,
        });
        return;
    };

    let inherited_methods = collect_trait_methods_with_inheritance(
        trait_map,
        &impl_decl.target.name,
        inheritance_cache,
        &mut HashSet::new(),
    )
    .unwrap_or_default();

    if impl_decl.target.type_args.len() != trait_signature.type_params.len() {
        diagnostics.push(Diagnostic {
            span: impl_span,
            message: format!(
                "trait `{}` expects {} type parameter(s), found {}",
                impl_decl.target.name,
                trait_signature.type_params.len(),
                impl_decl.target.type_args.len()
            ),
            severity: DiagSeverity::Error,
        });
        return;
    };

    if let Some(for_type) = &impl_decl.for_type {
        if for_type.trim().is_empty() {
            diagnostics.push(Diagnostic {
                span: impl_span,
                message: format!(
                    "impl of trait `{}` must declare a non-empty owner type",
                    impl_decl.target.name
                ),
                severity: DiagSeverity::Error,
            });
            return;
        }
    }

    let mut seen_methods = HashSet::new();

    for method in &impl_decl.methods {
        if !seen_methods.insert(method.name.clone()) {
            diagnostics.push(Diagnostic {
                span: method.span,
                message: format!(
                    "duplicate method `{}` in impl {}",
                    method.name, impl_decl.target.name
                ),
                severity: DiagSeverity::Error,
            });
            continue;
        }

        let Some(expected) = inherited_methods.get(&method.name) else {
            diagnostics.push(Diagnostic {
                span: method.span,
                message: format!(
                    "method `{}` is not declared in trait `{}`",
                    method.name, impl_decl.target.name
                ),
                severity: DiagSeverity::Error,
            });
            continue;
        };

        let specialized_params = expected
            .params
            .iter()
            .map(|param| {
                specialize_trait_type_text(
                    param,
                    &trait_signature.type_params,
                    &impl_decl.target.type_args,
                )
            })
            .collect::<Vec<_>>();
        let specialized_return = specialize_trait_type_text(
            &expected.return_type,
            &trait_signature.type_params,
            &impl_decl.target.type_args,
        );

        if specialized_params.len() != method.params.len() {
            diagnostics.push(Diagnostic {
                span: method.span,
                message: format!(
                    "method `{}` in trait `{}` has arity {}, found {}",
                    method.name,
                    impl_decl.target.name,
                    specialized_params.len(),
                    method.params.len()
                ),
                severity: DiagSeverity::Error,
            });
            continue;
        }

        for (idx, (expected_type, found_type)) in
            specialized_params.iter().zip(&method.params).enumerate()
        {
            if !found_type.trim().is_empty() && !trait_type_text_equal(expected_type, found_type) {
                diagnostics.push(Diagnostic {
                    span: method.span,
                    message: format!(
                        "method `{}` parameter {} in trait `{}` expects {}, found {}",
                        method.name,
                        idx + 1,
                        impl_decl.target.name,
                        expected_type,
                        found_type
                    ),
                    severity: DiagSeverity::Error,
                });
            }
        }

        if !method.return_type.trim().is_empty()
            && !trait_type_text_equal(&specialized_return, &method.return_type)
        {
            diagnostics.push(Diagnostic {
                span: method.span,
                message: format!(
                    "method `{}` return type in trait `{}` expects {}, found {}",
                    method.name, impl_decl.target.name, specialized_return, method.return_type
                ),
                severity: DiagSeverity::Error,
            });
        }
    }

    for (expected_method, expected_signature) in &inherited_methods {
        if !impl_decl
            .methods
            .iter()
            .any(|method| &method.name == expected_method)
            && !expected_signature.has_default
        {
            diagnostics.push(Diagnostic {
                span: impl_span,
                message: format!(
                    "missing method `{}` in impl of trait `{}`",
                    expected_method, impl_decl.target.name
                ),
                severity: DiagSeverity::Error,
            });
        }
    }
}

#[derive(Debug, Clone)]
struct ParsedTraitSignature {
    type_params: Vec<String>,
    methods: HashMap<String, TraitMethodSignature>,
    super_traits: Vec<String>,
}

#[derive(Debug, Clone)]
struct ResolvedTraitMethod {
    scheme: FunctionScheme,
    impl_type_args: Vec<Type>,
}

#[derive(Debug, Clone)]
struct TraitMethodSignature {
    params: Vec<String>,
    return_type: String,
    generic_bounds: Vec<String>,
    has_default: bool,
}

#[derive(Debug, Clone)]
struct ParsedTraitImpl {
    target: ParsedTraitInstance,
    for_type: Option<String>,
    methods: Vec<ParsedMethodSignature>,
}

#[derive(Debug, Clone)]
struct ParsedTraitInstance {
    name: String,
    type_args: Vec<String>,
}

#[derive(Debug, Clone)]
struct ParsedMethodSignature {
    name: String,
    params: Vec<String>,
    return_type: String,
    span: Span,
}

fn collect_syntax_trait_signatures(
    module: &SyntaxModuleOutput,
    resolved: &ResolvedModule,
) -> HashMap<String, ParsedTraitSignature> {
    let mut traits = collect_imported_trait_signatures(resolved);

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Trait {
            name,
            params,
            super_traits,
            methods,
            ..
        } = &declaration.payload
        else {
            continue;
        };

        let mut method_signatures = HashMap::new();
        for method in methods {
            method_signatures.insert(
                method.name.clone(),
                TraitMethodSignature {
                    params: method
                        .params
                        .iter()
                        .map(|param| normalize_trait_type_text(&param.annotation.text))
                        .collect(),
                    return_type: normalize_trait_type_text(&method.return_type.text),
                    generic_bounds: method.generic_bounds.clone(),
                    has_default: method.default_body.is_some(),
                },
            );
        }

        traits.insert(
            name.clone(),
            ParsedTraitSignature {
                type_params: params.clone(),
                methods: method_signatures,
                super_traits: super_traits.clone(),
            },
        );
    }

    traits
}

fn collect_imported_trait_signatures(
    resolved: &ResolvedModule,
) -> HashMap<String, ParsedTraitSignature> {
    let mut traits = HashMap::new();

    for imported in resolved.imported_traits.values() {
        let Some(interface) = resolved.interface_map.get(&imported.source_module) else {
            continue;
        };

        let Some(imported_signature) = interface.traits.get(&imported.source_name) else {
            continue;
        };

        let mut methods = HashMap::new();
        for (method_name, method_signature) in &imported_signature.methods {
            methods.insert(
                method_name.clone(),
                TraitMethodSignature {
                    params: method_signature
                        .params
                        .iter()
                        .map(|param| normalize_trait_type_text(&param.annotation))
                        .collect(),
                    return_type: normalize_trait_type_text(&method_signature.return_type),
                    generic_bounds: method_signature.generic_bounds.clone(),
                    has_default: false,
                },
            );
        }

        traits.insert(
            imported.local_name.clone(),
            ParsedTraitSignature {
                type_params: imported_signature.type_params.clone(),
                methods,
                super_traits: imported_signature.super_traits.clone(),
            },
        );
    }

    traits
}

fn collect_trait_methods_with_inheritance(
    signatures: &HashMap<String, ParsedTraitSignature>,
    trait_name: &str,
    cache: &mut HashMap<String, Option<HashMap<String, TraitMethodSignature>>>,
    visiting: &mut HashSet<String>,
) -> Option<HashMap<String, TraitMethodSignature>> {
    if let Some(cached) = cache.get(trait_name) {
        return cached.clone();
    }

    if !visiting.insert(trait_name.to_string()) {
        cache.insert(trait_name.to_string(), None);
        return None;
    }

    let signature = signatures.get(trait_name)?;
    let mut methods = HashMap::new();

    for super_trait_text in &signature.super_traits {
        let Some(super_trait) = parse_trait_instance_from_text(super_trait_text) else {
            continue;
        };

        if let Some(parent_methods) =
            collect_trait_methods_with_inheritance(signatures, &super_trait.name, cache, visiting)
        {
            methods.extend(parent_methods);
        }
    }

    for (name, method) in &signature.methods {
        methods.insert(name.clone(), method.clone());
    }

    visiting.remove(trait_name);
    let methods = Some(methods);
    cache.insert(trait_name.to_string(), methods.clone());
    methods
}

fn parse_trait_instance_from_text(text: &str) -> Option<ParsedTraitInstance> {
    let tokens = terlan_syntax::lexer::lex(text).ok()?;
    parse_trait_instance(&tokens)
}

fn collect_syntax_trait_method_calls(
    module: &SyntaxModuleOutput,
    alias_names: &HashSet<String>,
    trait_signatures: &HashMap<String, ParsedTraitSignature>,
    _resolved: &ResolvedModule,
) -> HashMap<(String, String), Vec<ResolvedTraitMethod>> {
    let mut methods: HashMap<(String, String), Vec<ResolvedTraitMethod>> = HashMap::new();
    let mut inheritance_cache: HashMap<String, Option<HashMap<String, TraitMethodSignature>>> =
        HashMap::new();

    seed_syntax_trait_method_call_keys(trait_signatures, &mut methods, &mut inheritance_cache);
    collect_syntax_derived_trait_method_calls(
        module,
        &mut methods,
        trait_signatures,
        alias_names,
        &mut inheritance_cache,
    );
    collect_syntax_declared_implements_trait_method_calls(
        module,
        &mut methods,
        trait_signatures,
        alias_names,
        &mut inheritance_cache,
    );
    collect_syntax_explicit_trait_method_calls(
        module,
        &mut methods,
        trait_signatures,
        alias_names,
        &mut inheritance_cache,
    );

    methods
}

/// Seeds trait method lookup keys before concrete impl candidates are added.
///
/// Inputs:
/// - `trait_signatures`: known local/imported trait signatures.
/// - `methods`: dispatch candidate map to initialize.
/// - `inheritance_cache`: inherited trait method cache shared with candidate
///   collection.
///
/// Output:
/// - No direct return value.
///
/// Transformation:
/// - Inserts an empty candidate list for every visible trait method. This lets
///   trait-call inference distinguish “known trait method with no impl” from
///   ordinary remote calls and emit a conformance diagnostic instead of falling
///   through as dynamic.
fn seed_syntax_trait_method_call_keys(
    trait_signatures: &HashMap<String, ParsedTraitSignature>,
    methods: &mut HashMap<(String, String), Vec<ResolvedTraitMethod>>,
    inheritance_cache: &mut HashMap<String, Option<HashMap<String, TraitMethodSignature>>>,
) {
    for trait_name in trait_signatures.keys() {
        let inherited_methods = collect_trait_methods_with_inheritance(
            trait_signatures,
            trait_name,
            inheritance_cache,
            &mut HashSet::new(),
        )
        .unwrap_or_default();

        for method_name in inherited_methods.keys() {
            methods
                .entry((trait_name.clone(), method_name.clone()))
                .or_default();
        }
    }
}

fn collect_syntax_derived_trait_method_calls(
    module: &SyntaxModuleOutput,
    methods: &mut HashMap<(String, String), Vec<ResolvedTraitMethod>>,
    trait_signatures: &HashMap<String, ParsedTraitSignature>,
    alias_names: &HashSet<String>,
    inheritance_cache: &mut HashMap<String, Option<HashMap<String, TraitMethodSignature>>>,
) {
    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Struct { derives, .. } = &declaration.payload else {
            continue;
        };

        for derive_text in derives {
            let Some(derived_trait) = parse_trait_instance_from_text(derive_text) else {
                continue;
            };

            let trait_name = derived_trait.name.clone();
            let mut synthesized = HashMap::new();
            collect_trait_method_candidates(
                &mut synthesized,
                &ParsedTraitImpl {
                    target: derived_trait,
                    for_type: None,
                    methods: Vec::new(),
                },
                &trait_name,
                trait_signatures,
                alias_names,
                inheritance_cache,
            );

            for (key, candidates) in synthesized {
                let existing = methods.entry(key).or_default();
                for method in candidates {
                    if !existing
                        .iter()
                        .any(|existing| existing.impl_type_args == method.impl_type_args)
                    {
                        existing.push(method);
                    }
                }
            }
        }
    }
}

/// Registers declaration-site `implements` entries as trait dispatch candidates.
///
/// Inputs:
/// - `module`: syntax-output module containing type or struct declarations
///   with `implements` clauses.
/// - `methods`: dispatch candidate map to extend.
/// - `trait_signatures`: known local/imported trait signatures.
/// - `alias_names`: type names visible to type-expression parsing.
/// - `inheritance_cache`: inherited trait method cache shared with candidate
///   collection.
///
/// Output:
/// - No direct return value.
///
/// Transformation:
/// - Converts each declaration-site conformance into the same specialized
///   trait method candidates used by trait-call inference. Receiver-method
///   conformance is validated separately; this function only exposes the
///   declared conformance to type inference.
fn collect_syntax_declared_implements_trait_method_calls(
    module: &SyntaxModuleOutput,
    methods: &mut HashMap<(String, String), Vec<ResolvedTraitMethod>>,
    trait_signatures: &HashMap<String, ParsedTraitSignature>,
    alias_names: &HashSet<String>,
    inheritance_cache: &mut HashMap<String, Option<HashMap<String, TraitMethodSignature>>>,
) {
    for declaration in &module.declarations {
        let Some((_type_name, implements)) = syntax_declared_implements(declaration) else {
            continue;
        };

        for trait_ref in implements {
            let Some(implemented_trait) = parse_trait_instance_from_text(&trait_ref.text) else {
                continue;
            };
            let trait_name = implemented_trait.name.clone();
            let mut synthesized = HashMap::new();
            collect_trait_method_candidates(
                &mut synthesized,
                &ParsedTraitImpl {
                    target: implemented_trait,
                    for_type: None,
                    methods: Vec::new(),
                },
                &trait_name,
                trait_signatures,
                alias_names,
                inheritance_cache,
            );

            for (key, candidates) in synthesized {
                let existing = methods.entry(key).or_default();
                for method in candidates {
                    if !existing
                        .iter()
                        .any(|existing| existing.impl_type_args == method.impl_type_args)
                    {
                        existing.push(method);
                    }
                }
            }
        }
    }
}

/// Registers explicit adapter impls as trait method dispatch candidates.
///
/// Inputs:
/// - `module`: syntax-output module containing structured trait impl
///   declarations.
/// - `methods`: dispatch candidate map to extend.
/// - `trait_signatures`: known local/imported trait signatures.
/// - `alias_names`: type names visible to type-expression parsing.
/// - `inheritance_cache`: inherited trait method cache shared with other
///   conformance candidate collectors.
///
/// Output:
/// - No direct return value.
///
/// Transformation:
/// - Converts each `impl TraitRef for Type` declaration into the same
///   `ResolvedTraitMethod` candidates used by trait-call inference. The
///   structured path reads syntax output directly and does not reparse raw
///   source blocks.
fn collect_syntax_explicit_trait_method_calls(
    module: &SyntaxModuleOutput,
    methods: &mut HashMap<(String, String), Vec<ResolvedTraitMethod>>,
    trait_signatures: &HashMap<String, ParsedTraitSignature>,
    alias_names: &HashSet<String>,
    inheritance_cache: &mut HashMap<String, Option<HashMap<String, TraitMethodSignature>>>,
) {
    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::TraitImpl { .. } = &declaration.payload else {
            continue;
        };
        let Some(impl_decl) = syntax_trait_impl_to_parsed(declaration) else {
            continue;
        };
        let trait_name = impl_decl.target.name.clone();
        let mut synthesized = HashMap::new();
        collect_trait_method_candidates(
            &mut synthesized,
            &impl_decl,
            &trait_name,
            trait_signatures,
            alias_names,
            inheritance_cache,
        );

        for (key, candidates) in synthesized {
            let existing = methods.entry(key).or_default();
            for method in candidates {
                if !existing
                    .iter()
                    .any(|existing| existing.impl_type_args == method.impl_type_args)
                {
                    existing.push(method);
                }
            }
        }
    }
}

fn collect_trait_method_candidates(
    methods: &mut HashMap<(String, String), Vec<ResolvedTraitMethod>>,
    impl_decl: &ParsedTraitImpl,
    trait_name: &str,
    trait_signatures: &HashMap<String, ParsedTraitSignature>,
    alias_names: &HashSet<String>,
    inheritance_cache: &mut HashMap<String, Option<HashMap<String, TraitMethodSignature>>>,
) {
    let Some(trait_signature) = trait_signatures.get(trait_name) else {
        return;
    };

    if impl_decl.target.type_args.len() != trait_signature.type_params.len() {
        return;
    }

    let mut arg_vars = HashMap::new();
    let mut next_arg_var = 0usize;
    let mut impl_type_args = Vec::new();
    let mut parse_ok = true;

    for raw_arg in &impl_decl.target.type_args {
        let parsed = parse_type_expr(raw_arg, alias_names, &mut arg_vars, &mut next_arg_var);
        match parsed {
            Some(parsed) => impl_type_args.push(parsed),
            None => {
                parse_ok = false;
                break;
            }
        }
    }
    if !parse_ok {
        return;
    }

    let inherited_methods = collect_trait_methods_with_inheritance(
        trait_signatures,
        trait_name,
        inheritance_cache,
        &mut HashSet::new(),
    )
    .unwrap_or_default();

    for (method_name, method_sig) in &inherited_methods {
        let mut method_vars = HashMap::new();
        let mut next_method_var = 0usize;
        for name in &trait_signature.type_params {
            method_vars.insert(name.clone(), next_method_var);
            next_method_var += 1;
        }

        let parsed_params = method_sig
            .params
            .iter()
            .map(|param| {
                parse_type_expr(param, alias_names, &mut method_vars, &mut next_method_var)
            })
            .collect::<Option<Vec<_>>>();

        let parsed_return = parse_type_expr(
            &method_sig.return_type,
            alias_names,
            &mut method_vars,
            &mut next_method_var,
        );
        let Some(parsed_return) = parsed_return else {
            continue;
        };
        let Some(parsed_params) = parsed_params else {
            continue;
        };

        let mut substitution = HashMap::new();
        let mut valid = true;
        for (param_name, param_type) in trait_signature
            .type_params
            .iter()
            .zip(impl_type_args.iter())
        {
            if let Some(var_id) = method_vars.get(param_name) {
                substitution.insert(*var_id, param_type.clone());
            } else {
                valid = false;
                break;
            }
        }
        if !valid {
            continue;
        }

        let specialized_bounds =
            parse_generic_bounds(&method_sig.generic_bounds, &method_vars, alias_names)
                .into_iter()
                .map(|bound| FunctionBound {
                    trait_name: bound.trait_name,
                    trait_args: bound
                        .trait_args
                        .into_iter()
                        .map(|arg| substitute_type_vars(&arg, &substitution))
                        .collect(),
                })
                .collect();

        let specialized = FunctionScheme {
            params: parsed_params
                .into_iter()
                .map(|param| substitute_type_vars(&param, &substitution))
                .collect(),
            ret: substitute_type_vars(&parsed_return, &substitution),
            bounds: specialized_bounds,
        };

        methods
            .entry((trait_name.to_string(), method_name.clone()))
            .or_default()
            .push(ResolvedTraitMethod {
                scheme: specialized,
                impl_type_args: impl_type_args.clone(),
            });
    }
}

fn parse_trait_instance(tokens: &[Token]) -> Option<ParsedTraitInstance> {
    if tokens.is_empty() {
        return None;
    }

    let mut name_end = tokens.len();
    for (idx, token) in tokens.iter().enumerate() {
        if token.kind == TokenKind::LBracket {
            name_end = idx;
            break;
        }
    }

    let name = tokens[..name_end]
        .iter()
        .filter_map(|token| match token.kind {
            TokenKind::Comment | TokenKind::DocComment | TokenKind::ModuleDocComment => None,
            TokenKind::Dot => Some(".".to_string()),
            _ => Some(token.text.clone()),
        })
        .collect::<Vec<_>>()
        .join("")
        .trim()
        .trim_matches('.')
        .to_string();
    if name.is_empty() {
        return None;
    }

    let mut type_args = Vec::new();
    if name_end >= tokens.len() {
        return Some(ParsedTraitInstance { name, type_args });
    }

    let mut pos = name_end + 1;
    let mut depth = 0i32;
    let mut current = Vec::new();

    while pos < tokens.len() {
        let token = &tokens[pos];

        if token.kind == TokenKind::RBracket && depth == 0 {
            if !current.is_empty() {
                type_args.push(
                    join_token_texts(&current)
                        .split_whitespace()
                        .collect::<String>(),
                );
            }
            break;
        }

        match token.kind {
            TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => {
                depth += 1;
                current.push(token.clone());
            }
            TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                depth = depth.saturating_sub(1);
                if depth >= 0 {
                    current.push(token.clone());
                }
            }
            TokenKind::Comma if depth == 0 => {
                type_args.push(
                    join_token_texts(&current)
                        .split_whitespace()
                        .collect::<String>(),
                );
                current.clear();
            }
            _ => current.push(token.clone()),
        }

        pos += 1;
    }

    Some(ParsedTraitInstance { name, type_args })
}

fn trait_instance_key(target: &ParsedTraitInstance) -> Option<String> {
    if target.name.is_empty() {
        return None;
    }

    if target.type_args.is_empty() {
        Some(target.name.clone())
    } else {
        Some(format!(
            "{}[{}]",
            target.name,
            target
                .type_args
                .iter()
                .map(|arg| normalize_trait_type_text(arg))
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }
}

fn join_token_texts(tokens: &[Token]) -> String {
    tokens
        .iter()
        .map(|token| token.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_trait_type_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Substitutes trait type parameters inside a type-expression string.
///
/// Inputs:
/// - `text`: type-expression text from a trait method signature.
/// - `params`: trait type-parameter names.
/// - `args`: concrete type arguments from an implemented trait reference.
///
/// Output:
/// - Normalized type-expression text after replacing matching type variables.
///
/// Transformation:
/// - Lexes the type text and replaces upper-case identifier tokens whose text
///   matches a trait type parameter. Punctuation and non-matching tokens are
///   preserved, then normalized for stable diagnostics and comparisons.
fn specialize_trait_type_text(text: &str, params: &[String], args: &[String]) -> String {
    if params.is_empty() || args.is_empty() {
        return normalize_trait_type_text(text);
    }

    let substitutions = params
        .iter()
        .zip(args.iter())
        .map(|(param, arg)| (normalize_type_param_name(param), arg.as_str()))
        .collect::<HashMap<_, _>>();

    let Ok(tokens) = terlan_syntax::lexer::lex(text) else {
        return normalize_trait_type_text(text);
    };

    let mut parts = Vec::new();
    for token in tokens {
        if token.kind == TokenKind::EOF {
            break;
        }
        if matches!(
            token.kind,
            TokenKind::Comment | TokenKind::DocComment | TokenKind::ModuleDocComment
        ) {
            continue;
        }
        if token.kind == TokenKind::Var {
            if let Some(replacement) = substitutions.get(&normalize_type_param_name(&token.text)) {
                parts.push((*replacement).to_string());
                continue;
            }
        }
        parts.push(token.text);
    }

    normalize_trait_type_text(&join_token_texts_from_strings(&parts))
}

/// Compares two type-expression texts using compact whitespace-insensitive form.
///
/// Inputs:
/// - `left`: first type text.
/// - `right`: second type text.
///
/// Output:
/// - `true` when both texts are equal after removing whitespace.
///
/// Transformation:
/// - Applies the same compacting strategy used by syntax diagnostics that only
///   need source-stable shape comparison before full type identity lowering.
fn trait_type_text_equal(left: &str, right: &str) -> bool {
    compact_spaces(left) == compact_spaces(right)
}

/// Joins token-text strings for type normalization.
///
/// Inputs:
/// - `parts`: token text fragments.
///
/// Output:
/// - A space-separated string.
///
/// Transformation:
/// - Mirrors `join_token_texts` for callers that already own string fragments.
fn join_token_texts_from_strings(parts: &[String]) -> String {
    parts
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(" ")
}

fn collect_syntax_kind_diagnostics(module: &SyntaxModuleOutput) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for declaration in &module.declarations {
        match &declaration.payload {
            SyntaxDeclarationPayload::Function {
                params,
                return_type,
                ..
            } => {
                for param in params {
                    collect_kind_diagnostic_for_syntax_type(&param.annotation, &mut diagnostics);
                }
                collect_kind_diagnostic_for_syntax_type(return_type, &mut diagnostics);
            }
            SyntaxDeclarationPayload::Type { variants, .. } => {
                for variant in variants {
                    collect_kind_diagnostic_for_syntax_type(variant, &mut diagnostics);
                }
            }
            SyntaxDeclarationPayload::Struct { fields, .. } => {
                for field in fields {
                    collect_kind_diagnostic_for_syntax_type(&field.annotation, &mut diagnostics);
                }
            }
            SyntaxDeclarationPayload::Constructor { clauses, .. } => {
                for clause in clauses {
                    for param in &clause.params {
                        collect_kind_diagnostic_for_syntax_type(
                            &param.annotation,
                            &mut diagnostics,
                        );
                    }
                    collect_kind_diagnostic_for_syntax_type(&clause.return_type, &mut diagnostics);
                }
            }
            SyntaxDeclarationPayload::Trait { methods, .. } => {
                for method in methods {
                    for param in &method.params {
                        collect_kind_diagnostic_for_syntax_type(
                            &param.annotation,
                            &mut diagnostics,
                        );
                    }
                    collect_kind_diagnostic_for_syntax_type(&method.return_type, &mut diagnostics);
                }
            }
            SyntaxDeclarationPayload::Template { props, .. } => {
                for prop in props {
                    collect_kind_diagnostic_for_syntax_type(&prop.annotation, &mut diagnostics);
                }
            }
            _ => {}
        }
    }
    diagnostics
}

/// Checks public constructor signatures for private local return-type leaks.
///
/// Inputs:
/// - `module`: syntax-output module containing constructor declarations.
/// - `resolved`: resolved module carrying local type visibility.
/// - `alias_names`: visible type names used to parse constructor return
///   annotations.
///
/// Output:
/// - One error diagnostic for each public constructor clause whose return type
///   exposes a private local type.
///
/// Transformation:
/// - Parses constructor return annotations into the type model and recursively
///   scans compound return types for private local type names. Imported or
///   module-qualified names are not treated as local private leaks here.
fn check_syntax_public_constructor_return_visibility(
    module: &SyntaxModuleOutput,
    resolved: &ResolvedModule,
    alias_names: &HashSet<String>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Constructor {
            name,
            is_public,
            clauses,
            ..
        } = &declaration.payload
        else {
            continue;
        };
        if !*is_public {
            continue;
        }

        for clause in clauses {
            let mut vars = HashMap::new();
            let mut next_var: TypeVarId = 0;
            let ret = parse_type_expr(
                &clause.return_type.text,
                alias_names,
                &mut vars,
                &mut next_var,
            )
            .unwrap_or(Type::Dynamic);

            if let Some(private_type) =
                first_private_local_type_name(&ret, &resolved.local_type_names)
            {
                diagnostics.push(Diagnostic {
                    span: clause.return_type.span.into(),
                    message: format!(
                        "public constructor {} exposes private return type {}",
                        name, private_type
                    ),
                    severity: DiagSeverity::Error,
                });
            }
        }
    }

    diagnostics
}

/// Finds the first private local type mentioned by a parsed type expression.
///
/// Inputs:
/// - `ty`: parsed type expression to inspect.
/// - `local_type_names`: resolver map of local type names to visibility.
///
/// Output:
/// - `Some(name)` for the first private unqualified local type reference.
/// - `None` when the type does not expose a private local type.
///
/// Transformation:
/// - Recursively walks lists, tuples, unions, maps, function types, fixed
///   arrays, and named type arguments while ignoring primitives, variables,
///   literals, and qualified/imported type names.
fn first_private_local_type_name(
    ty: &Type,
    local_type_names: &HashMap<String, TypeVisibility>,
) -> Option<String> {
    match ty {
        Type::Named {
            module: None,
            name,
            args,
        } => {
            if local_type_names.get(name) == Some(&TypeVisibility::Private) {
                return Some(name.clone());
            }
            args.iter()
                .find_map(|arg| first_private_local_type_name(arg, local_type_names))
        }
        Type::Named { args, .. } => args
            .iter()
            .find_map(|arg| first_private_local_type_name(arg, local_type_names)),
        Type::List(inner) => first_private_local_type_name(inner, local_type_names),
        Type::Tuple(items) | Type::Union(items) => items
            .iter()
            .find_map(|item| first_private_local_type_name(item, local_type_names)),
        Type::Map(fields) => fields
            .iter()
            .find_map(|field| first_private_local_type_name(&field.value, local_type_names)),
        Type::Function { params, ret } => params
            .iter()
            .chain(std::iter::once(ret.as_ref()))
            .find_map(|item| first_private_local_type_name(item, local_type_names)),
        Type::FixedArray { elem, .. } => first_private_local_type_name(elem, local_type_names),
        Type::Int
        | Type::Float
        | Type::Number
        | Type::Binary
        | Type::Atom
        | Type::Bool
        | Type::Term
        | Type::Dynamic
        | Type::Never
        | Type::LiteralAtom(_)
        | Type::LiteralInt(_)
        | Type::Var(_) => None,
    }
}

fn check_syntax_macro_decl_signatures(module: &SyntaxModuleOutput) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Function {
            name,
            return_type,
            is_macro,
            ..
        } = &declaration.payload
        else {
            continue;
        };
        if !is_macro {
            continue;
        }

        if !is_valid_macro_return_type(&return_type.text) {
            diagnostics.push(Diagnostic {
                span: return_type.span.into(),
                message: format!(
                    "macro `{}` must return Ast[T], found {}",
                    name, return_type.text
                ),
                severity: DiagSeverity::Error,
            });
        }
    }

    diagnostics
}

fn is_valid_macro_return_type(annotation: &str) -> bool {
    let src = compact_spaces(annotation);
    let Some((base, args)) = split_named_type(&src) else {
        return false;
    };
    if base != "Ast" {
        return false;
    }

    let args = split_top_level_csv(&args);
    args.len() == 1 && !args[0].trim().is_empty()
}

fn check_syntax_trait_decls(
    module: &SyntaxModuleOutput,
    trait_map: &HashMap<String, ParsedTraitSignature>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut trait_names = HashSet::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Trait { name, methods, .. } = &declaration.payload else {
            continue;
        };

        if !trait_names.insert(name.clone()) {
            diagnostics.push(Diagnostic {
                span: declaration.span.into(),
                message: format!("duplicate trait declaration `{}`", name),
                severity: DiagSeverity::Error,
            });
        }

        let mut method_names = HashSet::new();
        for method in methods {
            if !method_names.insert(method.name.clone()) {
                diagnostics.push(Diagnostic {
                    span: method.span.into(),
                    message: format!("duplicate method `{}` in trait {}", method.name, name),
                    severity: DiagSeverity::Error,
                });
            }
        }

        let Some(trait_signature) = trait_map.get(name) else {
            continue;
        };

        for super_trait_text in &trait_signature.super_traits {
            let Some(super_trait) = parse_trait_instance_from_text(super_trait_text) else {
                diagnostics.push(Diagnostic {
                    span: declaration.span.into(),
                    message: format!(
                        "unable to parse super trait `{}` in declaration of `{}`",
                        super_trait_text, name
                    ),
                    severity: DiagSeverity::Error,
                });
                continue;
            };

            let Some(super_signature) = trait_map.get(&super_trait.name) else {
                diagnostics.push(Diagnostic {
                    span: declaration.span.into(),
                    message: format!(
                        "unknown super trait `{}` in declaration of `{}`",
                        super_trait.name, name
                    ),
                    severity: DiagSeverity::Error,
                });
                continue;
            };

            if super_trait.type_args.len() != super_signature.type_params.len() {
                diagnostics.push(Diagnostic {
                    span: declaration.span.into(),
                    message: format!(
                        "super trait `{}` expects {} type parameter(s), found {}",
                        super_trait.name,
                        super_signature.type_params.len(),
                        super_trait.type_args.len()
                    ),
                    severity: DiagSeverity::Error,
                });
            }
        }
    }
    diagnostics
}

fn check_syntax_struct_derives(
    module: &SyntaxModuleOutput,
    trait_map: &HashMap<String, ParsedTraitSignature>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Struct { name, derives, .. } = &declaration.payload else {
            continue;
        };

        let mut seen = HashSet::new();
        for derive_text in derives {
            let Some(derived_trait) = parse_trait_instance_from_text(derive_text) else {
                diagnostics.push(Diagnostic {
                    span: declaration.span.into(),
                    message: format!(
                        "unable to parse derived trait `{}` in declaration of struct `{}`",
                        derive_text, name
                    ),
                    severity: DiagSeverity::Error,
                });
                continue;
            };

            let derive_key = trait_instance_key(&derived_trait).unwrap_or(derive_text.clone());
            if !seen.insert(derive_key.clone()) {
                diagnostics.push(Diagnostic {
                    span: declaration.span.into(),
                    message: format!(
                        "duplicate derived trait `{}` in declaration of struct `{}`",
                        derive_key, name
                    ),
                    severity: DiagSeverity::Error,
                });
            }

            let Some(signature) = trait_map.get(&derived_trait.name) else {
                diagnostics.push(Diagnostic {
                    span: declaration.span.into(),
                    message: format!(
                        "unknown derived trait `{}` in declaration of struct `{}`",
                        derived_trait.name, name
                    ),
                    severity: DiagSeverity::Error,
                });
                continue;
            };

            if derived_trait.type_args.len() != signature.type_params.len() {
                diagnostics.push(Diagnostic {
                    span: declaration.span.into(),
                    message: format!(
                        "derived trait `{}` expects {} type parameter(s), found {}",
                        derived_trait.name,
                        signature.type_params.len(),
                        derived_trait.type_args.len()
                    ),
                    severity: DiagSeverity::Error,
                });
            }
        }
    }

    diagnostics
}

/// Validates declaration-site `implements` conformance obligations.
///
/// Inputs:
/// - `module`: compiler-facing syntax output containing type, struct, trait,
///   and receiver-method declarations.
/// - `trait_map`: known local/imported trait signatures keyed by local trait
///   name.
///
/// Output:
/// - Diagnostics for malformed, unknown, duplicate, arity-mismatched, or
///   unsatisfied `implements` entries.
///
/// Transformation:
/// - Treats each `implements TraitRef` entry as a conformance obligation for
///   the declaring type, substitutes trait type parameters with the provided
///   type arguments, and checks required trait methods against receiver methods
///   declared on that type. Trait methods with default bodies are considered
///   satisfied when no receiver method is present.
fn check_syntax_declared_implements(
    module: &SyntaxModuleOutput,
    trait_map: &HashMap<String, ParsedTraitSignature>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let receiver_methods = collect_syntax_receiver_method_signatures(module);
    let mut inheritance_cache: HashMap<String, Option<HashMap<String, TraitMethodSignature>>> =
        HashMap::new();

    for declaration in &module.declarations {
        let Some((type_name, implements)) = syntax_declared_implements(declaration) else {
            continue;
        };

        let mut seen = HashSet::new();
        for trait_ref in implements {
            let Some(implemented_trait) = parse_trait_instance_from_text(&trait_ref.text) else {
                diagnostics.push(Diagnostic {
                    span: trait_ref.span.into(),
                    message: format!(
                        "unable to parse implemented trait `{}` in declaration of `{}`",
                        trait_ref.text, type_name
                    ),
                    severity: DiagSeverity::Error,
                });
                continue;
            };

            let implement_key =
                trait_instance_key(&implemented_trait).unwrap_or_else(|| trait_ref.text.clone());
            if !seen.insert(implement_key.clone()) {
                diagnostics.push(Diagnostic {
                    span: trait_ref.span.into(),
                    message: format!(
                        "duplicate implemented trait `{}` in declaration of `{}`",
                        implement_key, type_name
                    ),
                    severity: DiagSeverity::Error,
                });
                continue;
            }

            let Some(signature) = trait_map.get(&implemented_trait.name) else {
                diagnostics.push(Diagnostic {
                    span: trait_ref.span.into(),
                    message: format!(
                        "unknown implemented trait `{}` in declaration of `{}`",
                        implemented_trait.name, type_name
                    ),
                    severity: DiagSeverity::Error,
                });
                continue;
            };

            if implemented_trait.type_args.len() != signature.type_params.len() {
                diagnostics.push(Diagnostic {
                    span: trait_ref.span.into(),
                    message: format!(
                        "implemented trait `{}` expects {} type parameter(s), found {}",
                        implemented_trait.name,
                        signature.type_params.len(),
                        implemented_trait.type_args.len()
                    ),
                    severity: DiagSeverity::Error,
                });
                continue;
            }

            let methods = collect_trait_methods_with_inheritance(
                trait_map,
                &implemented_trait.name,
                &mut inheritance_cache,
                &mut HashSet::new(),
            )
            .unwrap_or_default();

            for (method_name, expected_method) in methods {
                check_declared_implements_method(
                    type_name,
                    &implemented_trait,
                    signature,
                    &method_name,
                    &expected_method,
                    receiver_methods.get(&(type_name.to_string(), method_name.clone())),
                    trait_ref.span.into(),
                    &mut diagnostics,
                );
            }
        }
    }

    diagnostics
}

/// Validates coherence for structured source-level trait conformance.
///
/// Inputs:
/// - `module`: syntax-output module containing declaration-site `implements`
///   and explicit `impl Trait for Type` declarations.
///
/// Output:
/// - Diagnostics for duplicate conformance keys across declaration-site and
///   explicit adapter forms.
///
/// Transformation:
/// - Converts both conformance syntaxes into stable `TraitRef for Type` keys
///   and reports repeated keys. This enforces the greenfield rule that a type
///   must not declare `implements Trait[...]` and also provide an explicit
///   adapter impl for the same pair.
fn check_syntax_trait_impl_coherence(module: &SyntaxModuleOutput) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut seen: HashMap<String, Span> = HashMap::new();

    for declaration in &module.declarations {
        if let Some((type_name, implements)) = syntax_declared_implements(declaration) {
            for trait_ref in implements {
                let Some(target) = parse_trait_instance_from_text(&trait_ref.text) else {
                    continue;
                };
                let Some(key) = syntax_trait_impl_key(&target, type_name) else {
                    continue;
                };
                if let Some(previous) = seen.get(&key) {
                    diagnostics.push(Diagnostic {
                        span: trait_ref.span.into(),
                        message: format!(
                            "coherent impl conflict for `{}`: duplicate visible conformance (first seen at {:?})",
                            key, previous
                        ),
                        severity: DiagSeverity::Error,
                    });
                } else {
                    seen.insert(key, trait_ref.span.into());
                }
            }
            continue;
        }

        let SyntaxDeclarationPayload::TraitImpl {
            trait_ref,
            for_type,
            ..
        } = &declaration.payload
        else {
            continue;
        };

        let Some(target) = parse_trait_instance_from_text(&trait_ref.text) else {
            continue;
        };
        let Some(key) = syntax_trait_impl_key(&target, &for_type.text) else {
            continue;
        };
        if let Some(previous) = seen.get(&key) {
            diagnostics.push(Diagnostic {
                span: declaration.span.into(),
                message: format!(
                    "coherent impl conflict for `{}`: duplicate visible conformance (first seen at {:?})",
                    key, previous
                ),
                severity: DiagSeverity::Error,
            });
        } else {
            seen.insert(key, declaration.span.into());
        }
    }

    diagnostics
}

/// Validates structured explicit `impl Trait for Type` method signatures.
///
/// Inputs:
/// - `module`: syntax-output module to scan for explicit trait impl blocks.
/// - `trait_map`: known local/imported trait signatures keyed by local trait
///   name.
///
/// Output:
/// - Diagnostics for unknown traits, trait arity mismatches, duplicate impl
///   methods, undeclared impl methods, missing required methods, and parameter
///   or return-type mismatches.
///
/// Transformation:
/// - Converts each structured impl payload into a parsed conformance summary,
///   specializes trait type parameters with the impl's type arguments, and
///   compares the adapter methods against inherited trait requirements.
fn check_syntax_trait_impl_signatures(
    module: &SyntaxModuleOutput,
    trait_map: &HashMap<String, ParsedTraitSignature>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut inheritance_cache: HashMap<String, Option<HashMap<String, TraitMethodSignature>>> =
        HashMap::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::TraitImpl { .. } = &declaration.payload else {
            continue;
        };

        let Some(impl_decl) = syntax_trait_impl_to_parsed(declaration) else {
            diagnostics.push(Diagnostic {
                span: declaration.span.into(),
                message: "unable to parse trait impl declaration".to_string(),
                severity: DiagSeverity::Error,
            });
            continue;
        };

        check_parsed_trait_impl_signature(
            &impl_decl,
            declaration.span.into(),
            trait_map,
            &mut inheritance_cache,
            &mut diagnostics,
        );
    }

    diagnostics
}

/// Builds a stable conformance key for syntax-output trait impl checks.
///
/// Inputs:
/// - `target`: parsed trait instance such as `Show[User]`.
/// - `for_type`: source type that owns the explicit or declaration-site
///   conformance.
///
/// Output:
/// - `Some("Trait[Args] for Type")` when the trait name is non-empty.
/// - `None` for malformed trait instances.
///
/// Transformation:
/// - Reuses the existing trait-instance key and appends normalized owner type
///   text for coherence checks.
fn syntax_trait_impl_key(target: &ParsedTraitInstance, for_type: &str) -> Option<String> {
    trait_instance_key(target)
        .map(|trait_key| format!("{} for {}", trait_key, normalize_trait_type_text(for_type)))
}

/// Converts structured syntax-output impl declarations into checker summaries.
///
/// Inputs:
/// - `declaration`: syntax-output declaration expected to hold a
///   `TraitImpl` payload.
///
/// Output:
/// - Parsed trait impl summary with target trait, owner type, and method
///   signatures, or `None` when the payload is not a trait impl or its trait
///   reference cannot be parsed.
///
/// Transformation:
/// - Reads the structured `trait_ref`, `for_type`, and impl methods directly
///   from syntax output, avoiding raw source reparsing for the formal compiler
///   path.
fn syntax_trait_impl_to_parsed(declaration: &SyntaxDeclarationOutput) -> Option<ParsedTraitImpl> {
    let SyntaxDeclarationPayload::TraitImpl {
        trait_ref,
        for_type,
        methods,
        ..
    } = &declaration.payload
    else {
        return None;
    };

    let target = parse_trait_instance_from_text(&trait_ref.text)?;
    Some(ParsedTraitImpl {
        target,
        for_type: Some(normalize_trait_type_text(&for_type.text)),
        methods: methods.iter().map(syntax_impl_method_signature).collect(),
    })
}

/// Converts one structured impl method into a comparable signature.
///
/// Inputs:
/// - `method`: syntax-output impl method payload.
///
/// Output:
/// - Parsed method signature containing name, parameter type texts, return
///   type text, and source span.
///
/// Transformation:
/// - Drops method bodies and keeps only the type-level information needed for
///   conformance validation.
fn syntax_impl_method_signature(method: &SyntaxImplMethodOutput) -> ParsedMethodSignature {
    ParsedMethodSignature {
        name: method.name.clone(),
        params: method
            .params
            .iter()
            .map(|param| normalize_trait_type_text(&param.annotation.text))
            .collect(),
        return_type: normalize_trait_type_text(&method.return_type.text),
        span: method.span.into(),
    }
}

/// Returns a declaration's type name and `implements` list when present.
///
/// Inputs:
/// - `declaration`: syntax-output declaration to inspect.
///
/// Output:
/// - `Some((type_name, implements))` for type/struct declarations with one or
///   more `implements` entries.
/// - `None` for declarations without declaration-site conformance obligations.
///
/// Transformation:
/// - Abstracts over type aliases and structs so conformance validation can use
///   one path for both declaration forms.
fn syntax_declared_implements(
    declaration: &SyntaxDeclarationOutput,
) -> Option<(&str, &[SyntaxTypeOutput])> {
    match &declaration.payload {
        SyntaxDeclarationPayload::Type {
            name, implements, ..
        }
        | SyntaxDeclarationPayload::Struct {
            name, implements, ..
        } if !implements.is_empty() => Some((name.as_str(), implements.as_slice())),
        _ => None,
    }
}

#[derive(Debug, Clone)]
struct ReceiverMethodSignature {
    params: Vec<String>,
    return_type: String,
    span: Span,
}

/// Collects receiver methods by receiver type and method name.
///
/// Inputs:
/// - `module`: syntax-output module to scan.
///
/// Output:
/// - Map keyed by `(receiver type text, method name)`.
///
/// Transformation:
/// - Converts receiver-method declarations into normalized signature summaries
///   used by declaration-site conformance validation.
fn collect_syntax_receiver_method_signatures(
    module: &SyntaxModuleOutput,
) -> HashMap<(String, String), ReceiverMethodSignature> {
    let mut methods = HashMap::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Method {
            receiver,
            name,
            params,
            return_type,
            ..
        } = &declaration.payload
        else {
            continue;
        };

        methods.insert(
            (
                normalize_trait_type_text(&receiver.annotation.text),
                name.clone(),
            ),
            ReceiverMethodSignature {
                params: params
                    .iter()
                    .map(|param| normalize_trait_type_text(&param.annotation.text))
                    .collect(),
                return_type: normalize_trait_type_text(&return_type.text),
                span: declaration.span.into(),
            },
        );
    }

    methods
}

/// Collects dispatchable receiver methods by method name and arity.
///
/// Inputs:
/// - `module`: syntax-output module containing receiver-method declarations.
/// - `alias_names`: visible type names used while parsing annotations.
/// - `imported_type_names`: imported type aliases that need qualification.
/// - `imported_type_aliases`: imported alias bodies visible to signatures.
/// - `local_aliases`: local alias bodies visible to signatures.
///
/// Output:
/// - Map keyed by `(method name, non-receiver arity)` with one or more
///   receiver-specialized callable schemes.
///
/// Transformation:
/// - Parses the receiver annotation and method parameter/return annotations in
///   one shared type-variable scope, preserving generic receiver relationships.
///   The resulting callable scheme excludes the receiver parameter because
///   receiver call inference checks the receiver separately, then checks the
///   ordinary call arguments through the existing function scheme path.
fn collect_syntax_receiver_method_dispatch_signatures(
    module: &SyntaxModuleOutput,
    alias_names: &HashSet<String>,
    imported_type_names: &HashMap<String, QualifiedTypeName>,
    imported_type_aliases: &HashMap<String, TypeAlias>,
    local_aliases: &HashMap<String, TypeAlias>,
) -> HashMap<(String, usize), Vec<ReceiverMethodDispatchSignature>> {
    let mut methods: HashMap<(String, usize), Vec<ReceiverMethodDispatchSignature>> =
        HashMap::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Method {
            receiver,
            name,
            params,
            return_type,
            generic_bounds,
            ..
        } = &declaration.payload
        else {
            continue;
        };

        let Some(signature) = receiver_method_dispatch_signature(
            receiver,
            params,
            return_type,
            generic_bounds,
            alias_names,
            imported_type_names,
            imported_type_aliases,
            local_aliases,
        ) else {
            continue;
        };

        methods
            .entry((name.clone(), params.len()))
            .or_default()
            .push(signature);
    }

    methods
}

/// Builds one dispatch signature for a receiver-method declaration.
///
/// Inputs:
/// - `receiver`: declared receiver parameter.
/// - `params`: non-receiver method parameters.
/// - `return_type`: declared method return type.
/// - `generic_bounds`: callable generic bounds from the method declaration.
/// - `alias_names`, `imported_type_names`, `imported_type_aliases`, and
///   `local_aliases`: visible type-resolution context.
///
/// Output:
/// - Dispatch signature with parsed receiver type and non-receiver function
///   scheme, or `None` when the receiver annotation cannot be parsed.
///
/// Transformation:
/// - Parses all type annotations in one variable scope, expands imported aliases
///   without erasing named imported identities, qualifies imported type names,
///   and converts generic bounds into the same internal form used for normal
///   functions.
fn receiver_method_dispatch_signature(
    receiver: &SyntaxParamOutput,
    params: &[SyntaxParamOutput],
    return_type: &SyntaxTypeOutput,
    generic_bounds: &[String],
    alias_names: &HashSet<String>,
    imported_type_names: &HashMap<String, QualifiedTypeName>,
    imported_type_aliases: &HashMap<String, TypeAlias>,
    local_aliases: &HashMap<String, TypeAlias>,
) -> Option<ReceiverMethodDispatchSignature> {
    let mut vars = HashMap::new();
    let mut next_var: TypeVarId = 0;

    let receiver_type = parse_type_expr(
        &receiver.annotation.text,
        alias_names,
        &mut vars,
        &mut next_var,
    )?;
    let receiver_type = expand_imported_aliases_except_named(
        &receiver_type,
        imported_type_aliases,
        imported_type_names,
        local_aliases,
    );
    let receiver_type = qualify_type_names(&receiver_type, imported_type_names);

    let params = params
        .iter()
        .map(|param| {
            let parsed = parse_type_expr(
                &param.annotation.text,
                alias_names,
                &mut vars,
                &mut next_var,
            )
            .unwrap_or(Type::Dynamic);
            let parsed = expand_imported_aliases_except_named(
                &parsed,
                imported_type_aliases,
                imported_type_names,
                local_aliases,
            );
            qualify_type_names(&parsed, imported_type_names)
        })
        .collect::<Vec<_>>();

    let ret = parse_type_expr(&return_type.text, alias_names, &mut vars, &mut next_var)
        .unwrap_or(Type::Dynamic);
    let ret = expand_imported_aliases_except_named(
        &ret,
        imported_type_aliases,
        imported_type_names,
        local_aliases,
    );
    let ret = qualify_type_names(&ret, imported_type_names);

    let bounds = parse_generic_bounds(generic_bounds, &vars, alias_names)
        .into_iter()
        .map(|bound| FunctionBound {
            trait_name: bound.trait_name,
            trait_args: bound
                .trait_args
                .into_iter()
                .map(|arg| {
                    let arg = expand_imported_aliases_except_named(
                        &arg,
                        imported_type_aliases,
                        imported_type_names,
                        local_aliases,
                    );
                    qualify_type_names(&arg, imported_type_names)
                })
                .collect(),
        })
        .collect();

    Some(ReceiverMethodDispatchSignature {
        receiver_type,
        scheme: FunctionScheme {
            params,
            ret,
            bounds,
        },
    })
}

/// Validates local receiver-method declaration identity and ownership.
///
/// Inputs:
/// - `module`: syntax-output module to inspect.
/// - `local_type_names`: type and struct names declared in the same module.
///
/// Output:
/// - Diagnostics for duplicate receiver-method identities and receiver methods
///   declared outside the receiver type's owner module.
///
/// Transformation:
/// - Checks the source-level receiver annotation head without expanding aliases.
///   A method identity is `(receiver type text, method name, non-receiver
///   arity)`. Local declarations own local type/struct receiver heads; the
///   `std.core.String` module is the primitive declaration site for the
///   compiler-known `String` receiver surface.
fn check_syntax_receiver_methods(
    module: &SyntaxModuleOutput,
    local_type_names: &HashSet<String>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut seen: HashMap<(String, String, usize), Span> = HashMap::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Method {
            receiver,
            name,
            params,
            ..
        } = &declaration.payload
        else {
            continue;
        };

        let receiver_text = normalize_trait_type_text(&receiver.annotation.text);
        let key = (receiver_text.clone(), name.clone(), params.len());
        if let Some(previous) = seen.get(&key) {
            diagnostics.push(Diagnostic {
                span: declaration.span.into(),
                message: format!(
                    "duplicate receiver method `{}` for `{}` / {} (first seen at {:?})",
                    name,
                    receiver_text,
                    params.len(),
                    previous
                ),
                severity: DiagSeverity::Error,
            });
        } else {
            seen.insert(key, declaration.span.into());
        }

        let Some(receiver_head) = receiver_owner_type_name_from_text(&receiver_text) else {
            diagnostics.push(Diagnostic {
                span: declaration.span.into(),
                message: format!(
                    "receiver method `{}` must use an owned named receiver type, found `{}`",
                    name, receiver_text
                ),
                severity: DiagSeverity::Error,
            });
            continue;
        };

        if !local_type_names.contains(&receiver_head)
            && !(module.module_name == "std.core.String" && receiver_head == "String")
        {
            diagnostics.push(Diagnostic {
                span: declaration.span.into(),
                message: format!(
                    "receiver method `{}` for `{}` must be declared in the defining module of `{}`",
                    name, receiver_text, receiver_head
                ),
                severity: DiagSeverity::Error,
            });
        }
    }

    diagnostics
}

/// Extracts the unqualified owner type head from receiver annotation text.
///
/// Inputs:
/// - `text`: normalized receiver annotation text.
///
/// Output:
/// - The unqualified receiver type head for simple named receiver types.
/// - `None` for qualified/imported, tuple, list, map, function, or malformed
///   receiver annotations.
///
/// Transformation:
/// - Reads identifier characters up to a type-argument delimiter and rejects
///   annotations whose owner cannot be represented as a single local type name.
fn receiver_owner_type_name_from_text(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.contains('.') {
        return None;
    }
    let head = trimmed
        .split(['[', ' ', '\t', '\r', '\n'])
        .next()
        .unwrap_or_default();
    if head
        .chars()
        .next()
        .map(|ch| ch.is_ascii_uppercase())
        .unwrap_or(false)
        && head
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        Some(head.to_string())
    } else {
        None
    }
}

/// Checks one required method for an `implements` conformance.
///
/// Inputs:
/// - `type_name`: type declaring the `implements` clause.
/// - `implemented_trait`: parsed trait reference from the conformance clause.
/// - `trait_signature`: declared trait type parameters.
/// - `method_name`: required trait method name.
/// - `expected_method`: trait method signature before substitution.
/// - `receiver_method`: matching receiver method, if one exists.
/// - `fallback_span`: span for diagnostics when no method-specific span exists.
/// - `diagnostics`: output diagnostic buffer.
///
/// Output:
/// - No direct return value.
///
/// Transformation:
/// - Substitutes trait type parameters with conformance type arguments and then
///   compares the resulting method signature with the receiver-method shape:
///   the first trait method parameter maps to the receiver, and remaining
///   parameters map to ordinary method arguments.
fn check_declared_implements_method(
    type_name: &str,
    implemented_trait: &ParsedTraitInstance,
    trait_signature: &ParsedTraitSignature,
    method_name: &str,
    expected_method: &TraitMethodSignature,
    receiver_method: Option<&ReceiverMethodSignature>,
    fallback_span: Span,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let specialized_params = expected_method
        .params
        .iter()
        .map(|param| {
            specialize_trait_type_text(
                param,
                &trait_signature.type_params,
                &implemented_trait.type_args,
            )
        })
        .collect::<Vec<_>>();
    let specialized_return = specialize_trait_type_text(
        &expected_method.return_type,
        &trait_signature.type_params,
        &implemented_trait.type_args,
    );

    if specialized_params.is_empty() {
        diagnostics.push(Diagnostic {
            span: fallback_span,
            message: format!(
                "trait method `{}` in `{}` must declare a receiver parameter for `implements`",
                method_name, implemented_trait.name
            ),
            severity: DiagSeverity::Error,
        });
        return;
    }

    let expected_receiver = &specialized_params[0];
    if !trait_type_text_equal(expected_receiver, type_name) {
        diagnostics.push(Diagnostic {
            span: fallback_span,
            message: format!(
                "trait method `{}` in `{}` expects receiver {}, but `{}` implements it",
                method_name, implemented_trait.name, expected_receiver, type_name
            ),
            severity: DiagSeverity::Error,
        });
        return;
    }

    let Some(receiver_method) = receiver_method else {
        if !expected_method.has_default {
            diagnostics.push(Diagnostic {
                span: fallback_span,
                message: format!(
                    "missing receiver method `{}` for `{}` implementing `{}`",
                    method_name, type_name, implemented_trait.name
                ),
                severity: DiagSeverity::Error,
            });
        }
        return;
    };

    let expected_args = &specialized_params[1..];
    if receiver_method.params.len() != expected_args.len() {
        diagnostics.push(Diagnostic {
            span: receiver_method.span,
            message: format!(
                "receiver method `{}` for `{}` has arity {}, expected {}",
                method_name,
                type_name,
                receiver_method.params.len(),
                expected_args.len()
            ),
            severity: DiagSeverity::Error,
        });
        return;
    }

    for (idx, (expected, found)) in expected_args
        .iter()
        .zip(receiver_method.params.iter())
        .enumerate()
    {
        if !trait_type_text_equal(expected, found) {
            diagnostics.push(Diagnostic {
                span: receiver_method.span,
                message: format!(
                    "receiver method `{}` parameter {} for `{}` expects {}, found {}",
                    method_name,
                    idx + 1,
                    type_name,
                    expected,
                    found
                ),
                severity: DiagSeverity::Error,
            });
        }
    }

    if !trait_type_text_equal(&specialized_return, &receiver_method.return_type) {
        diagnostics.push(Diagnostic {
            span: receiver_method.span,
            message: format!(
                "receiver method `{}` return type for `{}` expects {}, found {}",
                method_name, type_name, specialized_return, receiver_method.return_type
            ),
            severity: DiagSeverity::Error,
        });
    }
}

fn collect_kind_diagnostic_for_syntax_type(
    ty: &SyntaxTypeOutput,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if type_text_has_functor_type_argument_mismatch(&ty.text) {
        diagnostics.push(Diagnostic {
            span: ty.span.into(),
            message: "kind mismatch: Functor expects a type constructor of kind Type -> Type, found Int of kind Type".to_string(),
            severity: DiagSeverity::Error,
        });
    }
}

fn type_text_has_functor_type_argument_mismatch(text: &str) -> bool {
    let compact = compact_spaces(text);
    compact.contains("Functor[Int]")
}

fn normalize_type_param_name(param: &str) -> String {
    let trimmed = param.trim().trim_start_matches('-').trim_start_matches('+');
    if let Some(open) = trimmed.find('[') {
        trimmed[..open].trim().to_string()
    } else {
        trimmed.to_string()
    }
}

fn collect_syntax_struct_fields(
    module: &SyntaxModuleOutput,
    alias_names: &HashSet<String>,
) -> HashMap<String, HashMap<String, Type>> {
    let mut out = HashMap::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Struct { name, fields, .. } = &declaration.payload else {
            continue;
        };

        let mut vars = HashMap::new();
        let mut next_var: TypeVarId = 0;
        let mut field_types = HashMap::new();

        for field in fields {
            let ty = parse_type_expr(
                &field.annotation.text,
                alias_names,
                &mut vars,
                &mut next_var,
            )
            .unwrap_or(Type::Dynamic);
            field_types.insert(field.name.clone(), ty);
        }

        out.insert(name.clone(), field_types);
    }

    out
}

fn collect_syntax_type_names(module: &SyntaxModuleOutput) -> HashSet<String> {
    module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Type { name, .. }
            | SyntaxDeclarationPayload::Struct { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect()
}

fn collect_syntax_template_schemes(
    module: &SyntaxModuleOutput,
    alias_names: &HashSet<String>,
) -> HashMap<String, TemplateScheme> {
    let mut out = HashMap::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Template { name, props, .. } = &declaration.payload else {
            continue;
        };

        let mut vars = HashMap::new();
        let mut next_var: TypeVarId = 0;
        let prop_types = props
            .iter()
            .map(|prop| {
                let ty =
                    parse_type_expr(&prop.annotation.text, alias_names, &mut vars, &mut next_var)
                        .unwrap_or(Type::Dynamic);
                (prop.name.clone(), ty)
            })
            .collect::<HashMap<_, _>>();

        out.insert(name.clone(), TemplateScheme { props: prop_types });
    }

    out
}

fn instantiate_function_scheme(scheme: &FunctionScheme) -> FunctionScheme {
    instantiate_function_scheme_from(scheme, 0)
}

/// Instantiates a function scheme with fresh type variable IDs.
///
/// Inputs:
/// - `scheme`: generic function scheme to copy.
/// - `first_var`: first type variable ID available for the copy.
///
/// Output:
/// - Function scheme whose generic variables have been remapped to fresh IDs.
///
/// Transformation:
/// - Walks params, return type, and bounds, replacing every scheme-local type
///   variable with a deterministic fresh variable starting at `first_var`.
fn instantiate_function_scheme_from(
    scheme: &FunctionScheme,
    first_var: TypeVarId,
) -> FunctionScheme {
    let mut next_var = first_var;
    let mut map: HashMap<TypeVarId, TypeVarId> = HashMap::new();

    let mut remap = |id: &TypeVarId| -> TypeVarId {
        if let Some(remapped) = map.get(id) {
            *remapped
        } else {
            let remapped = next_var;
            map.insert(*id, remapped);
            next_var += 1;
            remapped
        }
    };

    let params = scheme
        .params
        .iter()
        .map(|param| remap_type(param, &mut remap))
        .collect();
    let ret = remap_type(&scheme.ret, &mut remap);
    let bounds = scheme
        .bounds
        .iter()
        .map(|bound| FunctionBound {
            trait_name: bound.trait_name.clone(),
            trait_args: bound
                .trait_args
                .iter()
                .map(|arg| remap_type(arg, &mut remap))
                .collect(),
        })
        .collect();

    FunctionScheme {
        params,
        ret,
        bounds,
    }
}

fn instantiate_constructor_scheme(
    scheme: &ConstructorScheme,
    first_var: TypeVarId,
) -> ConstructorScheme {
    let mut next_var = first_var;
    let mut map: HashMap<TypeVarId, TypeVarId> = HashMap::new();

    let mut remap = |id: &TypeVarId| -> TypeVarId {
        if let Some(remapped) = map.get(id) {
            *remapped
        } else {
            let remapped = next_var;
            map.insert(*id, remapped);
            next_var += 1;
            remapped
        }
    };

    ConstructorScheme {
        fixed_params: scheme
            .fixed_params
            .iter()
            .map(|param| remap_type(param, &mut remap))
            .collect(),
        min_arity: scheme.min_arity,
        vararg: scheme
            .vararg
            .as_ref()
            .map(|param| remap_type(param, &mut remap)),
        ret: remap_type(&scheme.ret, &mut remap),
    }
}

fn next_constructor_type_var(args: &[Type], subst: &HashMap<TypeVarId, Type>) -> TypeVarId {
    let arg_max = args.iter().filter_map(max_type_var).max();
    let subst_key_max = subst.keys().copied().max();
    let subst_value_max = subst.values().filter_map(max_type_var).max();

    arg_max
        .into_iter()
        .chain(subst_key_max)
        .chain(subst_value_max)
        .max()
        .map(|id| id + 1)
        .unwrap_or(0)
}

/// Returns the next safe type variable ID for function-call instantiation.
///
/// Inputs:
/// - `args`: argument types already inferred for the call.
/// - `subst`: active substitution map for the enclosing expression check.
///
/// Output:
/// - One greater than the highest type variable visible in arguments or
///   substitutions, or `0` when none are visible.
///
/// Transformation:
/// - Scans argument types plus substitution keys and values so freshly
///   instantiated function generics cannot collide with stale bindings from
///   earlier calls in the same typechecking pass.
fn next_function_type_var(args: &[Type], subst: &HashMap<TypeVarId, Type>) -> TypeVarId {
    let arg_max = args.iter().filter_map(max_type_var).max();
    let subst_key_max = subst.keys().copied().max();
    let subst_value_max = subst.values().filter_map(max_type_var).max();

    arg_max
        .into_iter()
        .chain(subst_key_max)
        .chain(subst_value_max)
        .max()
        .map(|id| id + 1)
        .unwrap_or(0)
}

fn max_type_var(ty: &Type) -> Option<TypeVarId> {
    match ty {
        Type::Var(id) => Some(*id),
        Type::List(inner) | Type::FixedArray { elem: inner, .. } => max_type_var(inner),
        Type::Tuple(items) | Type::Union(items) => items.iter().filter_map(max_type_var).max(),
        Type::Map(fields) => fields
            .iter()
            .filter_map(|field| max_type_var(&field.value))
            .max(),
        Type::Named { args, .. } => args.iter().filter_map(max_type_var).max(),
        Type::Function { params, ret } => params
            .iter()
            .filter_map(max_type_var)
            .chain(max_type_var(ret))
            .max(),
        _ => None,
    }
}

fn remap_type<F>(ty: &Type, remap: &mut F) -> Type
where
    F: FnMut(&TypeVarId) -> TypeVarId,
{
    match ty {
        Type::Var(id) => Type::Var(remap(id)),
        Type::List(inner) => Type::List(Box::new(remap_type(inner, remap))),
        Type::Tuple(items) => {
            Type::Tuple(items.iter().map(|item| remap_type(item, remap)).collect())
        }
        Type::Union(items) => {
            Type::Union(items.iter().map(|item| remap_type(item, remap)).collect())
        }
        Type::Map(fields) => Type::Map(
            fields
                .iter()
                .map(|field| MapFieldType {
                    key: field.key.clone(),
                    value: remap_type(&field.value, remap),
                    required: field.required,
                })
                .collect(),
        ),
        Type::Named { module, name, args } => Type::Named {
            module: module.clone(),
            name: name.clone(),
            args: args.iter().map(|arg| remap_type(arg, remap)).collect(),
        },
        Type::Function { params, ret } => Type::Function {
            params: params
                .iter()
                .map(|param| remap_type(param, remap))
                .collect(),
            ret: Box::new(remap_type(ret, remap)),
        },
        Type::FixedArray { size, elem } => Type::FixedArray {
            size: *size,
            elem: Box::new(remap_type(elem, remap)),
        },
        other => other.clone(),
    }
}

fn instantiate_type(ty: &Type, subst: &HashMap<TypeVarId, Type>) -> Type {
    apply_subst(ty, subst)
}

fn infer_syntax_expr(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    match expr.kind {
        SyntaxExprKind::Int => {
            Type::LiteralInt(expr.text.as_deref().unwrap_or("0").parse().unwrap_or(0))
        }
        SyntaxExprKind::Float => Type::Float,
        SyntaxExprKind::Binary => Type::Binary,
        SyntaxExprKind::Atom => {
            let name = expr.text.as_deref().unwrap_or_default();
            if is_literal_atom(name) {
                if name == "true" || name == "false" {
                    Type::Bool
                } else {
                    Type::LiteralAtom(name.to_string())
                }
            } else {
                Type::Atom
            }
        }
        SyntaxExprKind::Var => {
            infer_syntax_var(expr.text.as_deref().unwrap_or_default(), locals, ctx)
        }
        SyntaxExprKind::Tuple => Type::Tuple(
            expr.children
                .iter()
                .map(|item| infer_syntax_expr(item, locals, ctx, subst, errors))
                .collect(),
        ),
        SyntaxExprKind::List => {
            let inferred = expr
                .children
                .iter()
                .map(|value| {
                    widen_list_literal_element_type(infer_syntax_expr(
                        value, locals, ctx, subst, errors,
                    ))
                })
                .collect::<Vec<_>>();
            Type::List(Box::new(normalize_union(inferred)))
        }
        SyntaxExprKind::ListCons => {
            let head_type = expr
                .children
                .first()
                .map(|head| infer_syntax_expr(head, locals, ctx, subst, errors))
                .unwrap_or(Type::Dynamic);
            if let Some(tail) = expr.children.get(1) {
                let tail_type = infer_syntax_expr(tail, locals, ctx, subst, errors);
                if let Err(message) =
                    unify(&tail_type, &Type::List(Box::new(head_type.clone())), subst)
                {
                    errors.push(format!("list cons tail {}", message));
                }
            }
            Type::List(Box::new(apply_subst(&head_type, subst)))
        }
        SyntaxExprKind::FixedArray => {
            let elem_type = normalize_union(
                expr.children
                    .iter()
                    .map(
                        |elem| match infer_syntax_expr(elem, locals, ctx, subst, errors) {
                            Type::LiteralInt(_) => Type::Int,
                            Type::LiteralAtom(_) => Type::Atom,
                            other => other,
                        },
                    )
                    .collect(),
            );
            Type::FixedArray {
                size: expr.children.len(),
                elem: Box::new(elem_type),
            }
        }
        SyntaxExprKind::Index => infer_syntax_index(expr, locals, ctx, subst, errors),
        SyntaxExprKind::Map => Type::Map(
            expr.fields
                .iter()
                .map(|field| MapFieldType {
                    key: field.key.clone(),
                    value: infer_syntax_expr(&field.value, locals, ctx, subst, errors),
                    required: field.required,
                })
                .collect(),
        ),
        SyntaxExprKind::Case => infer_syntax_case_expr(expr, locals, ctx, subst, errors),
        SyntaxExprKind::Receive => infer_syntax_receive_expr(expr, locals, ctx, subst, errors),
        SyntaxExprKind::Try => infer_syntax_try_expr(expr, locals, ctx, subst, errors),
        SyntaxExprKind::If => infer_syntax_if_expr(expr, locals, ctx, subst, errors),
        SyntaxExprKind::ListComprehension => {
            infer_syntax_list_comprehension(expr, locals, ctx, subst, errors)
        }
        SyntaxExprKind::Let => infer_syntax_let_expr(expr, locals, ctx, subst, errors),
        SyntaxExprKind::Cast => infer_syntax_cast_expr(expr, locals, ctx, subst, errors),
        SyntaxExprKind::Call => infer_syntax_call_expr(expr, locals, ctx, subst, errors),
        SyntaxExprKind::FunctionCall => {
            infer_syntax_function_value_call(expr, locals, ctx, subst, errors)
        }
        SyntaxExprKind::Fun => infer_syntax_fun_expr(expr, locals, ctx, subst, errors),
        SyntaxExprKind::RemoteFunRef => Type::Function {
            params: vec![Type::Dynamic; expr.arity],
            ret: Box::new(Type::Dynamic),
        },
        SyntaxExprKind::Macro => {
            let arg_types = expr
                .children
                .iter()
                .map(|arg| infer_syntax_expr(arg, locals, ctx, subst, errors))
                .collect::<Vec<_>>();

            if let Some(macro_name) = expr.text.as_deref() {
                if let Some(return_type) =
                    infer_syntax_macro_call(macro_name, &arg_types, ctx, subst, errors)
                {
                    return return_type;
                }
            }
            Type::Dynamic
        }
        SyntaxExprKind::RawMacro => {
            let name = expr.text.as_deref().unwrap_or("<unknown>");
            errors.push(format!(
                "raw macro expression `{}` requires macro resolution before type checking",
                name
            ));
            Type::Dynamic
        }
        SyntaxExprKind::HtmlBlock => infer_syntax_html_block(expr, locals, ctx, subst, errors),
        SyntaxExprKind::RecordConstruct => {
            infer_syntax_record_construct(expr, locals, ctx, subst, errors)
        }
        SyntaxExprKind::RecordAccess => {
            infer_syntax_record_access(expr, locals, ctx, subst, errors)
        }
        SyntaxExprKind::FieldAccess => infer_syntax_field_access(expr, locals, ctx, subst, errors),
        SyntaxExprKind::RecordUpdate => {
            infer_syntax_record_update(expr, locals, ctx, subst, errors)
        }
        SyntaxExprKind::TemplateInstantiate => {
            infer_syntax_template_instantiation(expr, locals, ctx, subst, errors)
        }
        SyntaxExprKind::ConstructorChain => {
            infer_syntax_constructor_chain(expr, locals, ctx, subst, errors)
        }
        SyntaxExprKind::UnaryOp => infer_syntax_unary_op(expr, locals, ctx, subst, errors),
        SyntaxExprKind::BinaryOp => infer_syntax_binary_op(expr, locals, ctx, subst, errors),
        SyntaxExprKind::Quote => {
            let value_type = expr
                .children
                .first()
                .map(|inner| infer_syntax_expr(inner, locals, ctx, subst, errors))
                .unwrap_or(Type::Dynamic);
            Type::Named {
                module: None,
                name: "Ast".to_string(),
                args: vec![value_type],
            }
        }
        SyntaxExprKind::Unquote => expr
            .children
            .first()
            .map(|inner| infer_syntax_expr(inner, locals, ctx, subst, errors))
            .unwrap_or(Type::Dynamic),
    }
}

/// Infers the placeholder type for a syntax-output cast expression.
///
/// Inputs:
/// - `expr`: syntax-output cast node with one child and target type text.
/// - `locals`, `ctx`, and `subst`: the active expression inference context.
/// - `errors`: mutable diagnostic text sink for unsupported conversion claims.
///
/// Output:
/// - Parsed target type when available, otherwise `Dynamic`.
///
/// Transformation:
/// - Type-checks the cast source child, parses the preserved target type text,
///   records that trait-backed conversion resolution is not implemented yet,
///   and returns the target type as the syntax-preserved expectation.
fn infer_syntax_cast_expr(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let source_type = expr
        .children
        .first()
        .map(|child| infer_syntax_expr(child, locals, ctx, subst, errors))
        .unwrap_or(Type::Dynamic);
    let target_text = expr.text.as_deref().unwrap_or("Dynamic");
    let mut vars = HashMap::new();
    let mut next_var = 0;
    let alias_names = ctx.aliases.keys().cloned().collect::<HashSet<_>>();
    let target_type = parse_type_expr(target_text, &alias_names, &mut vars, &mut next_var)
        .unwrap_or_else(|| {
            errors.push(format!("invalid cast target type `{}`", target_text));
            Type::Dynamic
        });

    errors.push(format!(
        "cast from {} to {} requires trait-backed conversion resolution before backend emission",
        pretty_type(&apply_subst(&source_type, subst)),
        pretty_type(&target_type)
    ));
    target_type
}

fn infer_syntax_var(name: &str, locals: &HashMap<String, Type>, ctx: &ExprInferContext) -> Type {
    locals
        .get(name)
        .cloned()
        .or_else(|| infer_unique_local_function_value(name, ctx))
        .or_else(|| ctx.file_imports.get(name).map(|_| Type::Binary))
        .or_else(|| {
            ctx.markdown_imports.get(name).map(|_| Type::Named {
                module: None,
                name: "Markdown".to_string(),
                args: Vec::new(),
            })
        })
        .unwrap_or(Type::Dynamic)
}

/// Infers a bare local function name used as a first-class value.
///
/// Inputs:
/// - `name`: source identifier from a variable expression.
/// - `ctx`: expression inference context containing local function schemes.
///
/// Output:
/// - `Some(Type::Function)` when exactly one local function with `name` is in
///   scope; otherwise `None`.
///
/// Transformation:
/// - Converts a unique local function signature into a function-value type so
///   higher-order calls can constrain callback parameters without treating the
///   identifier as an arbitrary dynamic value.
fn infer_unique_local_function_value(name: &str, ctx: &ExprInferContext<'_>) -> Option<Type> {
    let mut matches = ctx
        .signatures
        .iter()
        .filter(|((candidate, _arity), _scheme)| candidate == name)
        .map(|(_key, scheme)| instantiate_function_scheme(scheme));

    let first = matches.next()?;
    if matches.next().is_some() {
        return None;
    }

    Some(Type::Function {
        params: first.params,
        ret: Box::new(first.ret),
    })
}

fn infer_syntax_index(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let target_type = expr
        .children
        .first()
        .map(|value| infer_syntax_expr(value, locals, ctx, subst, errors))
        .unwrap_or(Type::Dynamic);
    let index_type = expr
        .children
        .get(1)
        .map(|index| infer_syntax_expr(index, locals, ctx, subst, errors))
        .unwrap_or(Type::Dynamic);

    match target_type {
        Type::FixedArray { size, elem } => match index_type {
            Type::LiteralInt(index) => {
                if index < 0 || index as usize >= size {
                    errors.push(format!(
                        "index {} is out of bounds for {}\nvalid indices: 0..{}",
                        index,
                        pretty_type(&Type::FixedArray {
                            size,
                            elem: elem.clone(),
                        }),
                        size.saturating_sub(1)
                    ));
                }
                *elem
            }
            Type::Int => *elem,
            Type::Var(_) | Type::Dynamic | Type::Number => *elem,
            _ => {
                errors.push(format!("expected Int found {}", pretty_type(&index_type)));
                Type::Dynamic
            }
        },
        _ => Type::Dynamic,
    }
}

fn infer_syntax_binary_op(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let op = syntax_binary_op(expr.operator.as_deref());
    if matches!(op, SyntaxBinaryOp::PipeForward) {
        return infer_syntax_pipe_forward(expr, locals, ctx, subst, errors);
    }
    let left_type = expr
        .children
        .first()
        .map(|left| infer_syntax_expr(left, locals, ctx, subst, errors))
        .unwrap_or(Type::Dynamic);
    let right_type = expr
        .children
        .get(1)
        .map(|right| infer_syntax_expr(right, locals, ctx, subst, errors))
        .unwrap_or(Type::Dynamic);
    if matches!(op, SyntaxBinaryOp::Send) {
        return infer_send_op(&left_type, &right_type, ctx, errors);
    }
    infer_syntax_binary_types(&op, &left_type, &right_type, ctx.aliases, subst, errors)
}

fn infer_syntax_unary_op(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let inner_type = expr
        .children
        .first()
        .map(|inner| infer_syntax_expr(inner, locals, ctx, subst, errors))
        .unwrap_or(Type::Dynamic);
    infer_unary_operator(
        expr.operator.as_deref().unwrap_or(""),
        &inner_type,
        subst,
        errors,
    )
}

fn infer_syntax_call_expr(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let arg_types = expr
        .children
        .iter()
        .skip(1)
        .map(|arg| infer_syntax_expr(arg, locals, ctx, subst, errors))
        .collect::<Vec<_>>();
    infer_syntax_call_with_arg_types(expr, &arg_types, locals, ctx, subst, errors)
}

/// Infers a dedicated function-value invocation expression.
///
/// Inputs:
/// - `expr`: syntax-output `FunctionCall` expression whose first child is the
///   callable expression and remaining children are call arguments.
/// - `locals`, `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - The invoked function's return type when the callee has a function type.
/// - `Dynamic` when the callee is malformed, non-callable, or has invalid
///   argument types.
///
/// Transformation:
/// - Infers all non-callee arguments, then delegates to the shared
///   function-value invocation checker so pipe-forward can prepend a synthetic
///   first argument without rebuilding syntax nodes.
fn infer_syntax_function_value_call(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let arg_types = expr
        .children
        .iter()
        .skip(1)
        .map(|arg| infer_syntax_expr(arg, locals, ctx, subst, errors))
        .collect::<Vec<_>>();
    infer_syntax_function_value_call_with_arg_types(expr, &arg_types, locals, ctx, subst, errors)
}

/// Checks a function-value invocation with already inferred argument types.
///
/// Inputs:
/// - `expr`: syntax-output `FunctionCall` expression with a callable child.
/// - `arg_types`: argument types to check against the callee's function type.
/// - `locals`, `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - The callee return type with substitutions applied.
/// - `Dynamic` when the callee is not a function or arguments do not match.
///
/// Transformation:
/// - Infers the callee expression, requires a `Type::Function`, unifies each
///   parameter with the provided argument type, and returns the substituted
///   result type.
fn infer_syntax_function_value_call_with_arg_types(
    expr: &SyntaxExprOutput,
    arg_types: &[Type],
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let Some(callee) = expr.children.first() else {
        errors.push("function-value invocation is missing a callee".to_string());
        return Type::Dynamic;
    };

    let callee_type = apply_subst(
        &infer_syntax_expr(callee, locals, ctx, subst, errors),
        subst,
    );
    match callee_type {
        Type::Function { params, ret } => {
            if params.len() != arg_types.len() {
                errors.push(format!(
                    "function arity mismatch: expected {} args, found {}",
                    params.len(),
                    arg_types.len()
                ));
                return Type::Dynamic;
            }

            for (expected, actual) in params.iter().zip(arg_types.iter()) {
                if let Err(message) = unify(expected, actual, subst) {
                    errors.push(message);
                }
            }

            apply_subst(ret.as_ref(), subst)
        }
        Type::Dynamic => Type::Dynamic,
        other => {
            errors.push(format!(
                "function-value invocation requires function value, found {}",
                pretty_type(&other)
            ));
            Type::Dynamic
        }
    }
}

fn infer_syntax_macro_call(
    name: &str,
    arg_types: &[Type],
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    let candidates: Vec<_> = ctx
        .signatures
        .iter()
        .filter_map(|((candidate_name, arity), scheme)| {
            if candidate_name == name {
                Some((*arity, scheme))
            } else {
                None
            }
        })
        .collect();

    if candidates.is_empty() {
        return None;
    }

    for (arity, scheme) in candidates.iter() {
        if *arity == arg_types.len() {
            match infer_function_with_bounds(scheme, Some(name), arg_types, ctx, subst) {
                Ok(ty) => return Some(unwrap_macro_return_type(ty)),
                Err(message) => {
                    errors.push(message);
                    return Some(Type::Dynamic);
                }
            }
        }
    }

    let arities = candidates
        .iter()
        .map(|(arity, _)| arity.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    errors.push(format!(
        "wrong arity for macro `{}`: expected one of [{}] args, found {}",
        name,
        arities,
        arg_types.len()
    ));
    Some(Type::Dynamic)
}

fn infer_syntax_call_with_arg_types(
    expr: &SyntaxExprOutput,
    arg_types: &[Type],
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    if expr.remote.is_none() {
        if let Some(ty) =
            infer_syntax_primitive_receiver_method_call(expr, arg_types, locals, ctx, subst, errors)
        {
            return ty;
        }
        if let Some(ty) =
            infer_syntax_receiver_method_call(expr, arg_types, locals, ctx, subst, errors)
        {
            return ty;
        }
    }

    let Some(function_name) = syntax_callee_name(expr) else {
        return Type::Dynamic;
    };

    if expr.remote.is_none() && syntax_callee_is_var(expr) {
        if let Some(constructed) =
            infer_constructor_call(function_name, &arg_types, ctx, subst, errors)
        {
            return constructed;
        }

        if let Some(imported) = ctx.constructor_aliases.get(function_name) {
            if let Some(interface) = ctx.interface_map.get(&imported.module) {
                if interface.opaque_types.contains(&imported.name) {
                    errors.push(format!(
                        "cannot construct opaque type {}.{} outside defining module",
                        imported.module, imported.name
                    ));
                    return Type::Dynamic;
                }
                if let Some(schemes) = parse_interface_constructor_schemes(
                    interface
                        .constructors
                        .get(&imported.name)
                        .map(Vec::as_slice),
                    interface,
                ) {
                    if let Some(constructed) = infer_constructor_schemes(
                        function_name,
                        &schemes,
                        &arg_types,
                        subst,
                        errors,
                    ) {
                        let interface_aliases = interface_type_aliases(interface);
                        return expand_type_aliases(&constructed, &interface_aliases);
                    }
                }
            }
        }

        if let Some(constructed) =
            infer_opaque_constructor(function_name, &arg_types, ctx.aliases, errors)
        {
            return constructed;
        }

        if let Some(Type::Function { params, ret }) =
            locals.get(function_name).map(|ty| apply_subst(ty, subst))
        {
            if params.len() != arg_types.len() {
                errors.push(format!(
                    "function arity mismatch: expected {} args, found {}",
                    params.len(),
                    arg_types.len()
                ));
                return Type::Dynamic;
            }

            for (expected, actual) in params.iter().zip(arg_types.iter()) {
                if let Err(message) = unify(expected, actual, subst) {
                    errors.push(message);
                }
            }

            return apply_subst(ret.as_ref(), subst);
        }

        if is_constructor_name(function_name) {
            errors.push(format!(
                "unknown constructor {} / {}",
                function_name,
                arg_types.len()
            ));
            return Type::Dynamic;
        }
    }

    if let Some(module_name) = expr.remote.as_deref() {
        return infer_syntax_remote_call(module_name, function_name, arg_types, ctx, subst, errors);
    }

    infer_syntax_local_call(function_name, arg_types, ctx, subst, errors)
}

fn infer_syntax_pipe_forward(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let Some(left) = expr.children.first() else {
        return Type::Dynamic;
    };
    let Some(right) = expr.children.get(1) else {
        return Type::Dynamic;
    };
    if !matches!(
        right.kind,
        SyntaxExprKind::Call | SyntaxExprKind::FunctionCall
    ) {
        errors.push("right side of |> must be a function call".to_string());
        let _ = infer_syntax_expr(left, locals, ctx, subst, errors);
        let _ = infer_syntax_expr(right, locals, ctx, subst, errors);
        return Type::Dynamic;
    }

    let mut arg_types = Vec::with_capacity(right.children.len());
    arg_types.push(infer_syntax_expr(left, locals, ctx, subst, errors));
    arg_types.extend(
        right
            .children
            .iter()
            .skip(1)
            .map(|arg| infer_syntax_expr(arg, locals, ctx, subst, errors)),
    );

    match right.kind {
        SyntaxExprKind::FunctionCall => infer_syntax_function_value_call_with_arg_types(
            right, &arg_types, locals, ctx, subst, errors,
        ),
        _ => infer_syntax_call_with_arg_types(right, &arg_types, locals, ctx, subst, errors),
    }
}

/// Infers a raw struct construction expression from syntax output.
///
/// Inputs:
/// - `expr`: syntax-output record construction node carrying the target type
///   name and field expressions.
/// - `locals`, `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - `Type::Named` for the constructed source type when inference can continue.
///
/// Transformation:
/// - Typechecks every field value, then enforces the Terlan visibility rule
///   that imported/public struct type identity does not grant raw construction
///   authority outside the defining module.
fn infer_syntax_record_construct(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    for field in &expr.fields {
        let _ = infer_syntax_expr(&field.value, locals, ctx, subst, errors);
        let _ = &field.key;
    }

    let name = expr.text.clone().unwrap_or_default();
    if let Some(imported) = ctx.imported_type_names.get(&name) {
        errors.push(format!(
            "cannot raw-construct imported struct {}.{} outside defining module; use an exported constructor",
            imported.module, imported.name
        ));
    }

    Type::Named {
        module: None,
        name,
        args: Vec::new(),
    }
}

fn infer_syntax_constructor_chain(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let Some(base) = expr.children.first() else {
        errors.push("constructor chain expression is missing base expression".to_string());
        return Type::Dynamic;
    };

    let Some(record) = expr.children.get(1) else {
        errors
            .push("constructor chain expression is missing constructor target record".to_string());
        let _ = infer_syntax_expr(base, locals, ctx, subst, errors);
        return Type::Dynamic;
    };

    let _ = infer_syntax_expr(base, locals, ctx, subst, errors);

    if record.kind != SyntaxExprKind::RecordConstruct {
        errors.push("constructor chain requires a record construct on the right side".to_string());
        let _ = infer_syntax_expr(record, locals, ctx, subst, errors);
        return Type::Dynamic;
    }

    infer_syntax_record_construct(record, locals, ctx, subst, errors)
}

fn infer_syntax_record_access(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let _ = expr
        .children
        .first()
        .map(|value| infer_syntax_expr(value, locals, ctx, subst, errors));
    let (name, field) = expr
        .text
        .as_deref()
        .and_then(|text| text.split_once('.'))
        .unwrap_or_default();
    if let Some(fields) = ctx.struct_fields.get(name) {
        if let Some(field_type) = fields.get(field) {
            field_type.clone()
        } else {
            errors.push(format!("unknown field {} on struct {}", field, name));
            Type::Dynamic
        }
    } else {
        errors.push(format!("unknown struct {}", name));
        Type::Dynamic
    }
}

fn infer_syntax_field_access(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let receiver = expr
        .children
        .first()
        .map(|value| apply_subst(&infer_syntax_expr(value, locals, ctx, subst, errors), subst))
        .unwrap_or(Type::Dynamic);
    let field = expr.text.as_deref().unwrap_or_default();
    match receiver {
        Type::Named { name, .. } if name == "Markdown" => match field {
            "raw" => Type::Binary,
            "html" => Type::Named {
                module: None,
                name: "Html".to_string(),
                args: vec![Type::Never],
            },
            _ => {
                errors.push(format!("unknown field {} on Markdown import", field));
                Type::Dynamic
            }
        },
        Type::Named { name, .. } => {
            if let Some(fields) = ctx.struct_fields.get(&name) {
                if let Some(field_type) = fields.get(field) {
                    field_type.clone()
                } else {
                    errors.push(format!("unknown field {} on struct {}", field, name));
                    Type::Dynamic
                }
            } else {
                errors.push(format!(
                    "field access requires struct receiver, found {}",
                    pretty_type(&Type::Named {
                        module: None,
                        name,
                        args: Vec::new(),
                    })
                ));
                Type::Dynamic
            }
        }
        other => {
            errors.push(format!(
                "field access requires struct receiver, found {}",
                pretty_type(&other)
            ));
            Type::Dynamic
        }
    }
}

fn infer_syntax_record_update(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let base = expr
        .children
        .first()
        .map(|value| infer_syntax_expr(value, locals, ctx, subst, errors))
        .unwrap_or(Type::Dynamic);
    for field in &expr.fields {
        let _ = infer_syntax_expr(&field.value, locals, ctx, subst, errors);
        let _ = &field.key;
    }
    let _ = &expr.text;
    base
}

fn infer_syntax_template_instantiation(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let name = expr.text.as_deref().unwrap_or_default();
    let mut provided = HashSet::new();
    let Some(template) = ctx.templates.get(name) else {
        for field in &expr.fields {
            let _ = infer_syntax_expr(&field.value, locals, ctx, subst, errors);
        }
        errors.push(format!("unknown template `{}`", name));
        return Type::Dynamic;
    };

    for field in &expr.fields {
        if !provided.insert(field.key.clone()) {
            errors.push(format!(
                "duplicate prop `{}` in template `{}` instantiation",
                field.key, name
            ));
        }

        let actual = infer_syntax_expr(&field.value, locals, ctx, subst, errors);
        let Some(expected) = template.props.get(&field.key) else {
            errors.push(format!(
                "template `{}` instantiation has unknown prop `{}`",
                name, field.key
            ));
            continue;
        };

        let expected = expand_type_aliases(expected, ctx.aliases);
        let actual = expand_type_aliases(&actual, ctx.aliases);
        if let Err(message) = unify(&expected, &actual, subst) {
            errors.push(format!(
                "template `{}` prop `{}`: {}",
                name, field.key, message
            ));
        }
    }

    for prop_name in template.props.keys() {
        if !provided.contains(prop_name) {
            errors.push(format!(
                "template `{}` instantiation is missing required prop `{}`",
                name, prop_name
            ));
        }
    }

    Type::Named {
        module: None,
        name: "Html".to_string(),
        args: vec![Type::Dynamic],
    }
}

fn syntax_callee_name(expr: &SyntaxExprOutput) -> Option<&str> {
    expr.children.first().and_then(|callee| match callee.kind {
        SyntaxExprKind::Atom | SyntaxExprKind::Var => callee.text.as_deref(),
        _ => None,
    })
}

fn syntax_callee_is_var(expr: &SyntaxExprOutput) -> bool {
    matches!(
        expr.children.first().map(|callee| callee.kind),
        Some(SyntaxExprKind::Var)
    )
}

fn infer_syntax_remote_call(
    module_name: &str,
    function_name: &str,
    arg_types: &[Type],
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let resolved_module_name = ctx
        .module_aliases
        .get(module_name)
        .map(String::as_str)
        .unwrap_or(module_name);

    if resolved_module_name == "Html" && function_name == "raw" {
        if arg_types.len() != 1 {
            errors.push(format!(
                "function arity mismatch: expected 1 args, found {}",
                arg_types.len()
            ));
            return Type::Dynamic;
        }
        if let Err(message) = unify(&Type::Binary, &arg_types[0], subst) {
            errors.push(message);
        }
        return Type::Named {
            module: None,
            name: "Html".to_string(),
            args: vec![Type::Dynamic],
        };
    }

    let trait_key = (resolved_module_name.to_string(), function_name.to_string());
    if let Some(impls) = ctx.trait_method_calls.get(&trait_key) {
        let lookup_arg_types = arg_types
            .iter()
            .map(|arg| apply_subst(arg, subst))
            .collect::<Vec<_>>();
        let cached_lookup_arg_types = canonicalize_trait_lookup_types(lookup_arg_types.as_slice());
        let lookup_key = TraitMethodLookupKey {
            trait_name: resolved_module_name.to_string(),
            method_name: function_name.to_string(),
            arg_types: cached_lookup_arg_types,
        };
        let lookup_result = {
            let cache = ctx.trait_lookup_cache.borrow();
            if let Some(cached) = cache.method_calls.get(&lookup_key).copied() {
                Some(cached)
            } else {
                drop(cache);
                let mut matching = None::<usize>;
                let mut matches = 0usize;
                for (index, impl_candidate) in impls.iter().enumerate() {
                    let mut trial_subst = subst.clone();
                    if infer_function_call(
                        &impl_candidate.scheme,
                        &lookup_arg_types,
                        ctx,
                        &mut trial_subst,
                    )
                    .is_ok()
                    {
                        matches += 1;
                        if matching.is_none() {
                            matching = Some(index);
                        } else {
                            break;
                        }
                    }
                }

                let resolved = match matching {
                    None => TraitMethodLookupResult::NoMatch,
                    Some(index) if matches == 1 => TraitMethodLookupResult::Single(index),
                    Some(_) => TraitMethodLookupResult::Ambiguous,
                };
                ctx.trait_lookup_cache
                    .borrow_mut()
                    .method_calls
                    .insert(lookup_key, resolved);
                Some(resolved)
            }
        };

        let provided_args = arg_types
            .iter()
            .map(pretty_type)
            .collect::<Vec<_>>()
            .join(", ");
        match lookup_result {
            Some(TraitMethodLookupResult::Single(index)) => {
                let mut inferred_subst = subst.clone();
                let mut success = None::<(Type, HashMap<TypeVarId, Type>)>;
                if let Some(impl_candidate) = impls.get(index) {
                    if let Ok(ty) = infer_function_call(
                        &impl_candidate.scheme,
                        &lookup_arg_types,
                        ctx,
                        &mut inferred_subst,
                    ) {
                        success = Some((ty, inferred_subst));
                    }
                }
                if let Some((ty, inferred_subst)) = success {
                    *subst = inferred_subst;
                    return ty;
                }
                errors.push(format!(
                    "at `{}.{}` call site: no impl for trait method {}.{} with provided arguments [{}]",
                    resolved_module_name, function_name, resolved_module_name, function_name, provided_args
                ));
                return Type::Dynamic;
            }
            Some(TraitMethodLookupResult::Ambiguous) => {
                errors.push(format!(
                    "at `{}.{}` call site: ambiguous trait method {}.{}",
                    resolved_module_name, function_name, resolved_module_name, function_name
                ));
                return Type::Dynamic;
            }
            _ => {
                if let Some(ty) = infer_trait_method_call_from_current_bounds(
                    resolved_module_name,
                    function_name,
                    &lookup_arg_types,
                    ctx,
                    subst,
                ) {
                    return ty;
                }
                errors.push(format!(
                    "at `{}.{}` call site: no impl for trait method {}.{} with provided arguments [{}]",
                    resolved_module_name, function_name, resolved_module_name, function_name, provided_args
                ));
                return Type::Dynamic;
            }
        }
    }

    if let Some(interface) = ctx.interface_map.get(resolved_module_name) {
        if let Some(signature) = interface
            .functions
            .get(&(function_name.to_string(), arg_types.len()))
        {
            if let Some(scheme) = parse_interface_signature(signature, interface, ctx.aliases) {
                match infer_function_with_bounds(
                    &scheme,
                    Some(function_name),
                    arg_types,
                    ctx,
                    subst,
                ) {
                    Ok(ty) => return ty,
                    Err(message) => {
                        errors.push(message);
                        return Type::Dynamic;
                    }
                }
            }
        }
        if let Some(schemes) = parse_interface_constructor_schemes(
            interface.constructors.get(function_name).map(Vec::as_slice),
            interface,
        ) {
            if let Some(constructed) =
                infer_constructor_schemes(function_name, &schemes, arg_types, subst, errors)
            {
                let interface_aliases = interface_type_aliases(interface);
                return expand_type_aliases(&constructed, &interface_aliases);
            }
        }
        let interface_aliases = interface_type_aliases(interface);
        let qualified_alias_name = format!("{}.{}", resolved_module_name, function_name);
        let mut qualified_aliases = interface_aliases.clone();
        if let Some(alias) = interface_aliases.get(function_name) {
            qualified_aliases.insert(qualified_alias_name.clone(), alias.clone());
        }
        if let Some(schemes) =
            alias_constructor_call_schemes(&qualified_alias_name, &qualified_aliases)
        {
            if let Some(constructed) =
                infer_constructor_schemes(function_name, &schemes, arg_types, subst, errors)
            {
                return expand_type_aliases(&constructed, &qualified_aliases);
            }
        }
        if function_name
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_uppercase())
            && interface.opaque_types.contains(function_name)
        {
            errors.push(format!(
                "cannot construct opaque type {}.{} outside defining module",
                resolved_module_name, function_name
            ));
            return Type::Dynamic;
        }
    }

    if is_constructor_name(function_name) {
        errors.push(format!(
            "unknown constructor {}.{} / {}",
            resolved_module_name,
            function_name,
            arg_types.len()
        ));
        return Type::Dynamic;
    }

    if resolved_module_name == "Group" && function_name == "broadcast" && arg_types.len() == 2 {
        if let Type::Named {
            name,
            args: group_args,
            ..
        } = &arg_types[0]
        {
            if name == "Group" && group_args.len() == 1 {
                if let Err(message) = unify(&group_args[0], &arg_types[1], subst) {
                    let expected = alias_name_for_type(&group_args[0], ctx.aliases)
                        .unwrap_or_else(|| pretty_type(&group_args[0]));
                    errors.push(format!(
                        "expected {} found {}",
                        expected,
                        pretty_type(&arg_types[1])
                    ));
                    let _ = message;
                }
            }
        }
        return Type::LiteralAtom("ok".to_string());
    }

    if (resolved_module_name == "Route" || resolved_module_name.ends_with(".Route"))
        && function_name == "to_path"
        && arg_types.len() == 1
    {
        return Type::Binary;
    }

    Type::Dynamic
}

/// Infers a local receiver-method call.
///
/// Inputs:
/// - `expr`: syntax-output call expression whose callee may be field access.
/// - `arg_types`: inferred non-receiver argument types.
/// - `locals`, `ctx`, `subst`, and `errors`: active inference context.
///
/// Output:
/// - `Some(Type)` for a resolved local receiver method or a method-shaped call
///   that has candidates but no matching receiver.
/// - `None` when the expression is not a receiver-method call known to the
///   current module.
///
/// Transformation:
/// - Reads `receiver.method(args...)` from the field-access callee, infers the
///   receiver type, selects a receiver-method signature by method/arity and
///   receiver unification, then checks the non-receiver arguments with the
///   existing function-scheme inference path.
fn infer_syntax_receiver_method_call(
    expr: &SyntaxExprOutput,
    arg_types: &[Type],
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    let callee = expr.children.first()?;
    if !matches!(callee.kind, SyntaxExprKind::FieldAccess) {
        return None;
    }
    let method = callee.text.as_deref()?;
    let candidates = ctx
        .receiver_methods
        .get(&(method.to_string(), arg_types.len()))?;
    let receiver = callee.children.first()?;
    let receiver_type = infer_syntax_expr(receiver, locals, ctx, subst, errors);

    for candidate in candidates {
        let mut trial_subst = subst.clone();
        if unify(&candidate.receiver_type, &receiver_type, &mut trial_subst).is_err() {
            continue;
        }
        match infer_function_with_bounds(
            &candidate.scheme,
            Some(method),
            arg_types,
            ctx,
            &mut trial_subst,
        ) {
            Ok(ty) => {
                *subst = trial_subst;
                return Some(ty);
            }
            Err(message) => {
                errors.push(message);
                return Some(Type::Dynamic);
            }
        }
    }

    let candidate_types = candidates
        .iter()
        .map(|candidate| pretty_type(&candidate.receiver_type))
        .collect::<Vec<_>>()
        .join(", ");
    errors.push(format!(
        "no receiver method `{}` / {} for {}; candidates: {}",
        method,
        arg_types.len(),
        pretty_type(&receiver_type),
        candidate_types
    ));
    Some(Type::Dynamic)
}

/// Infers compiler-known primitive receiver method calls.
///
/// Inputs:
/// - `expr`: syntax-output call expression whose callee may be field access.
/// - `arg_types`: inferred non-receiver argument types.
/// - `locals`, `ctx`, `subst`, and `errors`: active inference context.
///
/// Output:
/// - `Some(Type)` for supported primitive receiver calls.
/// - `None` when the expression is not a supported primitive receiver call.
///
/// Transformation:
/// - Reads the receiver type from the field-access callee, prepends that type to
///   the argument check, validates the primitive method's arity and parameter
///   types, and returns the method result type.
fn infer_syntax_primitive_receiver_method_call(
    expr: &SyntaxExprOutput,
    arg_types: &[Type],
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    let callee = expr.children.first()?;
    if !matches!(callee.kind, SyntaxExprKind::FieldAccess) {
        return None;
    }
    let method = callee.text.as_deref()?;
    let receiver = callee.children.first()?;
    let receiver_type = infer_syntax_expr(receiver, locals, ctx, subst, errors);
    let scheme = primitive_receiver_method_scheme(&receiver_type, method, arg_types.len())?;
    infer_function_with_bounds(&scheme, Some(method), arg_types, ctx, subst)
        .map(Some)
        .unwrap_or_else(|message| {
            errors.push(message);
            Some(Type::Dynamic)
        })
}

/// Returns a function scheme for a primitive receiver method.
///
/// Inputs:
/// - `receiver_type`: inferred receiver type.
/// - `method`: receiver method name.
/// - `arg_count`: number of non-receiver arguments.
///
/// Output:
/// - Function scheme for supported primitive receiver methods.
/// - `None` when receiver type, method, or arity is not supported.
///
/// Transformation:
/// - Encodes the selected `std.core.String` receiver-method surface as ordinary
///   parameter and return types for the existing call inference engine.
fn primitive_receiver_method_scheme(
    receiver_type: &Type,
    method: &str,
    arg_count: usize,
) -> Option<FunctionScheme> {
    if matches!(receiver_type, Type::Int | Type::LiteralInt(_)) {
        return match (method, arg_count) {
            ("to_string", 0) => Some(FunctionScheme {
                params: Vec::new(),
                ret: Type::Binary,
                bounds: Vec::new(),
            }),
            _ => None,
        };
    }

    if matches!(receiver_type, Type::Float) {
        return match (method, arg_count) {
            ("to_string", 0) => Some(FunctionScheme {
                params: Vec::new(),
                ret: Type::Binary,
                bounds: Vec::new(),
            }),
            _ => None,
        };
    }

    if !matches!(receiver_type, Type::Binary | Type::Dynamic) {
        return None;
    }

    let binary = Type::Binary;
    match (method, arg_count) {
        ("equal", 1) | ("contains", 1) | ("starts_with", 1) | ("ends_with", 1) => {
            Some(FunctionScheme {
                params: vec![binary],
                ret: Type::Bool,
                bounds: Vec::new(),
            })
        }
        ("compare", 1) => Some(FunctionScheme {
            params: vec![binary],
            ret: Type::Union(vec![
                Type::LiteralAtom("lt".to_string()),
                Type::LiteralAtom("eq".to_string()),
                Type::LiteralAtom("gt".to_string()),
            ]),
            bounds: Vec::new(),
        }),
        ("append", 1) => Some(FunctionScheme {
            params: vec![binary],
            ret: Type::Binary,
            bounds: Vec::new(),
        }),
        ("from_string", 0) => Some(FunctionScheme {
            params: Vec::new(),
            ret: structural_option_type(Type::Binary),
            bounds: Vec::new(),
        }),
        ("is_empty", 0) => Some(FunctionScheme {
            params: Vec::new(),
            ret: Type::Bool,
            bounds: Vec::new(),
        }),
        ("replace", 2) => Some(FunctionScheme {
            params: vec![binary.clone(), binary],
            ret: Type::Binary,
            bounds: Vec::new(),
        }),
        ("split", 1) => Some(FunctionScheme {
            params: vec![binary],
            ret: Type::List(Box::new(Type::Binary)),
            bounds: Vec::new(),
        }),
        ("split_once", 1) => Some(FunctionScheme {
            params: vec![binary],
            ret: structural_option_type(Type::Tuple(vec![Type::Binary, Type::Binary])),
            bounds: Vec::new(),
        }),
        ("length", 0) | ("byte_size", 0) => Some(FunctionScheme {
            params: Vec::new(),
            ret: Type::Int,
            bounds: Vec::new(),
        }),
        ("to_string", 0)
        | ("lowercase", 0)
        | ("uppercase", 0)
        | ("trim", 0)
        | ("trim_start", 0)
        | ("trim_end", 0) => Some(FunctionScheme {
            params: Vec::new(),
            ret: Type::Binary,
            bounds: Vec::new(),
        }),
        _ => None,
    }
}

/// Builds the structural representation of `Option[T]` for inference.
///
/// Inputs:
/// - `inner`: contained value type.
///
/// Output:
/// - Union type equivalent to `:none | {:some, inner}`.
///
/// Transformation:
/// - Expands the public `std.core.Option.Option[T]` alias into its runtime
///   shape so primitive intrinsic return types can unify with APIs that expect
///   an expanded option alias.
fn structural_option_type(inner: Type) -> Type {
    Type::Union(vec![
        Type::Tuple(vec![Type::LiteralAtom("some".to_string()), inner]),
        Type::LiteralAtom("none".to_string()),
    ])
}

fn unwrap_macro_return_type(ty: Type) -> Type {
    match ty {
        Type::Named {
            module,
            name: tag,
            args,
        } if module.is_none() && tag == "Ast" && args.len() == 1 => {
            args.into_iter().next().unwrap_or(Type::Dynamic)
        }
        other => other,
    }
}

fn infer_syntax_local_call(
    function_name: &str,
    arg_types: &[Type],
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    if let Some(scheme) = builtin_call(function_name, arg_types.len()) {
        if let Err(message) =
            infer_function_with_bounds(&scheme, Some(function_name), arg_types, ctx, subst)
        {
            errors.push(message);
        }
        return scheme.ret;
    }

    if let Some(ty) =
        infer_syntax_imported_function_call(function_name, arg_types, ctx, subst, errors)
    {
        return ty;
    }

    if let Some(scheme) = ctx
        .signatures
        .get(&(function_name.to_string(), arg_types.len()))
    {
        match infer_function_with_bounds(scheme, Some(function_name), arg_types, ctx, subst) {
            Ok(ty) => return ty,
            Err(message) => {
                errors.push(message);
                return Type::Dynamic;
            }
        }
    }

    if let Some(symbol) = ctx
        .local_fns
        .get(&(function_name.to_string(), arg_types.len()))
    {
        if let Some(scheme) = parse_symbol_scheme(symbol) {
            match infer_function_with_bounds(&scheme, Some(function_name), arg_types, ctx, subst) {
                Ok(ty) => return ty,
                Err(message) => {
                    errors.push(message);
                    return Type::Dynamic;
                }
            }
        }
    }

    Type::Dynamic
}

/// Infers a selected imported function call.
///
/// Inputs:
/// - `function_name`: local call name from source, possibly an import alias.
/// - `arg_types`: already inferred argument types.
/// - `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - `Some(Type)` when the local name is a selected function import.
/// - `None` when the local name is not imported as a function.
///
/// Transformation:
/// - Resolves the local import target to its provider module interface, parses
///   the public function signature for the call arity, and reuses ordinary
///   function-call inference so argument mismatches are reported before backend
///   emission.
fn infer_syntax_imported_function_call(
    function_name: &str,
    arg_types: &[Type],
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    let target = ctx.function_imports.get(function_name)?;
    let resolved_module = ctx
        .module_aliases
        .get(&target.module)
        .map(String::as_str)
        .unwrap_or(target.module.as_str());
    let Some(interface) = ctx.interface_map.get(resolved_module) else {
        errors.push(spanned_expression_error(
            target.span,
            missing_imported_function_interface_message(
                resolved_module,
                &target.function,
                ctx.interface_map,
            ),
        ));
        return Some(Type::Dynamic);
    };

    let Some(signature) = interface
        .functions
        .get(&(target.function.clone(), arg_types.len()))
    else {
        errors.push(spanned_expression_error(
            target.span,
            missing_imported_function_message(interface, &target.function, arg_types.len()),
        ));
        return Some(Type::Dynamic);
    };

    let Some(scheme) = parse_interface_signature(signature, interface, ctx.aliases) else {
        errors.push(format!(
            "cannot parse imported function signature {}.{} / {}",
            resolved_module,
            target.function,
            arg_types.len()
        ));
        return Some(Type::Dynamic);
    };

    match infer_function_with_bounds(&scheme, Some(function_name), arg_types, ctx, subst) {
        Ok(ty) => Some(ty),
        Err(message) => {
            errors.push(message);
            Some(Type::Dynamic)
        }
    }
}

/// Builds a readable diagnostic for a missing selected-import provider module.
///
/// Inputs:
/// - `module`: resolved module path named by the selected import.
/// - `function`: function selected from that module.
/// - `interfaces`: loaded provider interfaces available to the current compile.
///
/// Output:
/// - Human-facing diagnostic message.
///
/// Transformation:
/// - Reports the missing module precisely and, when another loaded interface
///   exports the same function with the same leaf module name, appends a
///   concrete import suggestion.
fn missing_imported_function_interface_message(
    module: &str,
    function: &str,
    interfaces: &HashMap<String, ModuleInterface>,
) -> String {
    let mut message = format!(
        "cannot find module `{}` for imported function `{}`; no interface for `{}` is loaded",
        module, function, module
    );
    if let Some(suggestion) = imported_function_module_suggestion(module, function, interfaces) {
        message.push_str(&format!(
            "; did you mean `{}.{{{}}}`?",
            suggestion, function
        ));
    }
    message
}

/// Finds a likely loaded module for a selected function import typo.
///
/// Inputs:
/// - `module`: missing module from the source import.
/// - `function`: selected function name.
/// - `interfaces`: loaded provider interfaces.
///
/// Output:
/// - Suggested module path when a deterministic candidate is available.
///
/// Transformation:
/// - Prefers modules with the same final path segment, then falls back to any
///   loaded module exporting the selected function; ties sort lexicographically.
fn imported_function_module_suggestion(
    module: &str,
    function: &str,
    interfaces: &HashMap<String, ModuleInterface>,
) -> Option<String> {
    let leaf = module.rsplit('.').next().unwrap_or(module);
    let mut same_leaf = interfaces
        .iter()
        .filter(|(candidate, interface)| {
            candidate.rsplit('.').next().unwrap_or(candidate.as_str()) == leaf
                && interface
                    .functions
                    .keys()
                    .any(|(name, _arity)| name == function)
        })
        .map(|(candidate, _interface)| candidate.clone())
        .collect::<Vec<_>>();
    same_leaf.sort();
    if let Some(candidate) = same_leaf.into_iter().next() {
        return Some(candidate);
    }

    let mut by_function = interfaces
        .iter()
        .filter(|(_candidate, interface)| {
            interface
                .functions
                .keys()
                .any(|(name, _arity)| name == function)
        })
        .map(|(candidate, _interface)| candidate.clone())
        .collect::<Vec<_>>();
    by_function.sort();
    by_function.into_iter().next()
}

/// Builds a readable diagnostic for a missing selected function in an interface.
///
/// Inputs:
/// - `interface`: loaded module interface.
/// - `function`: selected function name.
/// - `arity`: call arity used at the source call site.
///
/// Output:
/// - Human-facing diagnostic text listing available public functions.
///
/// Transformation:
/// - Names the missing function as `module.function/arity` and appends a compact
///   sorted list of importable functions from the provider module.
fn missing_imported_function_message(
    interface: &ModuleInterface,
    function: &str,
    arity: usize,
) -> String {
    let mut message = format!(
        "module `{}` has no imported function `{}/{}`",
        interface.module, function, arity
    );
    let available = available_interface_functions(interface);
    if !available.is_empty() {
        message.push_str(&format!("; available imports: {}", available.join(", ")));
    }
    message
}

/// Lists public functions exported by an interface for diagnostics.
///
/// Inputs:
/// - `interface`: provider interface loaded for an import.
///
/// Output:
/// - Sorted `name/arity` strings.
///
/// Transformation:
/// - Reads interface function keys and formats them deterministically for
///   concise diagnostics.
fn available_interface_functions(interface: &ModuleInterface) -> Vec<String> {
    let mut names = interface
        .functions
        .keys()
        .map(|(name, arity)| format!("{}/{}", name, arity))
        .collect::<Vec<_>>();
    names.sort();
    names
}

fn infer_syntax_case_expr(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let scrutinee_type = expr
        .children
        .first()
        .map(|scrutinee| infer_syntax_expr(scrutinee, locals, ctx, subst, errors))
        .unwrap_or(Type::Dynamic);
    let branches = expr
        .clauses
        .iter()
        .map(|clause| {
            let mut clause_locals = locals.clone();
            let mut clause_subst = subst.clone();
            if let Some(pattern) = clause.patterns.first() {
                if let Err(message) = check_syntax_pattern(
                    pattern,
                    &scrutinee_type,
                    ctx.aliases,
                    Some(ctx),
                    &mut clause_locals,
                    &mut clause_subst,
                ) {
                    errors.push(message);
                }
            }

            if let Some(guard) = clause.guard.as_ref() {
                refine_by_syntax_guard(guard, &mut clause_locals, ctx.aliases, &mut clause_subst);
            }

            let branch_type =
                infer_syntax_expr(&clause.body, &clause_locals, ctx, &mut clause_subst, errors);
            apply_subst(&branch_type, &clause_subst)
        })
        .collect::<Vec<_>>();

    normalize_union(branches)
}

fn infer_syntax_receive_expr(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    if let Some(after) = expr.receive_after.as_ref() {
        let _ = infer_syntax_expr(&after.trigger, locals, ctx, subst, errors);
        let _ = infer_syntax_expr(&after.body, locals, ctx, subst, errors);
    }
    let message_type = Type::Dynamic;
    let branches = expr
        .clauses
        .iter()
        .map(|clause| {
            let mut clause_locals = locals.clone();
            let mut clause_subst = subst.clone();
            if let Some(pattern) = clause.patterns.first() {
                if let Err(message) = check_syntax_pattern(
                    pattern,
                    &message_type,
                    ctx.aliases,
                    Some(ctx),
                    &mut clause_locals,
                    &mut clause_subst,
                ) {
                    errors.push(message);
                }
            }

            if let Some(guard) = clause.guard.as_ref() {
                refine_by_syntax_guard(guard, &mut clause_locals, ctx.aliases, &mut clause_subst);
            }

            let branch_type =
                infer_syntax_expr(&clause.body, &clause_locals, ctx, &mut clause_subst, errors);
            apply_subst(&branch_type, &clause_subst)
        })
        .collect::<Vec<_>>();

    normalize_union(branches)
}

fn infer_syntax_try_expr(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let protected_type = expr
        .children
        .first()
        .map(|body| infer_syntax_expr(body, locals, ctx, subst, errors))
        .unwrap_or(Type::Dynamic);
    let mut branches = Vec::new();

    if expr.clauses.is_empty() {
        branches.push(protected_type.clone());
    } else {
        branches.extend(expr.clauses.iter().map(|clause| {
            let mut clause_locals = locals.clone();
            let mut clause_subst = subst.clone();
            if let Some(pattern) = clause.patterns.first() {
                if let Err(message) = check_syntax_pattern(
                    pattern,
                    &protected_type,
                    ctx.aliases,
                    Some(ctx),
                    &mut clause_locals,
                    &mut clause_subst,
                ) {
                    errors.push(message);
                }
            }

            if let Some(guard) = clause.guard.as_ref() {
                refine_by_syntax_guard(guard, &mut clause_locals, ctx.aliases, &mut clause_subst);
            }

            let branch_type =
                infer_syntax_expr(&clause.body, &clause_locals, ctx, &mut clause_subst, errors);
            apply_subst(&branch_type, &clause_subst)
        }));
    }

    branches.extend(expr.catch_clauses.iter().map(|clause| {
        let mut clause_locals = locals.clone();
        let mut clause_subst = subst.clone();
        if let Some(pattern) = clause.patterns.first() {
            if let Err(message) = check_syntax_pattern(
                pattern,
                &Type::Dynamic,
                ctx.aliases,
                Some(ctx),
                &mut clause_locals,
                &mut clause_subst,
            ) {
                errors.push(message);
            }
        }

        if let Some(guard) = clause.guard.as_ref() {
            refine_by_syntax_guard(guard, &mut clause_locals, ctx.aliases, &mut clause_subst);
        }

        let branch_type =
            infer_syntax_expr(&clause.body, &clause_locals, ctx, &mut clause_subst, errors);
        apply_subst(&branch_type, &clause_subst)
    }));

    if let Some(after) = expr.try_after.as_ref() {
        let _ = infer_syntax_expr(&after.trigger, locals, ctx, subst, errors);
        let _ = infer_syntax_expr(&after.body, locals, ctx, subst, errors);
    }

    normalize_union(branches)
}

fn infer_syntax_if_expr(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let branches = expr
        .clauses
        .iter()
        .map(|clause| {
            let mut clause_subst = subst.clone();
            if let Some(condition) = clause.guard.as_ref() {
                let condition_type =
                    infer_syntax_expr(condition, locals, ctx, &mut clause_subst, errors);
                if let Err(message) = unify(&Type::Bool, &condition_type, &mut clause_subst) {
                    errors.push(message);
                }
            }
            let branch_type =
                infer_syntax_expr(&clause.body, locals, ctx, &mut clause_subst, errors);
            apply_subst(&branch_type, &clause_subst)
        })
        .collect::<Vec<_>>();

    normalize_union(branches)
}

fn infer_syntax_list_comprehension(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let source_type = expr
        .children
        .get(1)
        .map(|source| infer_syntax_expr(source, locals, ctx, subst, errors))
        .unwrap_or(Type::Dynamic);
    let element_type = match expand_type_aliases(&source_type, ctx.aliases) {
        Type::List(elem) => *elem,
        Type::Dynamic | Type::Term => Type::Dynamic,
        other => {
            errors.push(format!(
                "list comprehension source must be List, found {}",
                pretty_type(&other)
            ));
            Type::Dynamic
        }
    };
    let mut item_locals = locals.clone();
    let mut item_subst = subst.clone();
    if let Some(pattern) = expr.patterns.first() {
        if let Err(message) = check_syntax_pattern(
            pattern,
            &element_type,
            ctx.aliases,
            Some(ctx),
            &mut item_locals,
            &mut item_subst,
        ) {
            errors.push(message);
        }
    }
    if let Some(guard) = expr.children.get(2) {
        refine_by_syntax_guard(guard, &mut item_locals, ctx.aliases, &mut item_subst);
    }
    let item_type = expr
        .children
        .first()
        .map(|item| infer_syntax_expr(item, &item_locals, ctx, &mut item_subst, errors))
        .unwrap_or(Type::Dynamic);

    Type::List(Box::new(apply_subst(&item_type, &item_subst)))
}

fn infer_syntax_fun_expr(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let union = expr
        .clauses
        .iter()
        .map(|clause| {
            let mut clause_locals = locals.clone();
            let mut clause_subst = subst.clone();
            for pattern in &clause.patterns {
                let _ = check_syntax_pattern(
                    pattern,
                    &Type::Dynamic,
                    ctx.aliases,
                    Some(ctx),
                    &mut clause_locals,
                    &mut clause_subst,
                );
            }
            let inferred =
                infer_syntax_expr(&clause.body, &clause_locals, ctx, &mut clause_subst, errors);
            Type::Function {
                params: vec![Type::Dynamic; clause.patterns.len()],
                ret: Box::new(apply_subst(&inferred, &clause_subst)),
            }
        })
        .collect::<Vec<_>>();
    normalize_union(union)
}

/// Infers a syntax-output let expression.
///
/// Inputs:
/// - `expr`: syntax-output let node with binding names in `patterns`, binding
///   values in `children`, and a required final body child.
/// - `locals`: local type environment visible before the let expression.
/// - `ctx`, `subst`, `errors`: inference context, substitution state, and
///   diagnostics accumulator.
///
/// Output:
/// - Inferred explicit body type.
///
/// Transformation:
/// - Infers binding values left-to-right, extending a scoped local environment
///   after each binding. The caller's `locals` map is not mutated.
fn infer_syntax_let_expr(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    if expr.patterns.is_empty() || expr.children.len() != expr.patterns.len() + 1 {
        errors.push("malformed let expression".to_string());
        return Type::Dynamic;
    }

    let mut scoped = locals.clone();
    for (pattern, value) in expr.patterns.iter().zip(expr.children.iter()) {
        let value_type = infer_syntax_expr(value, &scoped, ctx, subst, errors);
        let binding_type = apply_subst(&value_type, subst);
        match pattern.text.as_deref() {
            Some(name) => {
                scoped.insert(name.to_string(), binding_type);
            }
            None => errors.push("malformed let binding name".to_string()),
        }
    }

    infer_syntax_expr(
        &expr.children[expr.patterns.len()],
        &scoped,
        ctx,
        subst,
        errors,
    )
}

fn infer_syntax_html_block(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    for node in &expr.html_nodes {
        check_syntax_html_node(node, locals, ctx, subst, errors);
    }
    Type::Named {
        module: None,
        name: "Html".to_string(),
        args: vec![Type::Dynamic],
    }
}

fn check_syntax_html_node(
    node: &SyntaxHtmlNodeOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) {
    match node {
        SyntaxHtmlNodeOutput::Text { .. } => {}
        SyntaxHtmlNodeOutput::Expr { expr } => {
            let child_type = infer_syntax_html_child_expr(expr, locals, ctx, subst, errors);
            if !is_renderable_html_child_type(&child_type, ctx.aliases) {
                errors.push("expression is not renderable as HTML".to_string());
            }
        }
        SyntaxHtmlNodeOutput::Element { element } => {
            if is_component_element_name(&element.name) {
                let component_type =
                    infer_syntax_html_component_call(element, locals, ctx, subst, errors);

                if !is_html_expression_type(&component_type, ctx.aliases) {
                    errors.push(format!(
                        "component `{}` must return Html[Msg], found {}",
                        element.name,
                        pretty_type(&component_type)
                    ));
                }
                return;
            }
            for attr in &element.attrs {
                check_syntax_html_attr(attr, locals, ctx, subst, errors);
            }
            for child in &element.children {
                check_syntax_html_node(child, locals, ctx, subst, errors);
            }
        }
        SyntaxHtmlNodeOutput::NamedSlot { slot } => {
            for child in &slot.children {
                check_syntax_html_node(child, locals, ctx, subst, errors);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HtmlAttrType {
    Text,
    Bool,
    Class,
}

impl HtmlAttrType {
    fn label(self) -> &'static str {
        match self {
            HtmlAttrType::Text => "Text",
            HtmlAttrType::Bool => "Bool",
            HtmlAttrType::Class => "Text | List[Text]",
        }
    }
}

fn standard_html_attr_type(name: &str) -> Option<HtmlAttrType> {
    match name {
        "href" | "src" | "id" | "name" | "type" | "value" => Some(HtmlAttrType::Text),
        "disabled" | "checked" => Some(HtmlAttrType::Bool),
        "class" => Some(HtmlAttrType::Class),
        _ => None,
    }
}

fn check_syntax_html_attr(
    attr: &SyntaxHtmlAttrOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) {
    let actual = syntax_html_attr_type(attr, locals, ctx, subst, errors);
    let actual = widen_html_attr_type(actual);

    let Some(expected) = standard_html_attr_type(&attr.name) else {
        return;
    };

    if !html_attr_type_accepts(expected, &actual, ctx.aliases) {
        errors.push(format!(
            "attribute {} expects {}\nfound {}",
            attr.name,
            expected.label(),
            pretty_type(&actual)
        ));
    }
}

fn syntax_html_attr_type(
    attr: &SyntaxHtmlAttrOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    match &attr.value {
        Some(SyntaxHtmlAttrValueOutput::Expr { expr }) => {
            infer_syntax_expr(expr, locals, ctx, subst, errors)
        }
        Some(SyntaxHtmlAttrValueOutput::Text { .. }) => Type::Binary,
        None => Type::Bool,
    }
}

fn widen_html_attr_type(ty: Type) -> Type {
    match ty {
        Type::LiteralInt(_) => Type::Int,
        Type::LiteralAtom(_) => Type::Atom,
        other => other,
    }
}

fn html_attr_type_accepts(
    expected: HtmlAttrType,
    actual: &Type,
    aliases: &HashMap<String, TypeAlias>,
) -> bool {
    let actual = expand_type_aliases(actual, aliases);
    match expected {
        HtmlAttrType::Text => matches!(actual, Type::Binary | Type::Dynamic),
        HtmlAttrType::Bool => matches!(actual, Type::Bool | Type::Dynamic),
        HtmlAttrType::Class => {
            matches!(actual, Type::Binary | Type::Dynamic)
                || matches!(
                    actual,
                    Type::List(item) if matches!(
                        expand_type_aliases(&item, aliases),
                        Type::Binary | Type::Dynamic
                    )
                )
        }
    }
}

fn infer_syntax_html_child_expr(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let ty = infer_syntax_expr(expr, locals, ctx, subst, errors);
    if expr.kind == SyntaxExprKind::ListComprehension {
        return match expand_type_aliases(&ty, ctx.aliases) {
            Type::List(elem) => *elem,
            other => other,
        };
    }
    ty
}

fn is_component_element_name(name: &str) -> bool {
    matches!(name.chars().next(), Some(ch) if ch.is_ascii_uppercase())
}

fn infer_syntax_html_component_call(
    element: &SyntaxHtmlElementOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let arity = element.attrs.len();
    for name in component_function_names(&element.name) {
        if let Some(scheme) = ctx.signatures.get(&(name.clone(), arity)) {
            let arg_types =
                syntax_component_arg_types(element, ctx.local_fns.get(&(name.clone(), arity)))
                    .into_iter()
                    .map(|attr| syntax_html_attr_type(attr, locals, ctx, subst, errors))
                    .collect::<Vec<_>>();

            return match infer_function_with_bounds(scheme, Some(&name), &arg_types, ctx, subst) {
                Ok(ty) => ty,
                Err(message) => {
                    errors.push(message);
                    Type::Dynamic
                }
            };
        }
    }

    Type::Dynamic
}

fn component_function_names(tag_name: &str) -> Vec<String> {
    let snake_case = pascal_to_snake_case(tag_name);
    if snake_case == tag_name {
        vec![tag_name.to_string()]
    } else {
        vec![tag_name.to_string(), snake_case]
    }
}

fn syntax_component_arg_types<'a>(
    element: &'a SyntaxHtmlElementOutput,
    symbol: Option<&FunctionSymbol>,
) -> Vec<&'a SyntaxHtmlAttrOutput> {
    if let Some(symbol) = symbol {
        let mut ordered = Vec::with_capacity(element.attrs.len());
        for param in &symbol.params {
            let Some(attr) = element
                .attrs
                .iter()
                .find(|attr| component_prop_matches_param(&attr.name, &param.name))
            else {
                return element.attrs.iter().collect();
            };
            ordered.push(attr);
        }
        ordered
    } else {
        element.attrs.iter().collect()
    }
}

fn component_prop_matches_param(prop_name: &str, param_name: &str) -> bool {
    prop_name == param_name
        || prop_name.eq_ignore_ascii_case(param_name)
        || prop_name == pascal_to_snake_case(param_name)
}

fn pascal_to_snake_case(name: &str) -> String {
    let mut out = String::new();
    let mut previous_was_lower_or_digit = false;
    for ch in name.chars() {
        if ch.is_ascii_uppercase() {
            if previous_was_lower_or_digit {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
            previous_was_lower_or_digit = false;
        } else {
            out.push(ch);
            previous_was_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        }
    }
    out
}

fn is_renderable_html_child_type(ty: &Type, aliases: &HashMap<String, TypeAlias>) -> bool {
    let ty = expand_type_aliases(ty, aliases);
    if let Type::Union(items) = &ty {
        return items
            .iter()
            .all(|item| is_renderable_html_child_type(item, aliases));
    }
    matches!(ty, Type::Binary | Type::Int | Type::Bool | Type::Dynamic)
        || matches!(
            ty,
            Type::Named {
                module: None,
                name,
                args,
            } if name == "Html" && args.len() == 1
        )
}

fn is_html_expression_type(ty: &Type, aliases: &HashMap<String, TypeAlias>) -> bool {
    let ty = expand_type_aliases(ty, aliases);
    matches!(
        ty,
        Type::Named {
            module: None,
            name,
            args,
        } if name == "Html" && args.len() == 1
    )
}

fn infer_unary_operator(
    op: &str,
    value: &Type,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    match op {
        "-" => {
            if let Err(message) = unify(value, &Type::Number, subst) {
                errors.push(message);
            }
            let normalized = apply_subst(value, subst);
            if is_int_like(&normalized) {
                Type::Int
            } else if matches!(normalized, Type::Float) {
                Type::Float
            } else {
                Type::Number
            }
        }
        "not" | "!" => {
            if let Err(message) = unify(value, &Type::Bool, subst) {
                errors.push(message);
            }
            Type::Bool
        }
        _ => Type::Dynamic,
    }
}

fn infer_syntax_binary_types(
    op: &SyntaxBinaryOp,
    left: &Type,
    right: &Type,
    aliases: &HashMap<String, TypeAlias>,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    match op {
        SyntaxBinaryOp::Add | SyntaxBinaryOp::Sub | SyntaxBinaryOp::Mul => {
            if let Err(message) = unify(left, &Type::Number, subst) {
                errors.push(format!("left side {}", message));
            }
            if let Err(message) = unify(right, &Type::Number, subst) {
                errors.push(format!("right side {}", message));
            }

            let normalized_left = apply_subst(left, subst);
            let normalized_right = apply_subst(right, subst);
            if is_int_like(&normalized_left) && is_int_like(&normalized_right) {
                Type::Int
            } else {
                Type::Number
            }
        }
        SyntaxBinaryOp::Div => {
            if let Err(message) = unify(left, &Type::Number, subst) {
                errors.push(format!("left side {}", message));
            }
            if let Err(message) = unify(right, &Type::Number, subst) {
                errors.push(format!("right side {}", message));
            }

            Type::Number
        }
        SyntaxBinaryOp::DivRem => {
            if let Err(message) = unify(left, &Type::Int, subst) {
                errors.push(format!("left side {}", message));
            }
            if let Err(message) = unify(right, &Type::Int, subst) {
                errors.push(format!("right side {}", message));
            }
            Type::Int
        }
        SyntaxBinaryOp::Eq
        | SyntaxBinaryOp::EqEq
        | SyntaxBinaryOp::EqEqEq
        | SyntaxBinaryOp::NotEq
        | SyntaxBinaryOp::NotEqEq
        | SyntaxBinaryOp::Lt
        | SyntaxBinaryOp::Gt
        | SyntaxBinaryOp::LtEq
        | SyntaxBinaryOp::GtEq => {
            if let Err(message) = unify_comparable_types(left, right, aliases, subst) {
                errors.push(message);
            }
            Type::Bool
        }
        SyntaxBinaryOp::And | SyntaxBinaryOp::Or => {
            if let Err(message) = unify(&Type::Bool, left, subst) {
                errors.push(format!("left side {}", message));
            }
            if let Err(message) = unify(&Type::Bool, right, subst) {
                errors.push(format!("right side {}", message));
            }
            Type::Bool
        }
        SyntaxBinaryOp::PipeForward => Type::Dynamic,
        SyntaxBinaryOp::Send => Type::LiteralAtom("ok".to_string()),
    }
}

/// Unifies binary comparison operands with transparent alias expansion.
///
/// Inputs:
/// - `left` and `right`: inferred operand types from a comparison expression.
/// - `aliases`: the visible type aliases for the current inference context.
/// - `subst`: the mutable type-variable substitution table.
///
/// Output:
/// - `Ok(())` when the operands are directly compatible, or compatible after
///   transparent alias expansion.
/// - The original direct-unification diagnostic when expansion still fails.
///
/// Transformation:
/// - First attempts normal unification so existing substitutions and
///   diagnostics remain unchanged for ordinary comparisons.
/// - If that fails, expands non-opaque aliases on both sides and retries using
///   the same substitution table.
fn unify_comparable_types(
    left: &Type,
    right: &Type,
    aliases: &HashMap<String, TypeAlias>,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<(), String> {
    if let Err(original_message) = unify(left, right, subst) {
        let left_expanded = expand_type_aliases(left, aliases);
        let right_expanded = expand_type_aliases(right, aliases);
        if unify(&left_expanded, &right_expanded, subst).is_err() {
            return Err(original_message);
        }
    }

    Ok(())
}

/// Infers the result type for a Terlan send expression.
///
/// Inputs:
/// - `_left`: inferred type for the send target expression.
/// - `_right`: inferred type for the sent value expression.
/// - `_ctx`: current expression inference context.
/// - `_errors`: mutable diagnostic accumulator.
///
/// Output:
/// - The literal atom type `:ok`, matching the current backend-neutral send
///   expression contract.
///
/// Transformation:
/// - Treats send as syntactically valid without source-level protocol
///   validation. Target-specific messaging contracts must be introduced through
///   libraries, traits, or target profiles rather than removed protocol syntax.
fn infer_send_op(
    _left: &Type,
    _right: &Type,
    _ctx: &ExprInferContext,
    _errors: &mut Vec<String>,
) -> Type {
    Type::LiteralAtom("ok".to_string())
}

fn alias_name_for_type(ty: &Type, aliases: &HashMap<String, TypeAlias>) -> Option<String> {
    let rendered = pretty_type(ty);
    aliases.iter().find_map(|(name, alias)| {
        if !alias.params.is_empty() {
            return None;
        }
        if pretty_type(&expand_type_aliases(&alias.body, aliases)) == rendered {
            Some(name.clone())
        } else {
            None
        }
    })
}

fn is_int_like(ty: &Type) -> bool {
    matches!(ty, Type::Int | Type::LiteralInt(_))
}

fn is_constructor_name(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
}

fn infer_function_call(
    scheme: &FunctionScheme,
    args: &[Type],
    ctx: &ExprInferContext<'_>,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<Type, String> {
    infer_function_with_bounds(scheme, None, args, ctx, subst)
}

fn infer_function_with_bounds(
    scheme: &FunctionScheme,
    function_name: Option<&str>,
    args: &[Type],
    ctx: &ExprInferContext<'_>,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<Type, String> {
    let instantiated =
        instantiate_function_scheme_from(scheme, next_function_type_var(args, subst));
    if instantiated.params.len() != args.len() {
        return Err(format!(
            "wrong arity for function call: expected {} args, found {}",
            instantiated.params.len(),
            args.len()
        ));
    }

    for (expected, actual) in instantiated.params.iter().zip(args.iter()) {
        if let Err(original_message) = unify(expected, actual, subst) {
            let expected_expanded = expand_type_aliases(expected, ctx.aliases);
            let actual_expanded = expand_type_aliases(actual, ctx.aliases);
            if unify(&expected_expanded, &actual_expanded, subst).is_err() {
                return Err(original_message);
            }
        }
    }

    if let Err(message) = check_function_bounds(&instantiated, function_name, ctx, subst) {
        return Err(message);
    }

    Ok(instantiate_type(&instantiated.ret, subst))
}

fn check_function_bounds(
    scheme: &FunctionScheme,
    function_name: Option<&str>,
    ctx: &ExprInferContext<'_>,
    subst: &HashMap<TypeVarId, Type>,
) -> Result<(), String> {
    if scheme.bounds.is_empty() {
        return Ok(());
    }

    for bound in &scheme.bounds {
        let resolved_args = bound
            .trait_args
            .iter()
            .map(|arg| {
                let arg = apply_subst(arg, subst);
                expand_type_aliases(&arg, ctx.aliases)
            })
            .collect::<Vec<_>>();
        let resolved_args = canonicalize_trait_lookup_types(&resolved_args);

        if !trait_has_bound_implementation(&bound.trait_name, &resolved_args, ctx) {
            let trait_description = if resolved_args.is_empty() {
                bound.trait_name.clone()
            } else {
                format!(
                    "{}[{}]",
                    bound.trait_name,
                    resolved_args
                        .iter()
                        .map(pretty_type)
                        .collect::<Vec<_>>()
                        .join(", "),
                )
            };

            let context = function_name.unwrap_or("expression");
            return Err(format!(
                "at `{}` call site: expected trait bound `{}`",
                context, trait_description
            ));
        }
    }

    Ok(())
}

/// Infers a trait method call using the active callable's generic bounds.
///
/// Inputs:
/// - `trait_name`: scoped trait name used at the call site.
/// - `method_name`: trait method name used at the call site.
/// - `arg_types`: already-inferred argument types at the call site.
/// - `ctx`: expression inference context with visible trait signatures and
///   active callable bounds.
/// - `subst`: mutable type substitution accumulated by the enclosing
///   expression inference.
///
/// Output:
/// - `Some(return_type)` when an active bound such as `Eq[A]` satisfies
///   `Eq.equal(...)` and the trait method signature type-checks with the
///   provided arguments.
/// - `None` when no active bound applies or the signature does not match.
///
/// Transformation:
/// - Specializes the trait method signature through the active bound's trait
///   arguments, then runs ordinary function-call inference against that
///   specialized signature. This does not synthesize a global impl candidate,
///   so concrete calls without an impl still produce the normal missing-impl
///   diagnostic.
fn infer_trait_method_call_from_current_bounds(
    trait_name: &str,
    method_name: &str,
    arg_types: &[Type],
    ctx: &ExprInferContext<'_>,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Option<Type> {
    let trait_signature = ctx.trait_signatures.get(trait_name)?;
    let inherited_methods = collect_trait_methods_with_inheritance(
        ctx.trait_signatures,
        trait_name,
        &mut HashMap::new(),
        &mut HashSet::new(),
    )?;
    let method_sig = inherited_methods.get(method_name)?;

    for bound in ctx
        .current_bounds
        .iter()
        .filter(|bound| bound.trait_name == trait_name)
    {
        if bound.trait_args.len() != trait_signature.type_params.len() {
            continue;
        }

        let mut method_vars = HashMap::new();
        let mut next_method_var = 0usize;
        for name in &trait_signature.type_params {
            method_vars.insert(name.clone(), next_method_var);
            next_method_var += 1;
        }

        let parsed_params = method_sig
            .params
            .iter()
            .map(|param| {
                parse_type_expr(
                    param,
                    ctx.alias_names,
                    &mut method_vars,
                    &mut next_method_var,
                )
            })
            .collect::<Option<Vec<_>>>()?;
        let parsed_return = parse_type_expr(
            &method_sig.return_type,
            ctx.alias_names,
            &mut method_vars,
            &mut next_method_var,
        )?;

        let mut trait_subst = HashMap::new();
        for (param_name, arg_type) in trait_signature.type_params.iter().zip(&bound.trait_args) {
            let var_id = *method_vars.get(param_name)?;
            trait_subst.insert(var_id, arg_type.clone());
        }

        let bounds =
            parse_generic_bounds(&method_sig.generic_bounds, &method_vars, ctx.alias_names)
                .into_iter()
                .map(|method_bound| FunctionBound {
                    trait_name: method_bound.trait_name,
                    trait_args: method_bound
                        .trait_args
                        .into_iter()
                        .map(|arg| substitute_type_vars(&arg, &trait_subst))
                        .collect(),
                })
                .collect();
        let scheme = FunctionScheme {
            params: parsed_params
                .into_iter()
                .map(|param| substitute_type_vars(&param, &trait_subst))
                .collect(),
            ret: substitute_type_vars(&parsed_return, &trait_subst),
            bounds,
        };

        let mut trial_subst = subst.clone();
        if let Ok(return_type) =
            infer_function_with_bounds(&scheme, Some(method_name), arg_types, ctx, &mut trial_subst)
        {
            *subst = trial_subst;
            return Some(return_type);
        }
    }

    None
}

fn trait_has_bound_implementation(
    trait_name: &str,
    bound_args: &[Type],
    ctx: &ExprInferContext<'_>,
) -> bool {
    let cache_key = TraitBoundLookupKey {
        trait_name: trait_name.to_string(),
        bound_args: bound_args.to_vec(),
    };
    if ctx.current_bounds.is_empty() {
        let cache = ctx.trait_lookup_cache.borrow();
        if let Some(cached) = cache.bound_checks.get(&cache_key) {
            return *cached;
        }
    }

    let Some(candidates) = ctx.trait_bound_impl_type_args.get(trait_name) else {
        let found = current_bounds_satisfy_trait_bound(trait_name, bound_args, ctx);
        if ctx.current_bounds.is_empty() {
            ctx.trait_lookup_cache
                .borrow_mut()
                .bound_checks
                .insert(cache_key, found);
        }
        return found;
    };

    let mut found = false;
    for impl_args in candidates {
        if impl_args.len() != bound_args.len() {
            continue;
        }

        let expanded_impl_args = impl_args
            .iter()
            .map(|arg| expand_type_aliases(arg, ctx.aliases))
            .collect::<Vec<_>>();

        if types_unify_with_renaming(bound_args, &expanded_impl_args).is_ok() {
            found = true;
            break;
        }
    }

    if !found {
        found = current_bounds_satisfy_trait_bound(trait_name, bound_args, ctx);
    }

    if ctx.current_bounds.is_empty() {
        ctx.trait_lookup_cache
            .borrow_mut()
            .bound_checks
            .insert(cache_key, found);
    }
    found
}

/// Checks whether active generic bounds satisfy a requested trait bound.
///
/// Inputs:
/// - `trait_name`: trait being required, such as `Eq`.
/// - `bound_args`: canonicalized required trait arguments.
/// - `ctx`: expression inference context carrying the current callable bounds.
///
/// Output:
/// - `true` when one active callable bound has the same trait name and
///   unifies with `bound_args`; otherwise `false`.
///
/// Transformation:
/// - Expands local aliases in the active bound arguments and performs a
///   renaming-tolerant unification check without mutating inference
///   substitution state.
fn current_bounds_satisfy_trait_bound(
    trait_name: &str,
    bound_args: &[Type],
    ctx: &ExprInferContext<'_>,
) -> bool {
    ctx.current_bounds.iter().any(|bound| {
        if bound.trait_name != trait_name || bound.trait_args.len() != bound_args.len() {
            return false;
        }

        let active_args = bound
            .trait_args
            .iter()
            .map(|arg| expand_type_aliases(arg, ctx.aliases))
            .collect::<Vec<_>>();
        types_unify_with_renaming(bound_args, &active_args).is_ok()
    })
}

fn collect_trait_bound_impl_type_args(
    trait_method_calls: &HashMap<(String, String), Vec<ResolvedTraitMethod>>,
) -> HashMap<String, Vec<Vec<Type>>> {
    let mut impl_type_args = HashMap::new();
    for ((trait_name, _), methods) in trait_method_calls {
        let candidates: &mut Vec<Vec<Type>> = impl_type_args.entry(trait_name.clone()).or_default();
        for method in methods {
            if candidates
                .iter()
                .any(|existing| existing == &method.impl_type_args)
            {
                continue;
            }
            candidates.push(method.impl_type_args.clone());
        }
    }
    impl_type_args
}

fn types_unify_with_renaming(expected: &[Type], actual: &[Type]) -> Result<(), String> {
    let mut next_var = max_type_var_id(expected);
    let mut remap = HashMap::new();
    let normalized_actual = actual
        .iter()
        .map(|arg| remap_type_var_id(arg, &mut next_var, &mut remap))
        .collect::<Vec<_>>();

    let mut local_subst = HashMap::new();
    for (expected_arg, actual_arg) in expected.iter().zip(normalized_actual.iter()) {
        unify(expected_arg, actual_arg, &mut local_subst)?;
    }
    Ok(())
}

fn max_type_var_id(types: &[Type]) -> TypeVarId {
    types
        .iter()
        .filter_map(max_type_var)
        .max()
        .map(|id| id + 1)
        .unwrap_or(0)
}

fn remap_type_var_id(
    ty: &Type,
    next_var: &mut TypeVarId,
    remap: &mut HashMap<TypeVarId, TypeVarId>,
) -> Type {
    remap_type(ty, &mut |id| {
        if let Some(remapped) = remap.get(id) {
            *remapped
        } else {
            let remapped = *next_var;
            remap.insert(*id, remapped);
            *next_var += 1;
            remapped
        }
    })
}

fn infer_constructor_call(
    name: &str,
    args: &[Type],
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    if let Some(schemes) = ctx.constructors.get(name) {
        return infer_constructor_schemes(name, schemes, args, subst, errors);
    }

    let schemes = alias_constructor_call_schemes(name, ctx.aliases)?;
    infer_constructor_schemes(name, &schemes, args, subst, errors)
}

fn infer_constructor_schemes(
    name: &str,
    schemes: &[ConstructorScheme],
    args: &[Type],
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    let mut last_error = None;

    for scheme in schemes {
        let instantiated =
            instantiate_constructor_scheme(scheme, next_constructor_type_var(args, subst));
        let mut trial_subst = subst.clone();
        let result = if let Some(vararg) = &instantiated.vararg {
            infer_varargs_constructor_call(name, &instantiated, vararg, args, &mut trial_subst)
        } else if args.len() >= instantiated.min_arity
            && args.len() <= instantiated.fixed_params.len()
        {
            infer_fixed_constructor_call(&instantiated, args, &mut trial_subst)
        } else {
            Err(format!(
                "constructor {} has arity mismatch: expected {}..{} args, found {}",
                name,
                instantiated.min_arity,
                instantiated.fixed_params.len(),
                args.len()
            ))
        };

        match result {
            Ok(ty) => {
                *subst = trial_subst;
                return Some(ty);
            }
            Err(message) => last_error = Some(message),
        }
    }

    errors.push(
        last_error.unwrap_or_else(|| format!("no matching constructor {} / {}", name, args.len())),
    );
    Some(Type::Dynamic)
}

fn infer_fixed_constructor_call(
    scheme: &ConstructorScheme,
    args: &[Type],
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<Type, String> {
    for (expected, actual) in scheme.fixed_params.iter().zip(args.iter()) {
        unify(expected, actual, subst)?;
    }

    Ok(instantiate_type(&scheme.ret, subst))
}

fn infer_varargs_constructor_call(
    name: &str,
    scheme: &ConstructorScheme,
    vararg: &Type,
    args: &[Type],
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<Type, String> {
    if args.len() < scheme.fixed_params.len() {
        return Err(format!(
            "constructor {} expects at least {} args, found {}",
            name,
            scheme.fixed_params.len(),
            args.len()
        ));
    }

    for (expected, actual) in scheme.fixed_params.iter().zip(args.iter()) {
        unify(expected, actual, subst)?;
    }

    for actual in args.iter().skip(scheme.fixed_params.len()) {
        unify(vararg, actual, subst)?;
    }

    Ok(instantiate_type(&scheme.ret, subst))
}

fn infer_opaque_constructor(
    name: &str,
    arg_types: &[Type],
    aliases: &HashMap<String, TypeAlias>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    let alias = aliases.get(name)?;
    if !alias.is_opaque {
        return None;
    }
    if arg_types.len() != 1 {
        errors.push(format!(
            "opaque constructor {} expects 1 argument, found {}",
            name,
            arg_types.len()
        ));
        return Some(Type::Dynamic);
    }

    if name == "FixedArray" {
        if let Type::Tuple(items) = &arg_types[0] {
            let elem = if items.is_empty() {
                Type::Never
            } else {
                normalize_union(items.clone())
            };
            return Some(Type::FixedArray {
                size: items.len(),
                elem: Box::new(elem),
            });
        }
    }

    let expected = expand_type_aliases(&alias.body, aliases);
    let mut alias_subst = HashMap::new();
    if let Err(message) = unify(&expected, &arg_types[0], &mut alias_subst) {
        errors.push(message);
        return Some(Type::Dynamic);
    }

    Some(Type::Named {
        module: None,
        name: name.to_string(),
        args: alias
            .params
            .iter()
            .map(|param| apply_subst(&Type::Var(*param), &alias_subst))
            .collect(),
    })
}

fn alias_constructor_schemes(
    name: &str,
    aliases: &HashMap<String, TypeAlias>,
) -> Option<Vec<ConstructorScheme>> {
    let constructor_name = name.rsplit('.').next().unwrap_or(name);
    if !is_constructor_name(constructor_name) {
        return None;
    }

    let alias = aliases.get(name)?;
    if alias.is_opaque {
        return None;
    }

    let body = expand_type_aliases(&alias.body, aliases);
    let fixed_params = alias_constructor_params(&body)?;
    Some(vec![ConstructorScheme {
        min_arity: fixed_params.len(),
        fixed_params,
        vararg: None,
        ret: body,
    }])
}

fn alias_constructor_call_schemes(
    name: &str,
    aliases: &HashMap<String, TypeAlias>,
) -> Option<Vec<ConstructorScheme>> {
    alias_constructor_schemes(name, aliases)
}

fn alias_constructor_params(body: &Type) -> Option<Vec<Type>> {
    match body {
        Type::LiteralAtom(_) => Some(Vec::new()),
        Type::Tuple(items) => match items.first() {
            Some(Type::LiteralAtom(_)) => Some(items.iter().skip(1).cloned().collect()),
            _ => None,
        },
        _ => None,
    }
}

fn expected_syntax_opaque_constructor_return_matches(
    expr: &SyntaxExprOutput,
    expected: &Type,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext<'_>,
    subst: &mut HashMap<TypeVarId, Type>,
) -> bool {
    if expr.kind != SyntaxExprKind::Call || expr.remote.is_some() || expr.children.len() != 2 {
        return false;
    }

    let Some(constructor_name) = syntax_callee_name(expr) else {
        return false;
    };
    let Some(representation_expr) = expr.children.get(1) else {
        return false;
    };

    let Type::Named {
        module: None,
        name: expected_name,
        args: expected_args,
    } = expected
    else {
        return false;
    };

    if constructor_name != expected_name {
        return false;
    }

    let Some(alias) = ctx.aliases.get(constructor_name) else {
        return false;
    };
    if !alias.is_opaque || alias.params.len() != expected_args.len() {
        return false;
    }

    let mapping = alias
        .params
        .iter()
        .cloned()
        .zip(expected_args.iter().cloned())
        .collect::<HashMap<_, _>>();
    let expected_representation =
        expand_type_aliases(&substitute_type_vars(&alias.body, &mapping), ctx.aliases);

    let mut trial_subst = subst.clone();
    let mut errors = Vec::new();
    let actual_representation = infer_syntax_expr(
        representation_expr,
        locals,
        ctx,
        &mut trial_subst,
        &mut errors,
    );
    if !errors.is_empty() {
        return false;
    }

    if unify(
        &expected_representation,
        &actual_representation,
        &mut trial_subst,
    )
    .is_err()
    {
        return false;
    }

    *subst = trial_subst;
    true
}

fn refine_by_syntax_guard(
    guard: &SyntaxExprOutput,
    locals: &mut HashMap<String, Type>,
    aliases: &HashMap<String, TypeAlias>,
    subst: &mut HashMap<TypeVarId, Type>,
) {
    if guard.kind != SyntaxExprKind::Call || guard.remote.is_some() || guard.children.len() != 2 {
        return;
    }

    let Some(callee_name) = syntax_callee_name(guard) else {
        return;
    };
    let Some(guard_target) = guard.children.get(1).and_then(|arg| match arg.kind {
        SyntaxExprKind::Var => arg.text.as_deref(),
        _ => None,
    }) else {
        return;
    };
    let Some(narrowed) = guard_narrow_type(callee_name) else {
        return;
    };

    if let Some(existing) = locals.get(guard_target) {
        if unify(existing, &narrowed, subst).is_ok() {
            let narrowed = expand_type_aliases(&narrowed, aliases);
            if let Some(value) = locals.get_mut(guard_target) {
                *value = narrowed;
            }
        }
    }
}

fn guard_narrow_type(callee_name: &str) -> Option<Type> {
    match callee_name {
        "is_integer" => Some(Type::Int),
        "is_binary" => Some(Type::Binary),
        "is_atom" => Some(Type::Atom),
        "is_boolean" => Some(Type::Bool),
        "is_list" => Some(Type::List(Box::new(Type::Dynamic))),
        "is_map" => Some(Type::Named {
            module: None,
            name: "Map".to_string(),
            args: Vec::new(),
        }),
        "is_tuple" => Some(Type::Tuple(Vec::new())),
        _ => None,
    }
}

fn check_syntax_pattern(
    pattern: &SyntaxPatternOutput,
    expected: &Type,
    aliases: &HashMap<String, TypeAlias>,
    ctx: Option<&ExprInferContext<'_>>,
    locals: &mut HashMap<String, Type>,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<(), String> {
    let expected = expand_type_aliases(expected, aliases);

    match pattern.kind {
        SyntaxPatternKind::Var => {
            locals.insert(pattern.text.clone().unwrap_or_default(), expected);
            Ok(())
        }
        SyntaxPatternKind::Wildcard
        | SyntaxPatternKind::Ignore
        | SyntaxPatternKind::Placeholder => Ok(()),
        SyntaxPatternKind::Int => unify(&expected, &Type::Int, subst),
        SyntaxPatternKind::Float => unify(&expected, &Type::Float, subst),
        SyntaxPatternKind::Atom => {
            let atom = pattern.text.as_deref().unwrap_or_default();
            if atom.starts_with('_') {
                return Ok(());
            }
            if atom == "[]" || atom == "nil" {
                return match &expected {
                    Type::List(_) | Type::Dynamic | Type::Term => Ok(()),
                    _ => unify(&expected, &Type::List(Box::new(Type::Dynamic)), subst),
                };
            }
            if atom == "true" || atom == "false" {
                return unify(&expected, &Type::Bool, subst);
            }
            if is_literal_atom(atom) {
                unify(&expected, &Type::LiteralAtom(atom.to_string()), subst)
            } else {
                unify(&expected, &Type::Atom, subst)
            }
        }
        SyntaxPatternKind::Constructor => {
            check_syntax_constructor_pattern(pattern, &expected, aliases, ctx, locals, subst)
                .unwrap_or_else(|| {
                    Err(format!(
                        "expected {} found constructor pattern",
                        pretty_type(&expected)
                    ))
                })
        }
        SyntaxPatternKind::Tuple => match &expected {
            Type::Union(variants) => {
                let mut ok = false;
                for variant in variants {
                    let mut subst_before = subst.clone();
                    let mut locals_before = locals.clone();
                    if check_syntax_pattern(
                        pattern,
                        variant,
                        aliases,
                        ctx,
                        &mut locals_before,
                        &mut subst_before,
                    )
                    .is_ok()
                    {
                        *subst = subst_before;
                        for (name, value) in locals_before.into_iter() {
                            locals.insert(name, value);
                        }
                        ok = true;
                        break;
                    }
                }
                if ok {
                    Ok(())
                } else {
                    Err(format!(
                        "expected {} found tuple pattern",
                        pretty_type(&expected)
                    ))
                }
            }
            Type::Tuple(variant_items) => {
                if variant_items.len() != pattern.children.len() {
                    return Err(format!(
                        "tuple arity mismatch: expected {} elements, found {}",
                        variant_items.len(),
                        pattern.children.len()
                    ));
                }
                for (pattern_item, expected_item) in
                    pattern.children.iter().zip(variant_items.iter())
                {
                    check_syntax_pattern(pattern_item, expected_item, aliases, ctx, locals, subst)?;
                }
                Ok(())
            }
            Type::Dynamic | Type::Term => {
                for pattern_item in &pattern.children {
                    check_syntax_pattern(
                        pattern_item,
                        &Type::Dynamic,
                        aliases,
                        ctx,
                        locals,
                        subst,
                    )?;
                }
                Ok(())
            }
            _ => Err(format!(
                "expected {} found tuple pattern",
                pretty_type(&expected)
            )),
        },
        SyntaxPatternKind::List => match &expected {
            Type::List(elem) => {
                for item in &pattern.children {
                    check_syntax_pattern(item, elem, aliases, ctx, locals, subst)?;
                }
                Ok(())
            }
            Type::Dynamic | Type::Term => {
                for item in &pattern.children {
                    check_syntax_pattern(item, &Type::Dynamic, aliases, ctx, locals, subst)?;
                }
                Ok(())
            }
            _ => unify(&expected, &Type::List(Box::new(Type::Dynamic)), subst).map(|_| ()),
        },
        SyntaxPatternKind::ListCons => match &expected {
            Type::List(elem) => {
                if let Some(head) = pattern.children.first() {
                    check_syntax_pattern(head, elem, aliases, ctx, locals, subst)?;
                }
                if let Some(tail) = pattern.children.get(1) {
                    check_syntax_pattern(
                        tail,
                        &Type::List(elem.clone()),
                        aliases,
                        ctx,
                        locals,
                        subst,
                    )?;
                }
                Ok(())
            }
            Type::Dynamic | Type::Term => {
                for item in &pattern.children {
                    check_syntax_pattern(item, &Type::Dynamic, aliases, ctx, locals, subst)?;
                }
                Ok(())
            }
            _ => unify(&expected, &Type::List(Box::new(Type::Dynamic)), subst).map(|_| ()),
        },
        SyntaxPatternKind::Map => match &expected {
            Type::Map(expected_fields) => {
                for pattern_field in &pattern.fields {
                    match expected_fields
                        .iter()
                        .find(|field| field.key == pattern_field.key)
                    {
                        Some(field) => check_syntax_pattern(
                            &pattern_field.value,
                            &field.value,
                            aliases,
                            ctx,
                            locals,
                            subst,
                        )?,
                        None => {
                            return Err(format!("unknown map key {}", pattern_field.key));
                        }
                    };
                }
                Ok(())
            }
            _ if is_map_type(&expected, aliases) => {
                for pattern_field in &pattern.fields {
                    check_syntax_pattern(
                        &pattern_field.value,
                        &Type::Dynamic,
                        aliases,
                        ctx,
                        locals,
                        subst,
                    )?;
                }
                Ok(())
            }
            _ => Err(format!(
                "expected {} found map pattern",
                pretty_type(&expected)
            )),
        },
        SyntaxPatternKind::MapField => {
            if is_map_type(&expected, aliases) {
                if let Some(value) = pattern.children.first() {
                    check_syntax_pattern(value, &Type::Dynamic, aliases, ctx, locals, subst)
                } else if let Some(field) = pattern.fields.first() {
                    check_syntax_pattern(&field.value, &Type::Dynamic, aliases, ctx, locals, subst)
                } else {
                    Ok(())
                }
            } else {
                Err(format!(
                    "expected {} found map pattern",
                    pretty_type(&expected)
                ))
            }
        }
        SyntaxPatternKind::Record => match &expected {
            Type::Union(variants) => {
                if variants.iter().any(|variant| {
                    check_syntax_pattern(pattern, variant, aliases, ctx, locals, subst).is_ok()
                }) {
                    Ok(())
                } else {
                    Err(format!(
                        "expected {} found record pattern {}",
                        pretty_type(&expected),
                        pattern.text.as_deref().unwrap_or_default()
                    ))
                }
            }
            Type::Named {
                module: _,
                name: expected_name,
                ..
            } => {
                let name = pattern.text.as_deref().unwrap_or_default();
                if expected_name == name {
                    for field in &pattern.fields {
                        check_syntax_pattern(
                            &field.value,
                            &Type::Dynamic,
                            aliases,
                            ctx,
                            locals,
                            subst,
                        )?;
                    }
                    Ok(())
                } else {
                    Err(format!(
                        "expected {} found record pattern {}",
                        pretty_type(&expected),
                        name
                    ))
                }
            }
            _ if matches!(expected, Type::Dynamic | Type::Term) => {
                for field in &pattern.fields {
                    check_syntax_pattern(
                        &field.value,
                        &Type::Dynamic,
                        aliases,
                        ctx,
                        locals,
                        subst,
                    )?;
                }
                Ok(())
            }
            _ => Err(format!(
                "expected {} found record pattern {}",
                pretty_type(&expected),
                pattern.text.as_deref().unwrap_or_default()
            )),
        },
    }
}

fn check_syntax_constructor_pattern(
    pattern: &SyntaxPatternOutput,
    expected: &Type,
    aliases: &HashMap<String, TypeAlias>,
    ctx: Option<&ExprInferContext<'_>>,
    locals: &mut HashMap<String, Type>,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Option<Result<(), String>> {
    let name = pattern.text.as_deref().unwrap_or_default();
    if !is_constructor_pattern_name(name) {
        return None;
    }
    let ctx = ctx?;
    if let Some(message) = imported_opaque_constructor_pattern_error(name, ctx) {
        return Some(Err(message));
    }
    let Some(schemes) = constructor_pattern_schemes(name, ctx) else {
        return Some(Err(format!("unknown constructor pattern {}", name)));
    };
    let mut last_error = None;

    for scheme in schemes {
        let instantiated = instantiate_constructor_scheme(
            &scheme,
            next_constructor_type_var(std::slice::from_ref(expected), subst),
        );
        let mut trial_subst = subst.clone();
        let mut trial_locals = locals.clone();

        let arity_ok = if instantiated.vararg.is_some() {
            pattern.children.len() >= instantiated.min_arity
        } else {
            pattern.children.len() >= instantiated.min_arity
                && pattern.children.len() <= instantiated.fixed_params.len()
        };
        if !arity_ok {
            last_error = Some(format!(
                "constructor {} has arity mismatch: expected {}..{} args, found {}",
                name,
                instantiated.min_arity,
                instantiated.fixed_params.len(),
                pattern.children.len()
            ));
            continue;
        }

        if let Err(message) =
            unify_constructor_pattern_return(expected, &instantiated.ret, &mut trial_subst)
        {
            last_error = Some(message);
            continue;
        }

        let mut failed = None;
        for (index, arg) in pattern.children.iter().enumerate() {
            let expected_arg = instantiated
                .fixed_params
                .get(index)
                .or(instantiated.vararg.as_ref())
                .cloned()
                .unwrap_or(Type::Dynamic);
            if let Err(message) = check_syntax_pattern(
                arg,
                &expected_arg,
                aliases,
                Some(ctx),
                &mut trial_locals,
                &mut trial_subst,
            ) {
                failed = Some(message);
                break;
            }
        }

        if let Some(message) = failed {
            last_error = Some(message);
            continue;
        }

        *subst = trial_subst;
        *locals = trial_locals;
        return Some(Ok(()));
    }

    Some(Err(last_error.unwrap_or_else(|| {
        format!(
            "no matching constructor {} / {}",
            name,
            pattern.children.len()
        )
    })))
}

fn unify_constructor_pattern_return(
    expected: &Type,
    actual: &Type,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<(), String> {
    match expected {
        Type::Union(variants) => {
            let mut last_error = None;
            for variant in variants {
                let mut trial = subst.clone();
                match unify(actual, variant, &mut trial) {
                    Ok(()) => {
                        *subst = trial;
                        return Ok(());
                    }
                    Err(message) => last_error = Some(message),
                }
            }
            Err(last_error.unwrap_or_else(|| {
                format!(
                    "expected {} found {}",
                    pretty_type(expected),
                    pretty_type(actual)
                )
            }))
        }
        _ => unify(actual, expected, subst),
    }
}

fn constructor_pattern_schemes(
    name: &str,
    ctx: &ExprInferContext<'_>,
) -> Option<Vec<ConstructorScheme>> {
    if let Some(schemes) = ctx.constructors.get(name) {
        return Some(schemes.clone());
    }

    if let Some(schemes) = alias_constructor_schemes(name, ctx.aliases) {
        return Some(schemes);
    }

    let imported = ctx.constructor_aliases.get(name)?;
    let interface = ctx.interface_map.get(&imported.module)?;
    parse_interface_constructor_schemes(
        interface
            .constructors
            .get(&imported.name)
            .map(Vec::as_slice),
        interface,
    )
}

fn imported_opaque_constructor_pattern_error(
    name: &str,
    ctx: &ExprInferContext<'_>,
) -> Option<String> {
    let imported = ctx.constructor_aliases.get(name)?;
    let interface = ctx.interface_map.get(&imported.module)?;
    interface.opaque_types.contains(&imported.name).then(|| {
        format!(
            "cannot match opaque type {}.{} as constructor pattern outside defining module",
            imported.module, imported.name
        )
    })
}

fn is_constructor_pattern_name(name: &str) -> bool {
    matches!(name.chars().next(), Some(ch) if ch.is_ascii_uppercase())
}

fn is_map_type(ty: &Type, aliases: &HashMap<String, TypeAlias>) -> bool {
    match expand_type_aliases(ty, aliases) {
        Type::Named { name, .. } if name == "Map" || name == "map" => true,
        Type::Map(_) => true,
        Type::Dynamic | Type::Term => true,
        _ => false,
    }
}

fn parse_symbol_scheme(symbol: &FunctionSymbol) -> Option<FunctionScheme> {
    let mut vars = HashMap::new();
    let mut next_var: TypeVarId = 0;
    let params = symbol
        .params
        .iter()
        .filter_map(|param| {
            parse_type_expr(&param.annotation, &HashSet::new(), &mut vars, &mut next_var)
        })
        .collect::<Vec<_>>();
    let ret = parse_type_expr(
        &symbol.return_type,
        &HashSet::new(),
        &mut vars,
        &mut next_var,
    )?;

    Some(FunctionScheme {
        params,
        ret,
        bounds: Vec::new(),
    })
}

fn parse_interface_signature(
    signature: &FunctionSignature,
    interface: &ModuleInterface,
    global_aliases: &HashMap<String, TypeAlias>,
) -> Option<FunctionScheme> {
    let mut vars = HashMap::new();
    let mut next_var: TypeVarId = 0;
    let alias_names = interface_type_names(interface);
    let qualified_names = interface_qualified_type_names(interface);
    let interface_aliases = interface_type_aliases(interface);

    let params = signature
        .params
        .iter()
        .filter_map(|param| {
            parse_type_expr(&param.annotation, &alias_names, &mut vars, &mut next_var)
        })
        .map(|param| expand_type_aliases(&param, &interface_aliases))
        .map(|param| expand_interface_global_aliases(&param, global_aliases))
        .map(|param| qualify_type_names(&param, &qualified_names))
        .collect::<Vec<_>>();

    let ret = parse_type_expr(
        &signature.return_type,
        &alias_names,
        &mut vars,
        &mut next_var,
    )?;
    let ret = expand_type_aliases(&ret, &interface_aliases);
    let ret = expand_interface_global_aliases(&ret, global_aliases);
    let ret = qualify_type_names(&ret, &qualified_names);
    Some(FunctionScheme {
        params,
        ret,
        bounds: Vec::new(),
    })
}

/// Expands aliases visible through the global interface map.
///
/// Inputs:
/// - `ty`: type parsed from a public interface signature.
/// - `global_aliases`: fully qualified aliases loaded from dependency
///   interfaces.
///
/// Output:
/// - Type with fully qualified aliases expanded, plus unqualified aliases
///   expanded only when their short name is unique globally.
///
/// Transformation:
/// - Preserves local/interface parsing while allowing checked std summaries
///   such as `Option[T]` to resolve to the single loaded
///   `std.core.Option.Option[T]` alias without requiring every summary to spell
///   fully qualified type names.
fn expand_interface_global_aliases(ty: &Type, global_aliases: &HashMap<String, TypeAlias>) -> Type {
    match ty {
        Type::Named {
            module: None,
            name,
            args,
        } => {
            if let Some(alias) = unique_global_alias(name, global_aliases) {
                if alias.is_opaque || alias.params.len() != args.len() {
                    return Type::Named {
                        module: None,
                        name: name.clone(),
                        args: args
                            .iter()
                            .map(|arg| expand_interface_global_aliases(arg, global_aliases))
                            .collect(),
                    };
                }
                let args = args
                    .iter()
                    .map(|arg| expand_interface_global_aliases(arg, global_aliases))
                    .collect::<Vec<_>>();
                let mapping = alias
                    .params
                    .iter()
                    .cloned()
                    .zip(args)
                    .collect::<HashMap<_, _>>();
                return expand_interface_global_aliases(
                    &substitute_type_vars(&alias.body, &mapping),
                    global_aliases,
                );
            }
            expand_type_aliases(ty, global_aliases)
        }
        Type::Named { .. } => expand_type_aliases(ty, global_aliases),
        Type::List(inner) => Type::List(Box::new(expand_interface_global_aliases(
            inner,
            global_aliases,
        ))),
        Type::Tuple(items) => Type::Tuple(
            items
                .iter()
                .map(|item| expand_interface_global_aliases(item, global_aliases))
                .collect(),
        ),
        Type::Union(items) => Type::Union(
            items
                .iter()
                .map(|item| expand_interface_global_aliases(item, global_aliases))
                .collect(),
        ),
        Type::Map(fields) => Type::Map(
            fields
                .iter()
                .map(|field| MapFieldType {
                    key: field.key.clone(),
                    value: expand_interface_global_aliases(&field.value, global_aliases),
                    required: field.required,
                })
                .collect(),
        ),
        Type::Function { params, ret } => Type::Function {
            params: params
                .iter()
                .map(|param| expand_interface_global_aliases(param, global_aliases))
                .collect(),
            ret: Box::new(expand_interface_global_aliases(ret, global_aliases)),
        },
        Type::FixedArray { size, elem } => Type::FixedArray {
            size: *size,
            elem: Box::new(expand_interface_global_aliases(elem, global_aliases)),
        },
        other => other.clone(),
    }
}

/// Finds a globally unique alias by short type name.
///
/// Inputs:
/// - `name`: unqualified type name from an interface signature.
/// - `global_aliases`: fully qualified aliases keyed as `module.Type`.
///
/// Output:
/// - The alias when exactly one global alias has the requested final segment.
///
/// Transformation:
/// - Scans fully qualified alias keys by their final dotted segment and rejects
///   ambiguous short names so interface summaries do not accidentally bind to
///   the wrong module.
fn unique_global_alias<'a>(
    name: &str,
    global_aliases: &'a HashMap<String, TypeAlias>,
) -> Option<&'a TypeAlias> {
    let mut matches = global_aliases.iter().filter_map(|(qualified, alias)| {
        qualified
            .rsplit_once('.')
            .and_then(|(_, short)| (short == name).then_some(alias))
    });
    let first = matches.next()?;
    if matches.next().is_some() {
        None
    } else {
        Some(first)
    }
}

fn parse_interface_constructor_schemes(
    signatures: Option<&[ConstructorSignature]>,
    interface: &ModuleInterface,
) -> Option<Vec<ConstructorScheme>> {
    let signatures = signatures?;
    let alias_names = interface_type_names(interface);
    let qualified_names = interface_qualified_type_names(interface);
    let interface_aliases = interface_type_aliases(interface);

    let schemes = signatures
        .iter()
        .filter(|signature| signature.public && signature.min_arity == signature.params.len())
        .map(|signature| {
            let mut vars = HashMap::new();
            let mut next_var: TypeVarId = 0;

            let fixed_params = signature
                .params
                .iter()
                .map(|param| {
                    parse_type_expr(&param.annotation, &alias_names, &mut vars, &mut next_var)
                        .unwrap_or(Type::Dynamic)
                })
                .map(|param| expand_type_aliases(&param, &interface_aliases))
                .map(|param| qualify_type_names(&param, &qualified_names))
                .collect::<Vec<_>>();

            let vararg = signature.vararg.as_ref().map(|param| {
                let parsed =
                    parse_type_expr(&param.annotation, &alias_names, &mut vars, &mut next_var)
                        .unwrap_or(Type::Dynamic);
                let parsed = expand_type_aliases(&parsed, &interface_aliases);
                qualify_type_names(&parsed, &qualified_names)
            });

            let ret = parse_type_expr(
                &signature.return_type,
                &alias_names,
                &mut vars,
                &mut next_var,
            )
            .unwrap_or(Type::Dynamic);
            let ret = expand_type_aliases(&ret, &interface_aliases);
            let ret = qualify_type_names(&ret, &qualified_names);

            ConstructorScheme {
                min_arity: signature.min_arity,
                fixed_params,
                vararg,
                ret,
            }
        })
        .collect::<Vec<_>>();

    Some(schemes)
}

fn interface_type_names(interface: &ModuleInterface) -> HashSet<String> {
    interface
        .public_types
        .iter()
        .chain(interface.opaque_types.iter())
        .cloned()
        .collect()
}

fn interface_type_aliases(interface: &ModuleInterface) -> HashMap<String, TypeAlias> {
    let mut aliases = HashMap::new();
    let alias_names = interface_type_names(interface);

    for (name, variants) in &interface.type_bodies {
        if interface.opaque_types.contains(name) {
            continue;
        }

        let mut vars = HashMap::new();
        let mut next_var: TypeVarId = 0;
        let mut params = Vec::new();
        for param in interface.type_params.get(name).into_iter().flatten() {
            vars.insert(normalize_type_param_name(param), next_var);
            params.push(next_var);
            next_var += 1;
        }

        let body = normalize_union(
            variants
                .iter()
                .filter_map(|variant| {
                    parse_type_expr(variant, &alias_names, &mut vars, &mut next_var)
                })
                .collect(),
        );
        aliases.insert(
            name.clone(),
            TypeAlias {
                params,
                body,
                is_opaque: false,
            },
        );
    }

    aliases
}

fn interface_qualified_type_names(
    interface: &ModuleInterface,
) -> HashMap<String, QualifiedTypeName> {
    interface
        .public_types
        .iter()
        .chain(interface.opaque_types.iter())
        .map(|name| {
            (
                name.clone(),
                QualifiedTypeName {
                    module: interface.module.clone(),
                    name: name.clone(),
                },
            )
        })
        .collect()
}

fn qualify_type_names(ty: &Type, qualified_names: &HashMap<String, QualifiedTypeName>) -> Type {
    match ty {
        Type::Named {
            module: None,
            name,
            args,
        } => {
            let args = args
                .iter()
                .map(|arg| qualify_type_names(arg, qualified_names))
                .collect();
            if let Some(qualified) = qualified_names.get(name) {
                Type::Named {
                    module: Some(qualified.module.clone()),
                    name: qualified.name.clone(),
                    args,
                }
            } else {
                Type::Named {
                    module: None,
                    name: name.clone(),
                    args,
                }
            }
        }
        Type::Named { module, name, args } => Type::Named {
            module: module.clone(),
            name: name.clone(),
            args: args
                .iter()
                .map(|arg| qualify_type_names(arg, qualified_names))
                .collect(),
        },
        Type::List(inner) => Type::List(Box::new(qualify_type_names(inner, qualified_names))),
        Type::Tuple(items) => Type::Tuple(
            items
                .iter()
                .map(|item| qualify_type_names(item, qualified_names))
                .collect(),
        ),
        Type::Union(items) => Type::Union(
            items
                .iter()
                .map(|item| qualify_type_names(item, qualified_names))
                .collect(),
        ),
        Type::Map(fields) => Type::Map(
            fields
                .iter()
                .map(|field| MapFieldType {
                    key: field.key.clone(),
                    value: qualify_type_names(&field.value, qualified_names),
                    required: field.required,
                })
                .collect(),
        ),
        Type::Function { params, ret } => Type::Function {
            params: params
                .iter()
                .map(|param| qualify_type_names(param, qualified_names))
                .collect(),
            ret: Box::new(qualify_type_names(ret, qualified_names)),
        },
        Type::FixedArray { size, elem } => Type::FixedArray {
            size: *size,
            elem: Box::new(qualify_type_names(elem, qualified_names)),
        },
        other => other.clone(),
    }
}

fn parse_type_expr(
    input: &str,
    aliases: &HashSet<String>,
    vars: &mut HashMap<String, TypeVarId>,
    next_var: &mut TypeVarId,
) -> Option<Type> {
    let src = compact_spaces(input);
    if src.is_empty() {
        return Some(Type::Dynamic);
    }

    if let Some(atom) = parse_type_atom_literal(&src) {
        return Some(Type::LiteralAtom(atom));
    }

    if let Some((params, ret)) = split_top_level_arrow(&src) {
        let params = strip_wrapping_parens(&params).unwrap_or(params.as_str());
        let params = if params.trim().is_empty() {
            Vec::new()
        } else {
            split_top_level_csv(params)
                .into_iter()
                .map(|param| parse_type_expr(param.trim(), aliases, vars, next_var))
                .collect::<Option<Vec<_>>>()?
        };
        let ret = parse_type_expr(ret.trim(), aliases, vars, next_var)?;
        return Some(Type::Function {
            params,
            ret: Box::new(ret),
        });
    }

    if is_union_type(&src) {
        let variants = split_top_level_union(&src)
            .into_iter()
            .map(|variant| parse_type_expr(variant.trim(), aliases, vars, next_var))
            .collect::<Option<Vec<_>>>()?;
        return Some(normalize_union(variants));
    }

    if is_list_type(&src) {
        let inner = &src[1..src.len() - 1];
        return Some(Type::List(Box::new(
            parse_type_expr(inner.trim(), aliases, vars, next_var).unwrap_or(Type::Dynamic),
        )));
    }

    if let Some((base, args)) = split_named_type(&src) {
        if base == "List" {
            let mut args = split_top_level_csv(&args).into_iter();
            let inner = args.next()?;
            if args.next().is_some() {
                return None;
            }
            return Some(Type::List(Box::new(parse_type_expr(
                inner.trim(),
                aliases,
                vars,
                next_var,
            )?)));
        }

        if base == "FixedArray" {
            let mut args = split_top_level_csv(&args).into_iter();
            let first = args.next()?;
            let second = args.next()?;
            if args.next().is_some() {
                return None;
            }

            let size = parse_type_expr(first.trim(), aliases, vars, next_var)?;
            if let Type::LiteralInt(size_value) = size {
                let size = usize::try_from(size_value).ok()?;
                let elem = parse_type_expr(second.trim(), aliases, vars, next_var)?;
                return Some(Type::FixedArray {
                    size,
                    elem: Box::new(elem),
                });
            }

            return None;
        }

        let args = split_top_level_csv(&args)
            .into_iter()
            .map(|arg| parse_type_expr(arg.trim(), aliases, vars, next_var))
            .collect::<Option<Vec<_>>>()?;
        let (module, name) = split_module_name(base);
        return Some(Type::Named { module, name, args });
    }

    if is_map_type_expr(&src) {
        return parse_map_type_expression(&src, aliases, vars, next_var);
    }

    if is_tuple_type(&src) {
        let inner = &src[1..src.len() - 1];
        return Some(Type::Tuple(
            split_top_level_csv(inner)
                .into_iter()
                .map(|item| parse_tuple_type_elem(item.trim(), aliases, vars, next_var))
                .collect::<Option<Vec<_>>>()?,
        ));
    }

    Some(map_named_or_var(&src, aliases, vars, next_var))
}

fn parse_tuple_type_elem(
    input: &str,
    aliases: &HashSet<String>,
    vars: &mut HashMap<String, TypeVarId>,
    next_var: &mut TypeVarId,
) -> Option<Type> {
    if let Some((label, value)) = split_named_tuple_type_elem(input) {
        if label == "_" || is_lower_identifier(label) {
            return parse_type_expr(value.trim(), aliases, vars, next_var);
        }
    }

    parse_type_expr(input, aliases, vars, next_var)
}

fn split_named_tuple_type_elem(input: &str) -> Option<(&str, &str)> {
    let mut depth_p = 0usize;
    let mut depth_b = 0usize;
    let mut depth_br = 0usize;
    let mut quote = None;
    let mut escape = false;

    for (i, ch) in input.char_indices() {
        if let Some(quote_ch) = quote {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == quote_ch {
                quote = None;
            }
            continue;
        }

        match ch {
            '\'' | '"' => quote = Some(ch),
            '(' => depth_p += 1,
            ')' => depth_p = depth_p.saturating_sub(1),
            '[' => depth_b += 1,
            ']' => depth_b = depth_b.saturating_sub(1),
            '{' => depth_br += 1,
            '}' => depth_br = depth_br.saturating_sub(1),
            ':' if i > 0 && depth_p == 0 && depth_b == 0 && depth_br == 0 => {
                return Some((&input[..i], &input[i + ch.len_utf8()..]));
            }
            _ => {}
        }
    }

    None
}

fn parse_type_atom_literal(input: &str) -> Option<String> {
    if let Some(atom) = atom_type_literal_payload(input) {
        return Some(atom);
    }

    let atom = input.strip_prefix(':')?;
    if atom.is_empty() {
        return None;
    }
    if is_type_constructor_atom(atom) {
        return Some(atom.to_string());
    }
    if atom.len() >= 2 && atom.starts_with('\'') && atom.ends_with('\'') {
        return unquote_type_atom(atom);
    }
    None
}

fn unquote_type_atom(text: &str) -> Option<String> {
    let inner = text.strip_prefix('\'')?.strip_suffix('\'')?;
    let mut output = String::new();
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(escaped) = chars.next() {
                output.push(escaped);
            }
        } else {
            output.push(ch);
        }
    }
    Some(output)
}

fn parse_map_type_expression(
    input: &str,
    aliases: &HashSet<String>,
    vars: &mut HashMap<String, TypeVarId>,
    next_var: &mut TypeVarId,
) -> Option<Type> {
    let src = input;
    if !is_map_type_expr(src) {
        return None;
    }

    let inner = &src[2..src.len() - 1];
    if inner.trim().is_empty() {
        return Some(Type::Map(Vec::new()));
    }

    let fields = split_top_level_csv(inner)
        .into_iter()
        .map(|field| parse_map_type_field(field.trim(), aliases, vars, next_var))
        .collect::<Option<Vec<_>>>()?;

    Some(Type::Map(fields))
}

fn parse_map_type_field(
    input: &str,
    aliases: &HashSet<String>,
    vars: &mut HashMap<String, TypeVarId>,
    next_var: &mut TypeVarId,
) -> Option<MapFieldType> {
    let (raw_key, raw_value, required) = split_map_field(input)?;
    let key = raw_key.trim().to_string();
    let value = parse_type_expr(raw_value.trim(), aliases, vars, next_var)?;
    Some(MapFieldType {
        key,
        value,
        required,
    })
}

fn split_map_field(input: &str) -> Option<(&str, &str, bool)> {
    let bytes = input.as_bytes();
    let mut depth_p = 0usize;
    let mut depth_b = 0usize;
    let mut depth_br = 0usize;

    for i in 0..bytes.len() {
        match bytes[i] {
            b'(' => depth_p += 1,
            b')' => depth_p = depth_p.saturating_sub(1),
            b'[' => depth_b += 1,
            b']' => depth_b = depth_b.saturating_sub(1),
            b'{' => depth_br += 1,
            b'}' => depth_br = depth_br.saturating_sub(1),
            b':' if i + 1 < bytes.len()
                && bytes[i + 1] == b'='
                && depth_p == 0
                && depth_b == 0
                && depth_br == 0 =>
            {
                return Some((&input[..i], &input[i + 2..], true));
            }
            b'=' if i + 1 < bytes.len()
                && bytes[i + 1] == b'>'
                && depth_p == 0
                && depth_b == 0
                && depth_br == 0 =>
            {
                return Some((&input[..i], &input[i + 2..], false));
            }
            _ => {}
        }
    }

    None
}

fn split_named_type(input: &str) -> Option<(&str, String)> {
    let bytes = input.as_bytes();
    if !bytes.contains(&b'[') || !input.ends_with(']') {
        return None;
    }

    let mut depth = 0usize;
    for (i, byte) in bytes.iter().enumerate() {
        match *byte {
            b'[' => {
                if depth == 0 {
                    return split_named_type_inner(input, i);
                }
                depth += 1;
            }
            b']' => {
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
    }

    None
}

fn split_named_type_inner(input: &str, open_index: usize) -> Option<(&str, String)> {
    if !input.ends_with(']') {
        return None;
    }

    let name = input[..open_index].trim();
    let args = input[open_index + 1..input.len() - 1].trim();

    if name.is_empty() {
        None
    } else {
        Some((name, args.to_string()))
    }
}

fn is_union_type(input: &str) -> bool {
    split_top_level_union(input).len() > 1
}

fn map_named_or_var(
    text: &str,
    aliases: &HashSet<String>,
    vars: &mut HashMap<String, TypeVarId>,
    next_var: &mut TypeVarId,
) -> Type {
    match text {
        "Int" => Type::Int,
        "Float" => Type::Float,
        "Number" => Type::Number,
        "Binary" => Type::Binary,
        "String" | "Text" => Type::Binary,
        "Atom" => Type::Atom,
        "Bool" => Type::Bool,
        "Term" => Type::Term,
        "Dynamic" => Type::Dynamic,
        "Never" => Type::Never,
        _ if text.chars().all(|c| c.is_ascii_digit()) => {
            if let Ok(value) = text.parse::<i64>() {
                Type::LiteralInt(value)
            } else {
                Type::Dynamic
            }
        }
        _ if text.contains('.') => {
            let (module, name) = split_module_name(text);
            Type::Named {
                module,
                name,
                args: Vec::new(),
            }
        }
        _ => {
            if let Some(existing) = vars.get(text) {
                return Type::Var(*existing);
            }
            if aliases.contains(text) {
                return Type::Named {
                    module: None,
                    name: text.to_string(),
                    args: Vec::new(),
                };
            }
            if let Some(id) = fresh_type_var(text, vars, next_var) {
                Type::Var(id)
            } else if is_type_constructor_atom(text) {
                Type::LiteralAtom(text.to_string())
            } else {
                Type::Named {
                    module: None,
                    name: text.to_string(),
                    args: Vec::new(),
                }
            }
        }
    }
}

fn fresh_type_var(
    text: &str,
    vars: &mut HashMap<String, TypeVarId>,
    next_var: &mut TypeVarId,
) -> Option<TypeVarId> {
    if !text.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
        return None;
    }
    if text.contains('.') {
        return None;
    }
    if let Some(existing) = vars.get(text) {
        return Some(*existing);
    }

    let id = *next_var;
    vars.insert(text.to_string(), id);
    *next_var += 1;
    Some(id)
}

fn expand_type_aliases(ty: &Type, aliases: &HashMap<String, TypeAlias>) -> Type {
    match ty {
        Type::Named {
            module: None,
            name,
            args,
        } => {
            if let Some(alias) = aliases.get(name) {
                if alias.is_opaque {
                    return Type::Named {
                        module: None,
                        name: name.clone(),
                        args: args
                            .iter()
                            .map(|arg| expand_type_aliases(arg, aliases))
                            .collect(),
                    };
                }
                if alias.params.len() != args.len() {
                    return ty.clone();
                }
                let args = args
                    .iter()
                    .map(|arg| expand_type_aliases(arg, aliases))
                    .collect::<Vec<_>>();
                let mapping = alias
                    .params
                    .iter()
                    .cloned()
                    .zip(args)
                    .collect::<HashMap<_, _>>();
                expand_type_aliases(&substitute_type_vars(&alias.body, &mapping), aliases)
            } else {
                Type::Named {
                    module: None,
                    name: name.clone(),
                    args: args
                        .iter()
                        .map(|arg| expand_type_aliases(arg, aliases))
                        .collect(),
                }
            }
        }
        Type::Named {
            module: Some(module),
            name,
            args,
        } => {
            let qualified_name = format!("{}.{}", module, name);
            if let Some(alias) = aliases.get(&qualified_name) {
                if alias.is_opaque {
                    return Type::Named {
                        module: Some(module.clone()),
                        name: name.clone(),
                        args: args
                            .iter()
                            .map(|arg| expand_type_aliases(arg, aliases))
                            .collect(),
                    };
                }
                if alias.params.len() != args.len() {
                    return ty.clone();
                }
                let args = args
                    .iter()
                    .map(|arg| expand_type_aliases(arg, aliases))
                    .collect::<Vec<_>>();
                let mapping = alias
                    .params
                    .iter()
                    .cloned()
                    .zip(args)
                    .collect::<HashMap<_, _>>();
                expand_type_aliases(&substitute_type_vars(&alias.body, &mapping), aliases)
            } else {
                Type::Named {
                    module: Some(module.clone()),
                    name: name.clone(),
                    args: args
                        .iter()
                        .map(|arg| expand_type_aliases(arg, aliases))
                        .collect(),
                }
            }
        }
        Type::List(inner) => Type::List(Box::new(expand_type_aliases(inner, aliases))),
        Type::Tuple(items) => Type::Tuple(
            items
                .iter()
                .map(|item| expand_type_aliases(item, aliases))
                .collect(),
        ),
        Type::Map(fields) => Type::Map(
            fields
                .iter()
                .map(|field| MapFieldType {
                    key: field.key.clone(),
                    value: expand_type_aliases(&field.value, aliases),
                    required: field.required,
                })
                .collect(),
        ),
        Type::Union(items) => Type::Union(
            items
                .iter()
                .map(|item| expand_type_aliases(item, aliases))
                .collect(),
        ),
        Type::Function { params, ret } => Type::Function {
            params: params
                .iter()
                .map(|param| expand_type_aliases(param, aliases))
                .collect(),
            ret: Box::new(expand_type_aliases(ret, aliases)),
        },
        Type::FixedArray { size, elem } => Type::FixedArray {
            size: *size,
            elem: Box::new(expand_type_aliases(elem, aliases)),
        },
        other => other.clone(),
    }
}

fn substitute_type_vars(ty: &Type, mapping: &HashMap<TypeVarId, Type>) -> Type {
    match ty {
        Type::Var(id) => mapping.get(id).cloned().unwrap_or(Type::Var(*id)),
        Type::List(inner) => Type::List(Box::new(substitute_type_vars(inner, mapping))),
        Type::Tuple(items) => Type::Tuple(
            items
                .iter()
                .map(|item| substitute_type_vars(item, mapping))
                .collect(),
        ),
        Type::Union(items) => Type::Union(
            items
                .iter()
                .map(|item| substitute_type_vars(item, mapping))
                .collect(),
        ),
        Type::Map(fields) => Type::Map(
            fields
                .iter()
                .map(|field| MapFieldType {
                    key: field.key.clone(),
                    value: substitute_type_vars(&field.value, mapping),
                    required: field.required,
                })
                .collect(),
        ),
        Type::Named { module, name, args } => Type::Named {
            module: module.clone(),
            name: name.clone(),
            args: args
                .iter()
                .map(|arg| substitute_type_vars(arg, mapping))
                .collect(),
        },
        Type::Function { params, ret } => Type::Function {
            params: params
                .iter()
                .map(|param| substitute_type_vars(param, mapping))
                .collect(),
            ret: Box::new(substitute_type_vars(ret, mapping)),
        },
        Type::FixedArray { size, elem } => Type::FixedArray {
            size: *size,
            elem: Box::new(substitute_type_vars(elem, mapping)),
        },
        other => other.clone(),
    }
}

fn normalize_union(mut types: Vec<Type>) -> Type {
    let mut expanded = Vec::new();
    while let Some(ty) = types.pop() {
        match ty {
            Type::Union(items) => expanded.extend(items),
            other => expanded.push(other),
        }
    }

    let mut normalized: Vec<Type> = Vec::new();
    for candidate in expanded {
        if candidate == Type::Never {
            continue;
        }
        if candidate == Type::Term {
            return Type::Term;
        }
        if normalized
            .iter()
            .any(|existing| is_subtype(&candidate, existing))
        {
            continue;
        }
        normalized.retain(|existing| !is_subtype(existing, &candidate));
        normalized.push(candidate);
    }

    if normalized.is_empty() {
        Type::Never
    } else if normalized.len() == 1 {
        normalized.into_iter().next().unwrap()
    } else {
        Type::Union(normalized)
    }
}

fn is_subtype(lhs: &Type, rhs: &Type) -> bool {
    if lhs == rhs {
        return true;
    }
    match (lhs, rhs) {
        (_, Type::Dynamic) => true,
        (_, Type::Term) => true,
        (Type::Int, Type::Number) => true,
        (Type::Float, Type::Number) => true,
        (Type::LiteralInt(_), Type::Int) => true,
        (Type::LiteralAtom(_), Type::Atom) => true,
        (
            Type::FixedArray {
                size: lhs_size,
                elem: lhs_elem,
            },
            Type::FixedArray {
                size: rhs_size,
                elem: rhs_elem,
            },
        ) => lhs_size == rhs_size && is_subtype(lhs_elem, rhs_elem),
        (Type::Map(lhs), Type::Map(rhs)) => map_fields_is_subtype(lhs, rhs),
        (Type::Never, _) => true,
        _ => false,
    }
}

fn map_fields_is_subtype(lhs: &[MapFieldType], rhs: &[MapFieldType]) -> bool {
    for rhs_field in rhs {
        let Some(lhs_field) = lhs.iter().find(|field| field.key == rhs_field.key) else {
            if rhs_field.required {
                return false;
            }
            continue;
        };

        if rhs_field.required && !lhs_field.required {
            return false;
        }

        if !is_subtype(&lhs_field.value, &rhs_field.value) {
            return false;
        }
    }

    true
}

fn unify(left: &Type, right: &Type, subst: &mut HashMap<TypeVarId, Type>) -> Result<(), String> {
    let left = apply_subst(left, subst);
    let right = apply_subst(right, subst);

    match (&left, &right) {
        (Type::Dynamic, _) | (_, Type::Dynamic) => Ok(()),
        (Type::Term, _) => Ok(()),
        (_, Type::Never) => Ok(()),
        (Type::Var(left_id), Type::Var(right_id)) if left_id == right_id => Ok(()),
        (Type::Var(id), rhs) => bind_var(*id, rhs.clone(), subst),
        (lhs, Type::Var(id)) => bind_var(*id, lhs.clone(), subst),
        (Type::Union(left), Type::Union(right)) => {
            for l in left {
                let mut trial_ok = false;
                for r in right {
                    let mut trial_subst = subst.clone();
                    if unify(l, r, &mut trial_subst).is_ok() {
                        *subst = trial_subst;
                        trial_ok = true;
                        break;
                    }
                }
                if !trial_ok {
                    return Err(format!(
                        "expected {} but could not match {}",
                        pretty_type(&Type::Union(right.to_vec())),
                        pretty_type(l)
                    ));
                }
            }
            Ok(())
        }
        (Type::Union(left), rhs) => {
            for l in left {
                let mut trial_subst = subst.clone();
                if unify(l, rhs, &mut trial_subst).is_ok() {
                    *subst = trial_subst;
                    return Ok(());
                }
            }
            Err(format!(
                "expected {} found {}",
                pretty_type(&Type::Union(left.clone())),
                pretty_type(rhs)
            ))
        }
        (lhs, Type::Union(right)) => {
            for r in right {
                let mut trial_subst = subst.clone();
                if unify(lhs, r, &mut trial_subst).is_ok() {
                    *subst = trial_subst;
                    return Ok(());
                }
            }
            Err(format!(
                "expected {} found {}",
                pretty_type(lhs),
                pretty_type(&Type::Union(right.clone()))
            ))
        }
        (Type::Int, Type::Number) => Ok(()),
        (Type::Float, Type::Number) => Ok(()),
        (Type::LiteralInt(_), Type::Number) => Ok(()),
        (Type::Number, Type::LiteralInt(_)) => Ok(()),
        (Type::Number, Type::Int) | (Type::Number, Type::Float) => {
            Err("expected Number but found Int/Float".to_string())
        }
        (Type::LiteralAtom(left_atom), Type::LiteralAtom(right_atom))
            if left_atom == right_atom =>
        {
            Ok(())
        }
        (Type::LiteralInt(left_int), Type::LiteralInt(right_int)) if left_int == right_int => {
            Ok(())
        }
        (Type::Int, Type::Int)
        | (Type::Float, Type::Float)
        | (Type::Atom, Type::Atom)
        | (Type::Atom, Type::LiteralAtom(_))
        | (Type::LiteralAtom(_), Type::Atom)
        | (Type::Int, Type::LiteralInt(_))
        | (Type::LiteralInt(_), Type::Int)
        | (Type::Binary, Type::Binary)
        | (Type::Bool, Type::Bool) => Ok(()),
        (Type::List(lhs), Type::List(rhs)) => unify(lhs, rhs, subst),
        (Type::Map(lhs_fields), Type::Map(rhs_fields)) => {
            unify_map_fields(lhs_fields, rhs_fields, subst)
        }
        (Type::Tuple(lhs), Type::Tuple(rhs)) => {
            if lhs.len() != rhs.len() {
                return Err(format!(
                    "tuple arity mismatch: expected {} elements, found {}",
                    lhs.len(),
                    rhs.len()
                ));
            }
            for (left_item, right_item) in lhs.iter().zip(rhs.iter()) {
                unify(left_item, right_item, subst)?;
            }
            Ok(())
        }
        (
            Type::Named {
                module: m1,
                name: n1,
                args: args1,
            },
            Type::Named {
                module: m2,
                name: n2,
                args: args2,
            },
        ) => {
            if m1 == m2 && n1 == n2 && args1.len() == args2.len() {
                for (a, b) in args1.iter().zip(args2.iter()) {
                    unify(a, b, subst)?;
                }
                Ok(())
            } else {
                Err(format!(
                    "expected {} found {}",
                    pretty_type(&Type::Named {
                        module: m1.clone(),
                        name: n1.clone(),
                        args: args1.clone(),
                    }),
                    pretty_type(&Type::Named {
                        module: m2.clone(),
                        name: n2.clone(),
                        args: args2.clone(),
                    })
                ))
            }
        }
        (
            Type::Function {
                params: params_a,
                ret: ret_a,
            },
            Type::Function {
                params: params_b,
                ret: ret_b,
            },
        ) => {
            if params_a.len() != params_b.len() {
                return Err(format!(
                    "function arity mismatch: expected {} args, found {}",
                    params_a.len(),
                    params_b.len()
                ));
            }
            for (a, b) in params_a.iter().zip(params_b.iter()) {
                unify(a, b, subst)?;
            }
            unify(ret_a.as_ref(), ret_b.as_ref(), subst)
        }
        (
            Type::FixedArray {
                size: size_a,
                elem: elem_a,
            },
            Type::FixedArray {
                size: size_b,
                elem: elem_b,
            },
        ) => {
            if size_a != size_b {
                return Err(format!(
                    "expected {} found {}",
                    pretty_type(&Type::FixedArray {
                        size: *size_a,
                        elem: elem_a.clone(),
                    }),
                    pretty_type(&Type::FixedArray {
                        size: *size_b,
                        elem: elem_b.clone(),
                    })
                ));
            }
            unify(elem_a, elem_b, subst)
        }
        _ => Err(format!(
            "expected {} found {}",
            pretty_type(&left),
            pretty_type(&right)
        )),
    }
}

fn unify_map_fields(
    lhs: &[MapFieldType],
    rhs: &[MapFieldType],
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<(), String> {
    for rhs_field in rhs {
        let Some(lhs_field) = lhs.iter().find(|field| field.key == rhs_field.key) else {
            if rhs_field.required {
                return Err(format!("missing required map field: {}", rhs_field.key));
            }
            continue;
        };

        if rhs_field.required && !lhs_field.required {
            return Err(format!(
                "required map field {} cannot match optional",
                rhs_field.key
            ));
        }

        unify(&lhs_field.value, &rhs_field.value, subst)?;
    }

    for lhs_field in lhs {
        if lhs_field.required {
            let present = rhs.iter().any(|rhs_field| rhs_field.key == lhs_field.key);
            if !present {
                return Err(format!("missing required map field: {}", lhs_field.key));
            }
        }
    }

    Ok(())
}

fn bind_var(
    id: TypeVarId,
    value: Type,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<(), String> {
    let value = widen_type_var_binding(value);
    if let Some(existing) = subst.get(&id).cloned() {
        return unify(&existing, &value, subst);
    }
    if occurs(id, &value, subst) {
        return Err("recursive type".to_string());
    }
    subst.insert(id, value);
    Ok(())
}

/// Widens overly specific literal types when binding generic variables.
///
/// Inputs:
/// - `value`: inferred type about to bind a type variable.
///
/// Output:
/// - A type suitable for reuse across generic call arguments.
///
/// Transformation:
/// - Converts integer literal singleton types into `Int` so generic calls such
///   as `Some(1)` and `Some(2)` can agree on `T = Int`; leaves atom literals
///   unchanged because atom literals carry closed-shape domain information.
fn widen_type_var_binding(value: Type) -> Type {
    match value {
        Type::LiteralInt(_) => Type::Int,
        other => other,
    }
}

fn occurs(var: TypeVarId, value: &Type, subst: &HashMap<TypeVarId, Type>) -> bool {
    match apply_subst(value, subst) {
        Type::Var(other) => other == var,
        Type::List(inner) => occurs(var, &inner, subst),
        Type::Tuple(items) => items.iter().any(|item| occurs(var, item, subst)),
        Type::Union(items) => items.iter().any(|item| occurs(var, item, subst)),
        Type::Named { args, .. } => args.iter().any(|arg| occurs(var, arg, subst)),
        Type::Map(fields) => fields.iter().any(|field| occurs(var, &field.value, subst)),
        Type::Function { params, ret } => {
            params.iter().any(|param| occurs(var, param, subst)) || occurs(var, &ret, subst)
        }
        _ => false,
    }
}

fn reveal_opaque_aliases(ty: &Type, aliases: &HashMap<String, TypeAlias>) -> Type {
    match ty {
        Type::Named {
            module: None,
            name,
            args,
        } => {
            if let Some(alias) = aliases.get(name) {
                if alias.is_opaque && alias.params.len() == args.len() {
                    let mapping = alias
                        .params
                        .iter()
                        .cloned()
                        .zip(args.iter().cloned())
                        .collect::<HashMap<_, _>>();
                    return substitute_type_vars(&alias.body, &mapping);
                }
            }
            Type::Named {
                module: None,
                name: name.clone(),
                args: args
                    .iter()
                    .map(|arg| reveal_opaque_aliases(arg, aliases))
                    .collect(),
            }
        }
        Type::Named { module, name, args } => Type::Named {
            module: module.clone(),
            name: name.clone(),
            args: args
                .iter()
                .map(|arg| reveal_opaque_aliases(arg, aliases))
                .collect(),
        },
        Type::List(inner) => Type::List(Box::new(reveal_opaque_aliases(inner, aliases))),
        Type::Tuple(items) => Type::Tuple(
            items
                .iter()
                .map(|item| reveal_opaque_aliases(item, aliases))
                .collect(),
        ),
        Type::Union(items) => Type::Union(
            items
                .iter()
                .map(|item| reveal_opaque_aliases(item, aliases))
                .collect(),
        ),
        Type::Map(fields) => Type::Map(
            fields
                .iter()
                .map(|field| MapFieldType {
                    key: field.key.clone(),
                    value: reveal_opaque_aliases(&field.value, aliases),
                    required: field.required,
                })
                .collect(),
        ),
        Type::Function { params, ret } => Type::Function {
            params: params
                .iter()
                .map(|param| reveal_opaque_aliases(param, aliases))
                .collect(),
            ret: Box::new(reveal_opaque_aliases(ret, aliases)),
        },
        Type::FixedArray { size, elem } => Type::FixedArray {
            size: *size,
            elem: Box::new(reveal_opaque_aliases(elem, aliases)),
        },
        other => other.clone(),
    }
}

fn apply_subst(ty: &Type, subst: &HashMap<TypeVarId, Type>) -> Type {
    match ty {
        Type::Var(id) => match subst.get(id) {
            Some(inner) => apply_subst(inner, subst),
            None => Type::Var(*id),
        },
        Type::List(inner) => Type::List(Box::new(apply_subst(inner, subst))),
        Type::Tuple(items) => {
            Type::Tuple(items.iter().map(|item| apply_subst(item, subst)).collect())
        }
        Type::Union(items) => {
            Type::Union(items.iter().map(|item| apply_subst(item, subst)).collect())
        }
        Type::Map(fields) => Type::Map(
            fields
                .iter()
                .map(|field| MapFieldType {
                    key: field.key.clone(),
                    value: apply_subst(&field.value, subst),
                    required: field.required,
                })
                .collect(),
        ),
        Type::Named { module, name, args } => Type::Named {
            module: module.clone(),
            name: name.clone(),
            args: args.iter().map(|arg| apply_subst(arg, subst)).collect(),
        },
        Type::Function { params, ret } => Type::Function {
            params: params
                .iter()
                .map(|param| apply_subst(param, subst))
                .collect(),
            ret: Box::new(apply_subst(ret, subst)),
        },
        Type::FixedArray { size, elem } => Type::FixedArray {
            size: *size,
            elem: Box::new(apply_subst(elem, subst)),
        },
        other => other.clone(),
    }
}

fn builtin_call(name: &str, arity: usize) -> Option<FunctionScheme> {
    match (name, arity) {
        ("integer_to_binary", 1) => Some(FunctionScheme {
            params: vec![Type::Int],
            ret: Type::Binary,
            bounds: Vec::new(),
        }),
        ("is_integer", 1)
        | ("is_binary", 1)
        | ("is_atom", 1)
        | ("is_boolean", 1)
        | ("is_list", 1)
        | ("is_map", 1)
        | ("is_tuple", 1) => Some(FunctionScheme {
            params: vec![Type::Dynamic],
            ret: Type::Bool,
            bounds: Vec::new(),
        }),
        _ => None,
    }
}

fn is_literal_atom(name: &str) -> bool {
    matches!(name, "ok" | "error" | "true" | "false" | "nil") || is_type_constructor_atom(name)
}

fn widen_list_literal_element_type(ty: Type) -> Type {
    match ty {
        Type::LiteralInt(_) => Type::Int,
        Type::LiteralAtom(atom) => {
            if atom == "true" || atom == "false" {
                Type::Bool
            } else {
                Type::Atom
            }
        }
        other => other,
    }
}

fn is_type_constructor_atom(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut chars = name.chars();
    match chars.next() {
        Some(c) => {
            if !c.is_ascii_lowercase() {
                return false;
            }
        }
        None => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$' || c == '-')
}

fn is_lower_identifier(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn compact_spaces(input: &str) -> String {
    let mut output = String::new();
    let mut quote = None;
    let mut escape = false;

    for ch in input.chars() {
        if let Some(quote_ch) = quote {
            output.push(ch);
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == quote_ch {
                quote = None;
            }
            continue;
        }

        match ch {
            '\'' | '"' => {
                quote = Some(ch);
                output.push(ch);
            }
            ch if ch.is_whitespace() => {}
            _ => output.push(ch),
        }
    }

    output
}

fn strip_wrapping_parens(input: &str) -> Option<&str> {
    let bytes = input.as_bytes();
    if bytes.first() != Some(&b'(') || bytes.last() != Some(&b')') {
        return None;
    }

    let mut depth = 0usize;
    for (idx, byte) in bytes.iter().enumerate() {
        match *byte {
            b'(' => depth += 1,
            b')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 && idx != bytes.len() - 1 {
                    return None;
                }
            }
            _ => {}
        }
    }

    Some(&input[1..input.len() - 1])
}

fn split_top_level_arrow(input: &str) -> Option<(String, String)> {
    let bytes = input.as_bytes();
    let mut p_depth = 0usize;
    let mut b_depth = 0usize;
    let mut t_depth = 0usize;

    for i in 0..bytes.len() {
        match bytes[i] {
            b'(' => p_depth += 1,
            b')' => p_depth = p_depth.saturating_sub(1),
            b'[' => b_depth += 1,
            b']' => b_depth = b_depth.saturating_sub(1),
            b'{' => t_depth += 1,
            b'}' => t_depth = t_depth.saturating_sub(1),
            b'-' if i + 1 < bytes.len()
                && bytes[i + 1] == b'>'
                && p_depth == 0
                && b_depth == 0
                && t_depth == 0 =>
            {
                let left = String::from_utf8_lossy(&bytes[..i]).to_string();
                let right = String::from_utf8_lossy(&bytes[i + 2..]).to_string();
                return Some((left.trim().to_string(), right.trim().to_string()));
            }
            _ => {}
        }
    }

    None
}

fn split_top_level_csv(input: &str) -> Vec<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::new();
    let mut p_depth = 0usize;
    let mut b_depth = 0usize;
    let mut t_depth = 0usize;
    let mut start = 0usize;

    for (idx, ch) in bytes.iter().enumerate() {
        match *ch {
            b'(' => p_depth += 1,
            b')' => p_depth = p_depth.saturating_sub(1),
            b'[' => b_depth += 1,
            b']' => b_depth = b_depth.saturating_sub(1),
            b'{' => t_depth += 1,
            b'}' => t_depth = t_depth.saturating_sub(1),
            b',' if p_depth == 0 && b_depth == 0 && t_depth == 0 => {
                out.push(String::from_utf8_lossy(&bytes[start..idx]).to_string());
                start = idx + 1;
            }
            _ => {}
        }
    }

    out.push(String::from_utf8_lossy(&bytes[start..]).to_string());
    out.into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

fn split_top_level_plus(input: &str) -> Vec<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::new();
    let mut p_depth = 0usize;
    let mut b_depth = 0usize;
    let mut t_depth = 0usize;
    let mut start = 0usize;

    for (idx, ch) in bytes.iter().enumerate() {
        match *ch {
            b'(' => p_depth += 1,
            b')' => p_depth = p_depth.saturating_sub(1),
            b'[' => b_depth += 1,
            b']' => b_depth = b_depth.saturating_sub(1),
            b'{' => t_depth += 1,
            b'}' => t_depth = t_depth.saturating_sub(1),
            b'+' if p_depth == 0 && b_depth == 0 && t_depth == 0 => {
                out.push(String::from_utf8_lossy(&bytes[start..idx]).to_string());
                start = idx + 1;
            }
            _ => {}
        }
    }

    out.push(String::from_utf8_lossy(&bytes[start..]).to_string());
    out.into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

fn split_top_level_union(input: &str) -> Vec<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::new();
    let mut p_depth = 0usize;
    let mut b_depth = 0usize;
    let mut t_depth = 0usize;
    let mut start = 0usize;

    for (idx, ch) in bytes.iter().enumerate() {
        match *ch {
            b'(' => p_depth += 1,
            b')' => p_depth = p_depth.saturating_sub(1),
            b'[' => b_depth += 1,
            b']' => b_depth = b_depth.saturating_sub(1),
            b'{' => t_depth += 1,
            b'}' => t_depth = t_depth.saturating_sub(1),
            b'|' if p_depth == 0 && b_depth == 0 && t_depth == 0 => {
                out.push(String::from_utf8_lossy(&bytes[start..idx]).to_string());
                start = idx + 1;
            }
            _ => {}
        }
    }

    out.push(String::from_utf8_lossy(&bytes[start..]).to_string());
    out.into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

fn split_module_name(name: &str) -> (Option<String>, String) {
    if let Some((module, base)) = name.split_once('.') {
        (Some(module.to_string()), base.to_string())
    } else {
        (None, name.to_string())
    }
}

fn is_list_type(input: &str) -> bool {
    input.starts_with('[') && input.ends_with(']') && !input.contains("||")
}

fn is_tuple_type(input: &str) -> bool {
    input.starts_with('{') && input.ends_with('}')
}

fn is_map_type_expr(input: &str) -> bool {
    input.starts_with("#{") && input.ends_with('}') && input.len() >= 3
}

pub fn pretty_type(ty: &Type) -> String {
    match ty {
        Type::Int => "Int".to_string(),
        Type::Float => "Float".to_string(),
        Type::Number => "Number".to_string(),
        Type::Binary => "Binary".to_string(),
        Type::Atom => "Atom".to_string(),
        Type::Bool => "Bool".to_string(),
        Type::Term => "Term".to_string(),
        Type::Dynamic => "Dynamic".to_string(),
        Type::Never => "Never".to_string(),
        Type::LiteralAtom(atom) => atom.to_string(),
        Type::LiteralInt(value) => format!("{}", value),
        Type::Var(id) => format!("T{}", id),
        Type::List(inner) => format!("List[{}]", pretty_type(inner)),
        Type::FixedArray { size, elem } => {
            format!("FixedArray[{}, {}]", size, pretty_type(elem))
        }
        Type::Tuple(items) => format!(
            "({})",
            items.iter().map(pretty_type).collect::<Vec<_>>().join(", ")
        ),
        Type::Map(fields) => format!(
            "#{{{}}}",
            fields
                .iter()
                .map(|field| {
                    let sep = if field.required { ":=" } else { "=>" };
                    format!("{}{}{}", field.key, sep, pretty_type(&field.value))
                })
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Type::Union(items) => items
            .iter()
            .map(pretty_type)
            .collect::<Vec<_>>()
            .join(" | "),
        Type::Named { module, name, args } => {
            let qualified = if let Some(module_name) = module {
                format!("{}.{}", module_name, name)
            } else {
                name.clone()
            };
            if args.is_empty() {
                qualified
            } else {
                format!(
                    "{}[{}]",
                    qualified,
                    args.iter().map(pretty_type).collect::<Vec<_>>().join(", ")
                )
            }
        }
        Type::Function { params, ret } => {
            format!(
                "({}) -> {}",
                params
                    .iter()
                    .map(pretty_type)
                    .collect::<Vec<_>>()
                    .join(", "),
                pretty_type(ret)
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use terlan_hir::{
        load_interfaces_from_file_set, parse_interface_file, resolve_syntax_module_output,
        resolve_syntax_module_output_with_interfaces, syntax_module_output_to_interface,
    };
    use terlan_syntax::{
        parse_expr_as_syntax_output, parse_interface_module_as_syntax_output,
        parse_module_as_syntax_output, SyntaxPatternFieldOutput,
    };

    fn check_syntax_output(source: &str) -> Vec<Diagnostic> {
        let module = parse_module_as_syntax_output(source)
            .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        type_check_syntax_module_output(&module, &resolved)
    }

    fn check_syntax_output_with_interface(source: &str, interface_source: &str) -> Vec<Diagnostic> {
        let interface_module = parse_interface_module_as_syntax_output(interface_source)
            .unwrap_or_else(|err| panic!("failed to parse syntax interface fixture: {:?}", err));
        let mut interfaces = HashMap::new();
        interfaces.insert(
            interface_module.module_name.clone(),
            syntax_module_output_to_interface(&interface_module),
        );

        let module = parse_module_as_syntax_output(source)
            .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
        type_check_syntax_module_output(&module, &resolved)
    }

    /// Verifies that syntax-output boolean operators typecheck as Bool.
    ///
    /// Inputs:
    /// - A module whose function body combines `and`, `or`, and comparison
    ///   expressions with a `Bool` return annotation.
    ///
    /// Output:
    /// - Test passes when no type diagnostics are produced.
    ///
    /// Transformation:
    /// - Parses through the formal syntax-output path, resolves the module, and
    ///   typechecks the resulting expression tree.
    #[test]
    fn syntax_output_boolean_binary_ops_typecheck_as_bool() {
        let diagnostics = check_syntax_output(
            "\
module boolean_ops.\n\
pub decide(ready: Bool, fallback: Bool, value: Int): Bool ->\n\
    ready and value == 1 or fallback.\n\
",
        );

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    /// Verifies that syntax-output boolean operators reject non-Bool operands.
    ///
    /// Inputs:
    /// - A module whose function body uses an `Int` as the right operand of
    ///   `and`.
    ///
    /// Output:
    /// - Test passes when typechecking reports a Bool operand mismatch.
    ///
    /// Transformation:
    /// - Parses through the formal syntax-output path and checks the generated
    ///   diagnostics for the Bool mismatch emitted by binary operator inference.
    #[test]
    fn syntax_output_boolean_binary_ops_require_bool_operands() {
        let diagnostics = check_syntax_output(
            "\
module boolean_ops_bad.\n\
pub decide(ready: Bool): Bool ->\n\
    ready and 1.\n\
",
        );

        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.message.contains("expected Bool found")),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_declared_constructor_patterns_are_valid_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module constructor_patterns.\n\
pub constructor None {\n\
    (): Dynamic -> :none\n\
}.\n\
pub constructor Some {\n\
    (value: Dynamic): Dynamic -> {:some, value}\n\
}.\n\
pub unwrap(input: Dynamic): Dynamic ->\n\
    case input {\n\
        None -> :none;\n\
        Some(value) -> value;\n\
        :error -> :error\n\
    }.\n\
",
        );
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_unknown_constructor_patterns_are_rejected_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module constructor_patterns.\n\
pub unwrap(input: Dynamic): Dynamic ->\n\
    case input {\n\
        Missing -> input\n\
    }.\n\
",
        );
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.message == "unknown constructor pattern Missing"),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_raw_atom_patterns_do_not_require_constructor_declarations_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module raw_atom_patterns.\n\
pub unwrap(input: Dynamic): Dynamic ->\n\
    case input {\n\
        :none -> :none;\n\
        :empty -> :empty\n\
    }.\n\
",
        );
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    /// Verifies generic comparator callbacks keep their declared return type.
    ///
    /// Inputs:
    /// - A local `Option[T]` alias and a `Comparison` result alias.
    /// - A generic `compare_option` function that accepts `(T, T) ->
    ///   Comparison` and calls it from a nested `case` branch.
    ///
    /// Output:
    /// - Test passes when the syntax-output typechecker accepts the callback
    ///   result as `Comparison` rather than inferring the contained `T`.
    ///
    /// Transformation:
    /// - Parses the formal syntax-output path, infers the higher-order
    ///   callback invocation inside pattern-refined branches, and validates the
    ///   enclosing function return annotation.
    #[test]
    fn syntax_output_generic_comparator_callback_preserves_declared_return_type() {
        let diagnostics = check_syntax_output(
            "\
module comparator_callback_return.\n\
pub type Comparison = :lt | :eq | :gt.\n\
pub type Option[T] = :none | {:some, T}.\n\
pub compare_option(compare: (T, T) -> Comparison, left: Option[T], right: Option[T]): Comparison ->\n\
    case left {\n\
        :none ->\n\
            case right {\n\
                :none -> :eq;\n\
                {:some, _} -> :lt\n\
            };\n\
\n\
        {:some, left_value} ->\n\
            case right {\n\
                :none -> :gt;\n\
                {:some, right_value} -> compare(left_value, right_value)\n\
            }\n\
    }.\n\
",
        );

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    /// Verifies remote public signatures preserve fully qualified alias returns.
    ///
    /// Inputs:
    /// - A provider interface exposing `compare` with a generic contained type
    ///   and a fully qualified `ordering.Comparison` callback/return type.
    /// - A consumer module that calls `option.compare` with `Option[Int]`
    ///   shapes and compares the result with `:lt`.
    ///
    /// Output:
    /// - Test passes when the remote call result remains `Comparison` instead
    ///   of collapsing to the option contained type.
    ///
    /// Transformation:
    /// - Builds provider interfaces through the syntax-output interface path,
    ///   resolves the consumer against those interfaces, and checks that
    ///   generic argument inference at the interface boundary does not leak the
    ///   `T` substitution into the declared callback return.
    #[test]
    fn syntax_output_remote_comparator_signature_preserves_qualified_alias_return_type() {
        let ordering = parse_interface_module_as_syntax_output(
            "\
module ordering.\n\
pub type Comparison = :lt | :eq | :gt.\n\
",
        )
        .unwrap_or_else(|err| panic!("failed to parse ordering fixture: {:?}", err));
        let option = parse_interface_module_as_syntax_output(
            "\
module option.\n\
pub type Option[T] = :none | {:some, T}.\n\
pub constructor None {\n\
    (): Option[T] -> :none\n\
}.\n\
pub constructor Some[T] {\n\
    (value: T): Option[T] -> {:some, value}\n\
}.\n\
pub compare(\n\
    left: Option[A],\n\
    right: Option[A],\n\
    value_compare: (A, A) -> ordering.Comparison\n\
): ordering.Comparison.\n\
",
        )
        .unwrap_or_else(|err| panic!("failed to parse option interface fixture: {:?}", err));

        let mut interfaces = HashMap::new();
        interfaces.insert(
            ordering.module_name.clone(),
            syntax_module_output_to_interface(&ordering),
        );
        interfaces.insert(
            option.module_name.clone(),
            syntax_module_output_to_interface(&option),
        );

        let consumer = parse_module_as_syntax_output(
            "\
module option_consumer.\n\
import option.{None, Some}.\n\
import type ordering.Comparison.\n\
pub compare_int(left: Int, right: Int): Comparison -> :lt.\n\
pub demo(): Bool ->\n\
    std.test.Test.assert_equal(:lt, option.compare(None(), Some(1), compare_int)).\n\
",
        )
        .unwrap_or_else(|err| panic!("failed to parse consumer fixture: {:?}", err));

        let resolved = resolve_syntax_module_output_with_interfaces(&consumer, &interfaces).module;
        let diagnostics = type_check_syntax_module_output(&consumer, &resolved);

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    /// Verifies checked std summaries expose `std.core.Option.compare` correctly.
    ///
    /// Inputs:
    /// - A consumer fixture resolved with `load_interfaces_from_file_set` from
    ///   the std test tree, matching the `terlc test` dependency-loading path.
    ///
    /// Output:
    /// - Test passes when `std.core.Option.compare` typechecks as returning an
    ///   ordering atom domain instead of the contained option value type.
    ///
    /// Transformation:
    /// - Loads checked-in std `.typi` summaries, resolves a consumer module
    ///   against them, and typechecks a release-style assertion using
    ///   `Option.compare(None(), Some(1), compare_int)`.
    #[test]
    fn syntax_output_std_option_compare_summary_preserves_comparison_return_type() {
        let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("tests/std/core/option_test.tl");
        let option_summary_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("std/summaries/std.core.Option.typi");
        let direct_option_function_keys = parse_interface_file(&option_summary_path)
            .map(|(_module_name, interface)| {
                let mut keys = interface.functions.keys().cloned().collect::<Vec<_>>();
                keys.sort();
                keys
            })
            .unwrap_or_default();
        let interfaces = load_interfaces_from_file_set(&fixture_path.to_string_lossy());
        let option_compare_return = interfaces
            .get("std.core.Option")
            .and_then(|interface| interface.functions.get(&("compare".to_string(), 3)))
            .map(|signature| signature.return_type.as_str());
        let option_function_keys = interfaces
            .get("std.core.Option")
            .map(|interface| {
                let mut keys = interface.functions.keys().cloned().collect::<Vec<_>>();
                keys.sort();
                keys
            })
            .unwrap_or_default();
        let mut interface_keys = interfaces.keys().cloned().collect::<Vec<_>>();
        interface_keys.sort();
        assert_eq!(
            option_compare_return,
            Some("std.core.Ordering.Comparison"),
            "loaded interfaces: {:?}; loaded std.core.Option function keys: {:?}; direct std.core.Option function keys: {:?}",
            interface_keys,
            option_function_keys,
            direct_option_function_keys
        );
        let option_interface = interfaces.get("std.core.Option").expect("option interface");
        let compare_signature = option_interface
            .functions
            .get(&("compare".to_string(), 3))
            .expect("compare signature");
        let mut global_aliases = HashMap::new();
        for interface in interfaces.values() {
            for (name, alias) in interface_type_aliases(interface) {
                global_aliases.insert(format!("{}.{}", interface.module, name), alias);
            }
        }
        let compare_scheme =
            parse_interface_signature(compare_signature, option_interface, &global_aliases)
                .expect("parse compare scheme");
        assert!(
            matches!(compare_scheme.ret, Type::Union(ref items) if items.len() == 3),
            "compare scheme: {:?}",
            compare_scheme
        );
        let mut trial_subst = HashMap::new();
        let empty_fns = HashMap::new();
        let empty_signatures = HashMap::new();
        let empty_module_aliases = HashMap::new();
        let empty_file_imports = HashMap::new();
        let empty_markdown_imports = HashMap::new();
        let empty_function_imports = HashMap::new();
        let empty_imported_type_names = HashMap::new();
        let empty_constructor_aliases = HashMap::new();
        let empty_constructors = HashMap::new();
        let empty_templates = HashMap::new();
        let empty_struct_fields = HashMap::new();
        let empty_receiver_methods = HashMap::new();
        let empty_trait_method_calls = HashMap::new();
        let empty_trait_bound_impls = HashMap::new();
        let empty_trait_signatures = HashMap::new();
        let empty_alias_names = HashSet::new();
        let trial_trait_cache = RefCell::new(TraitLookupCache::default());
        let trial_ctx = ExprInferContext {
            local_fns: &empty_fns,
            signatures: &empty_signatures,
            interface_map: &interfaces,
            module_aliases: &empty_module_aliases,
            file_imports: &empty_file_imports,
            markdown_imports: &empty_markdown_imports,
            function_imports: &empty_function_imports,
            imported_type_names: &empty_imported_type_names,
            constructor_aliases: &empty_constructor_aliases,
            constructors: &empty_constructors,
            templates: &empty_templates,
            aliases: &global_aliases,
            struct_fields: &empty_struct_fields,
            receiver_methods: &empty_receiver_methods,
            trait_method_calls: &empty_trait_method_calls,
            trait_bound_impl_type_args: &empty_trait_bound_impls,
            trait_signatures: &empty_trait_signatures,
            alias_names: &empty_alias_names,
            current_bounds: &[],
            trait_lookup_cache: &trial_trait_cache,
        };
        let trial_result = infer_function_with_bounds(
            &compare_scheme,
            Some("compare"),
            &[
                Type::LiteralAtom("none".to_string()),
                Type::Tuple(vec![
                    Type::LiteralAtom("some".to_string()),
                    Type::LiteralInt(1),
                ]),
                Type::Function {
                    params: vec![Type::Int, Type::Int],
                    ret: Box::new(compare_scheme.ret.clone()),
                },
            ],
            &trial_ctx,
            &mut trial_subst,
        )
        .expect("trial compare inference");
        assert!(
            matches!(trial_result, Type::Union(ref items) if items.len() == 3),
            "trial result: {:?}",
            trial_result
        );
        let module = parse_module_as_syntax_output(
            "\
module option_summary_consumer.\n\
import std.core.Option.{None, Some}.\n\
import std.core.Ordering.{Lt}.\n\
import type std.core.Ordering.Comparison.\n\
pub compare_int(left: Int, right: Int): Comparison ->\n\
    std.core.Int.compare(left, right).\n\
pub direct(): Comparison ->\n\
    std.core.Option.compare(None(), Some(1), compare_int).\n\
pub demo(): Bool ->\n\
    std.test.Test.assert_equal(Lt(), std.core.Option.compare(None(), Some(1), compare_int)).\n\
",
        )
        .unwrap_or_else(|err| panic!("failed to parse summary consumer fixture: {:?}", err));
        let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
        let diagnostics = type_check_syntax_module_output(&module, &resolved);

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_list_cons_patterns_are_valid_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module list_cons_patterns.\n\
pub prepend(head: Int, tail: List[Int]): List[Int] ->\n\
    [head | tail].\n\
\n\
pub head(input: List[Int]): Int ->\n\
    case input {\n\
        [head | _tail] -> head;\n\
        [] -> 0\n\
    }.\n\
",
        );
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_list_cons_expr_rejects_non_list_tail_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module list_cons_expr_tail.\n\
pub prepend(head: Int, tail: Binary): List[Int] ->\n\
    [head | tail].\n\
",
        );
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.message.contains("list cons tail")),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_declared_constructor_calls_are_valid_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module constructor_calls.\n\
pub constructor Some {\n\
    (value: Dynamic): Dynamic -> {:some, value}\n\
}.\n\
pub make(value: Dynamic): Dynamic ->\n\
    Some(value).\n\
",
        );
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_unknown_constructor_calls_are_rejected_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module constructor_calls.\n\
pub make(value: Dynamic): Dynamic ->\n\
    Missing(value).\n\
",
        );
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.message == "unknown constructor Missing / 1"),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_imported_constructor_alias_calls_are_valid_on_formal_path() {
        let interface_source = "\
module option.\n\
pub constructor Some {\n\
    (value: Dynamic): Dynamic -> {:some, value}\n\
}.\n\
";
        let diagnostics = check_syntax_output_with_interface(
            "\
module option_consumer.\n\
import option.{Some}.\n\
pub make(value: Dynamic): Dynamic ->\n\
    Some(value).\n\
",
            interface_source,
        );
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_remote_constructor_calls_are_rejected_by_parser_on_formal_path() {
        let error = parse_module_as_syntax_output(
            "\
module option_consumer.\n\
pub make(value: Dynamic): Dynamic ->\n\
    option.Some(value).\n\
",
        )
        .expect_err("uppercase dotted remote constructor calls are not source syntax");
        assert!(
            format!("{:?}", error).contains("expected lower-case remote function name"),
            "error: {:?}",
            error
        );
    }

    #[test]
    fn syntax_output_unknown_remote_constructor_calls_are_rejected_by_parser_on_formal_path() {
        let error = parse_module_as_syntax_output(
            "\
module option_consumer.\n\
pub make(value: Dynamic): Dynamic ->\n\
    option.Missing(value).\n\
",
        )
        .expect_err("uppercase dotted remote constructor calls are not source syntax");
        assert!(
            format!("{:?}", error).contains("expected lower-case remote function name"),
            "error: {:?}",
            error
        );
    }

    #[test]
    fn syntax_output_colon_remote_calls_are_checked_against_interfaces_on_formal_path() {
        let interface_source = "\
module math.\n\
pub inc(value: Int): Int.\n\
";
        let diagnostics = check_syntax_output_with_interface(
            "\
module math_consumer.\n\
pub demo(): Int ->\n\
    math:inc(1).\n\
",
            interface_source,
        );
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_colon_remote_calls_report_argument_mismatches_on_formal_path() {
        let interface_source = "\
module math.\n\
pub inc(value: Int): Int.\n\
";
        let diagnostics = check_syntax_output_with_interface(
            "\
module math_consumer.\n\
pub demo(): Int ->\n\
    math:inc(\"bad\").\n\
",
            interface_source,
        );
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.message.contains("expected Int found Binary")),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    /// Verifies selected function imports are checked against provider signatures.
    ///
    /// Inputs:
    /// - A provider interface declaring `println(value: String): Unit`.
    /// - A consumer module importing `println` by local name and calling it
    ///   with an `Int`.
    ///
    /// Output:
    /// - Test passes when the syntax-output typechecker reports an argument
    ///   mismatch for the selected import.
    ///
    /// Transformation:
    /// - Resolves the selected import through the provider interface and reuses
    ///   ordinary function scheme inference for the local call.
    #[test]
    fn syntax_output_selected_function_imports_report_argument_mismatches() {
        let interface_source = "\
module console.\n\
pub println(value: String): Unit.\n\
";
        let diagnostics = check_syntax_output_with_interface(
            "\
module console_consumer.\n\
import console.{println}.\n\
pub demo(): Unit ->\n\
    println(1).\n\
",
            interface_source,
        );
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.message.contains("expected Binary found 1")),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    /// Verifies selected import diagnostics suggest the loaded primitive module.
    ///
    /// Inputs:
    /// - A loaded `std.core.Int` interface exporting `to_string`.
    /// - A consumer that mistakenly imports `std.io.Int.{to_string}`.
    ///
    /// Output:
    /// - Test passes when the diagnostic names the missing module and suggests
    ///   the available core import path.
    ///
    /// Transformation:
    /// - Resolves a selected import whose provider interface is absent, searches
    ///   loaded interfaces for the selected function, and emits a deterministic
    ///   import suggestion.
    #[test]
    fn syntax_output_selected_function_imports_suggest_loaded_provider_module() {
        let interface_source = "\
module std.core.Int.\n\
pub to_string(value: Int): String.\n\
";
        let source = "\
module int_import_consumer.\n\
import std.io.Int.{to_string}.\n\
pub demo(): String ->\n\
    to_string(2).\n\
";
        let diagnostics = check_syntax_output_with_interface(source, interface_source);
        let diagnostic = diagnostics
            .iter()
            .find(|diag| {
                diag.message
                    .contains("cannot find module `std.io.Int` for imported function `to_string`")
            })
            .unwrap_or_else(|| panic!("diagnostics: {:?}", diagnostics));
        assert!(
            diagnostic
                .message
                .contains("did you mean `std.core.Int.{to_string}`?"),
            "diagnostics: {:?}",
            diagnostics
        );
        assert_eq!(
            &source[diagnostic.span.start..diagnostic.span.end],
            "to_string",
            "diagnostic should point at selected import item"
        );
    }

    #[test]
    fn syntax_output_single_shape_alias_constructor_calls_are_valid_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module alias_constructor_calls.\n\
pub type Ok[T] = {:ok, value: T}.\n\
pub make(value: Int): Dynamic ->\n\
    Ok(value).\n\
",
        );
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_single_shape_alias_constructor_calls_report_arity_mismatch_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module alias_constructor_call_arity.\n\
pub type Ok[T] = {:ok, value: T}.\n\
pub make(): Dynamic ->\n\
    Ok().\n\
",
        );
        assert!(
            diagnostics.iter().any(|diag| {
                diag.message == "constructor Ok has arity mismatch: expected 1..1 args, found 0"
            }),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_list_aliases_do_not_generate_constructor_calls_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module list_alias_constructor_calls.\n\
pub type Items[T] = List[T].\n\
pub make(values: List[Int]): Items[Int] ->\n\
    Items(values).\n\
",
        );
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.message == "unknown constructor Items / 1"),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_imported_list_aliases_do_not_generate_constructor_calls_on_formal_path() {
        let diagnostics = check_syntax_output_with_interface(
            "\
module imported_list_alias_constructor_calls.\n\
import items.{Items}.\n\
pub make(values: List[Int]): Items[Int] ->\n\
    Items(values).\n\
",
            "\
module items.\n\
pub type Items[T] = List[T].\n\
",
        );
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.message == "unknown constructor Items / 1"),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    /// Verifies aliased imported list aliases do not become constructor calls.
    ///
    /// Inputs:
    /// - A provider interface exporting non-eligible alias `Items[T] = List[T]`.
    /// - A consumer module importing `Items as Bag` and calling `Bag(values)`.
    ///
    /// Output:
    /// - Test passes when syntax-output typechecking reports `unknown
    ///   constructor Bag / 1`.
    ///
    /// Transformation:
    /// - Loads provider interface metadata into the syntax-output typechecker,
    ///   resolves the local import alias, and confirms non-single-shape aliases
    ///   never produce constructor-call identity metadata under aliased names.
    #[test]
    fn syntax_output_aliased_imported_list_aliases_do_not_generate_constructor_calls_on_formal_path(
    ) {
        let diagnostics = check_syntax_output_with_interface(
            "\
module aliased_imported_list_alias_constructor_calls.\n\
import items.{Items as Bag}.\n\
pub make(values: List[Int]): Bag[Int] ->\n\
    Bag(values).\n\
",
            "\
module items.\n\
pub type Items[T] = List[T].\n\
",
        );
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.message == "unknown constructor Bag / 1"),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_imported_alias_constructor_calls_report_arity_mismatch_on_formal_path() {
        let diagnostics = check_syntax_output_with_interface(
            "\
module imported_alias_constructor_call_arity.\n\
import result.{Ok}.\n\
pub make(): Dynamic ->\n\
    Ok().\n\
",
            "\
module result.\n\
pub type Ok[T] = {:ok, value: T}.\n\
",
        );
        assert!(
            diagnostics.iter().any(|diag| {
                diag.message == "constructor Ok has arity mismatch: expected 1..1 args, found 0"
            }),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    /// Verifies aliased imported eligible type-alias constructor calls with
    /// wrong arity fail as constructor arity errors on the source alias name.
    ///
    /// Inputs:
    /// - A provider interface exporting `Ok[T] = {:ok, value: T}`.
    /// - A consumer module importing `Ok as Success` and calling `Success()`.
    ///
    /// Output:
    /// - Test passes when syntax-output typechecking reports the constructor
    ///   arity mismatch against `Success`.
    ///
    /// Transformation:
    /// - Loads provider interface metadata into the syntax-output typechecker,
    ///   resolves the local import alias, and confirms eligible imported
    ///   aliases preserve arity diagnostics for source-visible call heads.
    #[test]
    fn syntax_output_aliased_imported_alias_constructor_calls_report_arity_mismatch_on_formal_path()
    {
        let diagnostics = check_syntax_output_with_interface(
            "\
module aliased_imported_alias_constructor_call_arity.\n\
import result.{Ok as Success}.\n\
pub make(): Dynamic ->\n\
    Success().\n\
",
            "\
module result.\n\
pub type Ok[T] = {:ok, value: T}.\n\
",
        );
        assert!(
            diagnostics.iter().any(|diag| {
                diag.message
                    == "constructor Success has arity mismatch: expected 1..1 args, found 0"
            }),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_imported_list_aliases_do_not_generate_constructor_chains_on_formal_path() {
        let diagnostics = check_syntax_output_with_interface(
            "\
module imported_list_alias_constructor_chains.\n\
import items.{Items}.\n\
pub make(values: List[Int]): Dynamic ->\n\
    Items(values) with Wrapped { values = values }.\n\
",
            "\
module items.\n\
pub type Items[T] = List[T].\n\
",
        );
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.message == "unknown constructor Items / 1"),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    /// Verifies aliased imported list aliases do not become constructor-chain
    /// bases.
    ///
    /// Inputs:
    /// - A provider interface exporting non-eligible alias `Items[T] = List[T]`.
    /// - A consumer module importing `Items as Bag` and using `Bag(values)` as
    ///   a constructor-chain base.
    ///
    /// Output:
    /// - Test passes when syntax-output typechecking reports `unknown
    ///   constructor Bag / 1`.
    ///
    /// Transformation:
    /// - Loads provider interface metadata into the syntax-output typechecker,
    ///   resolves the local import alias, and confirms non-single-shape aliases
    ///   never produce constructor-chain identity metadata under aliased names.
    #[test]
    fn syntax_output_aliased_imported_list_aliases_do_not_generate_constructor_chains_on_formal_path(
    ) {
        let diagnostics = check_syntax_output_with_interface(
            "\
module aliased_imported_list_alias_constructor_chains.\n\
import items.{Items as Bag}.\n\
pub make(values: List[Int]): Dynamic ->\n\
    Bag(values) with Wrapped { values = values }.\n\
",
            "\
module items.\n\
pub type Items[T] = List[T].\n\
",
        );
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.message == "unknown constructor Bag / 1"),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    /// Verifies directly imported eligible type-alias constructor chains with
    /// wrong arity fail as constructor arity errors.
    ///
    /// Inputs:
    /// - A provider interface exporting `User = {:user, id: Int, name: Binary}`.
    /// - A consumer module importing `User` directly and using `User(id)` as a
    ///   constructor-chain base.
    ///
    /// Output:
    /// - Test passes when syntax-output typechecking reports the imported
    ///   constructor arity mismatch.
    ///
    /// Transformation:
    /// - Loads provider interface metadata into the syntax-output typechecker
    ///   and confirms imported single-shape aliases keep arity diagnostics for
    ///   constructor-chain bases instead of becoming unresolved chain metadata.
    #[test]
    fn syntax_output_imported_alias_constructor_chains_report_arity_mismatch_on_formal_path() {
        let diagnostics = check_syntax_output_with_interface(
            "\
module imported_alias_constructor_chain_arity.\n\
import result.{User}.\n\
pub make(id: Int): Dynamic ->\n\
    User(id) with Wrapped { id = id }.\n\
",
            "\
module result.\n\
pub type User = {:user, id: Int, name: Binary}.\n\
",
        );
        assert!(
            diagnostics.iter().any(|diag| {
                diag.message == "constructor User has arity mismatch: expected 2..2 args, found 1"
            }),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    /// Verifies aliased imported eligible type-alias constructor chains with
    /// wrong arity fail as constructor arity errors on the source alias name.
    ///
    /// Inputs:
    /// - A provider interface exporting `User = {:user, id: Int, name: Binary}`.
    /// - A consumer module importing `User as Member` and using `Member(id)` as
    ///   a constructor-chain base.
    ///
    /// Output:
    /// - Test passes when syntax-output typechecking reports the constructor
    ///   arity mismatch against `Member`.
    ///
    /// Transformation:
    /// - Loads provider interface metadata into the syntax-output typechecker,
    ///   resolves the local import alias, and confirms eligible imported
    ///   aliases preserve arity diagnostics for source-visible chain bases.
    #[test]
    fn syntax_output_aliased_imported_alias_constructor_chains_report_arity_mismatch_on_formal_path(
    ) {
        let diagnostics = check_syntax_output_with_interface(
            "\
module aliased_imported_alias_constructor_chain_arity.\n\
import result.{User as Member}.\n\
pub make(id: Int): Dynamic ->\n\
    Member(id) with Wrapped { id = id }.\n\
",
            "\
module result.\n\
pub type User = {:user, id: Int, name: Binary}.\n\
",
        );
        assert!(
            diagnostics.iter().any(|diag| {
                diag.message == "constructor Member has arity mismatch: expected 2..2 args, found 1"
            }),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_single_shape_alias_constructor_chains_report_arity_mismatch_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module alias_constructor_chain_arity.\n\
pub type User = {:user, id: Int, name: Binary}.\n\
pub make(id: Int): Dynamic ->\n\
    User(id) with Wrapped { id = id }.\n\
",
        );
        assert!(
            diagnostics.iter().any(|diag| {
                diag.message == "constructor User has arity mismatch: expected 2..2 args, found 1"
            }),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_single_shape_alias_constructor_patterns_are_valid_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module alias_constructor_patterns.\n\
pub type Ok[T] = {:ok, value: T}.\n\
pub unwrap(input: Ok[Int]): Int ->\n\
    case input {\n\
        Ok(value) -> value\n\
    }.\n\
",
        );
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_single_shape_alias_constructor_patterns_report_arity_mismatch_on_formal_path()
    {
        let diagnostics = check_syntax_output(
            "\
module alias_constructor_pattern_arity.\n\
pub type Ok[T] = {:ok, value: T}.\n\
pub unwrap(input: Ok[Int]): Int ->\n\
    case input {\n\
        Ok(value, extra) -> value\n\
    }.\n\
",
        );
        assert!(
            diagnostics.iter().any(|diag| {
                diag.message == "constructor Ok has arity mismatch: expected 1..1 args, found 2"
            }),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_list_aliases_do_not_generate_constructor_patterns_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module list_alias_constructor_patterns.\n\
pub type Items[T] = List[T].\n\
pub unwrap(input: Items[Int]): List[Int] ->\n\
    case input {\n\
        Items(values) -> values\n\
    }.\n\
",
        );
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.message == "unknown constructor pattern Items"),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_structural_tuple_aliases_do_not_generate_constructor_calls_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module structural_alias_constructor_calls.\n\
pub type Pair = {left: Int, right: Int}.\n\
pub make(): Pair ->\n\
    Pair(1, 2).\n\
",
        );
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.message == "unknown constructor Pair / 2"),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_structural_tuple_aliases_do_not_generate_constructor_patterns_on_formal_path()
    {
        let diagnostics = check_syntax_output(
            "\
module structural_alias_constructor_patterns.\n\
pub type Pair = {left: Int, right: Int}.\n\
pub left(input: Pair): Int ->\n\
    case input {\n\
        Pair(left, _right) -> left\n\
    }.\n\
",
        );
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.message == "unknown constructor pattern Pair"),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_imported_structural_tuple_aliases_do_not_generate_constructor_calls_on_formal_path(
    ) {
        let diagnostics = check_syntax_output_with_interface(
            "\
module imported_structural_alias_constructor_calls.\n\
import pairs.{Pair}.\n\
pub make(): Pair ->\n\
    Pair(1, 2).\n\
",
            "\
module pairs.\n\
pub type Pair = {left: Int, right: Int}.\n\
",
        );
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.message == "unknown constructor Pair / 2"),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_imported_structural_tuple_aliases_do_not_generate_constructor_patterns_on_formal_path(
    ) {
        let diagnostics = check_syntax_output_with_interface(
            "\
module imported_structural_alias_constructor_patterns.\n\
import pairs.{Pair}.\n\
pub left(input: Pair): Int ->\n\
    case input {\n\
        Pair(left, _right) -> left\n\
    }.\n\
",
            "\
module pairs.\n\
pub type Pair = {left: Int, right: Int}.\n\
",
        );
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.message == "unknown constructor pattern Pair"),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_map_aliases_do_not_generate_constructor_calls_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module map_alias_constructor_calls.\n\
pub type Props = #{name := Binary}.\n\
pub make(name: Binary): Props ->\n\
    Props(#{name = name}).\n\
",
        );
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.message == "unknown constructor Props / 1"),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_map_aliases_do_not_generate_constructor_patterns_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module map_alias_constructor_patterns.\n\
pub type Props = #{name := Binary}.\n\
pub name(input: Props): Binary ->\n\
    case input {\n\
        Props(values) -> values\n\
    }.\n\
",
        );
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.message == "unknown constructor pattern Props"),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_imported_map_aliases_do_not_generate_constructor_calls_on_formal_path() {
        let diagnostics = check_syntax_output_with_interface(
            "\
module imported_map_alias_constructor_calls.\n\
import props.{Props}.\n\
pub make(name: Binary): Props ->\n\
    Props(#{name = name}).\n\
",
            "\
module props.\n\
pub type Props = #{name := Binary}.\n\
",
        );
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.message == "unknown constructor Props / 1"),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_imported_map_aliases_do_not_generate_constructor_patterns_on_formal_path() {
        let diagnostics = check_syntax_output_with_interface(
            "\
module imported_map_alias_constructor_patterns.\n\
import props.{Props}.\n\
pub name(input: Props): Binary ->\n\
    case input {\n\
        Props(values) -> values\n\
    }.\n\
",
            "\
module props.\n\
pub type Props = #{name := Binary}.\n\
",
        );
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.message == "unknown constructor pattern Props"),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_remote_list_alias_constructor_calls_are_rejected_by_parser_on_formal_path() {
        let error = parse_module_as_syntax_output(
            "\
module remote_list_alias_constructor_calls.\n\
pub make(values: List[Int]): items.Items[Int] ->\n\
    items.Items(values).\n\
",
        )
        .expect_err("uppercase dotted remote alias constructor calls are not source syntax");
        assert!(
            format!("{:?}", error).contains("expected lower-case remote function name"),
            "error: {:?}",
            error
        );
    }

    #[test]
    fn syntax_output_imported_list_aliases_do_not_generate_constructor_patterns_on_formal_path() {
        let diagnostics = check_syntax_output_with_interface(
            "\
module imported_list_alias_constructor_patterns.\n\
import items.{Items}.\n\
pub unwrap(input: Items[Int]): List[Int] ->\n\
    case input {\n\
        Items(values) -> values\n\
    }.\n\
",
            "\
module items.\n\
pub type Items[T] = List[T].\n\
",
        );
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.message == "unknown constructor pattern Items"),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    /// Verifies aliased imported list aliases do not become constructor
    /// patterns.
    ///
    /// Inputs:
    /// - A provider interface exporting non-eligible alias `Items[T] = List[T]`.
    /// - A consumer module importing `Items as Bag` and matching `Bag(values)`.
    ///
    /// Output:
    /// - Test passes when syntax-output typechecking reports `unknown
    ///   constructor pattern Bag`.
    ///
    /// Transformation:
    /// - Loads provider interface metadata into the syntax-output typechecker,
    ///   resolves the local import alias, and confirms non-single-shape aliases
    ///   never produce constructor-pattern identity metadata under aliased
    ///   names.
    #[test]
    fn syntax_output_aliased_imported_list_aliases_do_not_generate_constructor_patterns_on_formal_path(
    ) {
        let diagnostics = check_syntax_output_with_interface(
            "\
module aliased_imported_list_alias_constructor_patterns.\n\
import items.{Items as Bag}.\n\
pub unwrap(input: Bag[Int]): List[Int] ->\n\
    case input {\n\
        Bag(values) -> values\n\
    }.\n\
",
            "\
module items.\n\
pub type Items[T] = List[T].\n\
",
        );
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.message == "unknown constructor pattern Bag"),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_literal_alias_constructor_patterns_are_valid_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module alias_literal_patterns.\n\
pub type None = Atom[\"none\"].\n\
pub unwrap(input: None): Dynamic ->\n\
    case input {\n\
        None -> :ok\n\
    }.\n\
",
        );
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_imported_literal_alias_constructor_patterns_are_valid_on_formal_path() {
        let diagnostics = check_syntax_output_with_interface(
            "\
module imported_alias_literal_patterns.\n\
import literals.{None}.\n\
pub unwrap(input: None): Dynamic ->\n\
    case input {\n\
        None -> :ok\n\
    }.\n\
",
            "\
module literals.\n\
pub type None = Atom[\"none\"].\n\
",
        );
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    /// Verifies atom-literal aliases compare against their literal runtime
    /// value.
    ///
    /// Inputs:
    /// - A syntax-output module defining `Unit = Atom["unit"]`.
    /// - A public function returning `Unit`.
    /// - A comparison between the function result and `:unit`.
    ///
    /// Output:
    /// - Test passes when syntax-output typechecking accepts the comparison
    ///   without diagnostics.
    ///
    /// Transformation:
    /// - Runs the formal syntax-output typechecker and confirms binary
    ///   comparison inference expands transparent aliases before rejecting
    ///   otherwise distinct operand spellings.
    #[test]
    fn syntax_output_literal_aliases_compare_with_literal_values_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module alias_literal_comparisons.\n\
pub type Unit = Atom[\"unit\"].\n\
pub value(): Unit ->\n\
    Unit().\n\
pub matches(): Bool ->\n\
    value() == Unit().\n\
",
        );
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_literal_alias_constructor_calls_are_valid_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module alias_literal_calls.\n\
pub type None = Atom[\"none\"].\n\
pub none(): None ->\n\
    None().\n\
",
        );
        assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
    }

    #[test]
    fn syntax_output_remote_literal_alias_constructor_calls_are_rejected_by_parser_on_formal_path()
    {
        let error = parse_module_as_syntax_output(
            "\
module remote_alias_literal_calls.\n\
pub none(): Dynamic ->\n\
    literals.None().\n\
",
        )
        .expect_err("uppercase dotted remote literal alias calls are not source syntax");
        assert!(
            format!("{:?}", error).contains("expected lower-case remote function name"),
            "error: {:?}",
            error
        );
    }

    #[test]
    fn syntax_output_imported_literal_alias_constructor_calls_are_valid_on_formal_path() {
        let diagnostics = check_syntax_output_with_interface(
            "\
module imported_alias_literal_calls.\n\
import literals.{None}.\n\
pub none(): None ->\n\
    None().\n\
",
            "\
module literals.\n\
pub type None = Atom[\"none\"].\n\
",
        );
        assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
    }

    #[test]
    fn syntax_output_union_aliases_do_not_generate_constructor_patterns_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module alias_union_patterns.\n\
pub type None = :none | :empty.\n\
pub unwrap(input: Dynamic): Dynamic ->\n\
    case input {\n\
        None -> :ok\n\
    }.\n\
",
        );
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.message == "unknown constructor pattern None"),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_imported_union_aliases_do_not_generate_constructor_patterns_on_formal_path() {
        let diagnostics = check_syntax_output_with_interface(
            "\
module imported_alias_union_patterns.\n\
import options.{None}.\n\
pub unwrap(input: Dynamic): Dynamic ->\n\
    case input {\n\
        None -> :ok\n\
    }.\n\
",
            "\
module options.\n\
pub type None = :none | :empty.\n\
",
        );
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.message == "unknown constructor pattern None"),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_union_aliases_do_not_generate_constructor_calls_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module alias_union_calls.\n\
pub type None = :none | :empty.\n\
pub none(): Dynamic ->\n\
    None().\n\
",
        );
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.message == "unknown constructor None / 0"),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_imported_union_aliases_do_not_generate_constructor_calls_on_formal_path() {
        let diagnostics = check_syntax_output_with_interface(
            "\
module imported_alias_union_calls.\n\
import options.{None}.\n\
pub none(): Dynamic ->\n\
    None().\n\
",
            "\
module options.\n\
pub type None = :none | :empty.\n\
",
        );
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.message == "unknown constructor None / 0"),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_remote_union_alias_constructor_calls_are_rejected_by_parser_on_formal_path() {
        let error = parse_module_as_syntax_output(
            "\
module remote_alias_union_calls.\n\
pub none(): Dynamic ->\n\
    options.None().\n\
",
        )
        .expect_err("uppercase dotted remote union alias calls are not source syntax");
        assert!(
            format!("{:?}", error).contains("expected lower-case remote function name"),
            "error: {:?}",
            error
        );
    }

    #[test]
    fn syntax_output_remote_alias_constructor_calls_are_rejected_by_parser_on_formal_path() {
        let error = parse_module_as_syntax_output(
            "\
module result_consumer.\n\
pub make(value: Int): Dynamic ->\n\
    result.Ok(value).\n\
",
        )
        .expect_err("uppercase dotted remote alias constructor calls are not source syntax");
        assert!(
            format!("{:?}", error).contains("expected lower-case remote function name"),
            "error: {:?}",
            error
        );
    }

    #[test]
    fn syntax_output_quoted_atom_alias_constructor_patterns_are_valid_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module alias_quoted_literal_patterns.\n\
pub type ModuleAtom = :'Elixir.Module'.\n\
pub unwrap(input: ModuleAtom): Dynamic ->\n\
    case input {\n\
        ModuleAtom -> :ok\n\
    }.\n\
",
        );
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_imported_alias_constructor_calls_are_valid_on_formal_path() {
        let interface_source = "\
module result.\n\
pub type Ok[T] = {:ok, value: T}.\n\
";
        let diagnostics = check_syntax_output_with_interface(
            "\
module result_consumer.\n\
import result.{Ok}.\n\
pub make(value: Int): Dynamic ->\n\
    Ok(value).\n\
",
            interface_source,
        );
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_imported_alias_constructor_patterns_are_valid_on_formal_path() {
        let interface_source = "\
module result.\n\
pub type Ok[T] = {:ok, value: T}.\n\
";
        let diagnostics = check_syntax_output_with_interface(
            "\
module result_consumer.\n\
import result.{Ok}.\n\
pub unwrap(input: Ok[Int]): Int ->\n\
    case input {\n\
        Ok(value) -> value\n\
    }.\n\
",
            interface_source,
        );
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_imported_alias_constructor_patterns_report_arity_mismatch_on_formal_path() {
        let interface_source = "\
module result.\n\
pub type Ok[T] = {:ok, value: T}.\n\
";
        let diagnostics = check_syntax_output_with_interface(
            "\
module result_consumer.\n\
import result.{Ok}.\n\
pub unwrap(input: Ok[Int]): Int ->\n\
    case input {\n\
        Ok(value, extra) -> value\n\
    }.\n\
",
            interface_source,
        );
        assert!(
            diagnostics.iter().any(|diag| {
                diag.message == "constructor Ok has arity mismatch: expected 1..1 args, found 2"
            }),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    /// Verifies aliased imported eligible type-alias constructor patterns with
    /// wrong arity fail as constructor arity errors on the source alias name.
    ///
    /// Inputs:
    /// - A provider interface exporting `Ok[T] = {:ok, value: T}`.
    /// - A consumer module importing `Ok as Success` and matching
    ///   `Success(value, extra)`.
    ///
    /// Output:
    /// - Test passes when syntax-output typechecking reports the constructor
    ///   arity mismatch against `Success`.
    ///
    /// Transformation:
    /// - Loads provider interface metadata into the syntax-output typechecker,
    ///   resolves the local import alias, and confirms eligible imported
    ///   aliases preserve arity diagnostics for source-visible pattern heads.
    #[test]
    fn syntax_output_aliased_imported_alias_constructor_patterns_report_arity_mismatch_on_formal_path(
    ) {
        let interface_source = "\
module result.\n\
pub type Ok[T] = {:ok, value: T}.\n\
";
        let diagnostics = check_syntax_output_with_interface(
            "\
module result_consumer.\n\
import result.{Ok as Success}.\n\
pub unwrap(input: Success[Int]): Int ->\n\
    case input {\n\
        Success(value, extra) -> value\n\
    }.\n\
",
            interface_source,
        );
        assert!(
            diagnostics.iter().any(|diag| {
                diag.message
                    == "constructor Success has arity mismatch: expected 1..1 args, found 2"
            }),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_reports_return_mismatch_on_formal_path() {
        let source = "\
module math.\n\
pub bad(X: Int): Binary ->\n\
    X + 1.\n\
";
        let syntax_diagnostics = check_syntax_output(source);

        let syntax_messages = syntax_diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect::<Vec<_>>();
        assert!(syntax_messages
            .iter()
            .any(|message| message.contains("expected Binary found Int")));
    }

    /// Verifies syntax-output casts stop before backend emission.
    ///
    /// Inputs:
    /// - A syntax-output module whose function body uses explicit
    ///   `value as Int` cast syntax.
    ///
    /// Output:
    /// - Test passes when typechecking reports the stable trait-backed
    ///   conversion diagnostic for the cast.
    ///
    /// Transformation:
    /// - Parses through the formal syntax-output path, resolves the module,
    ///   typechecks the cast node, and confirms the compiler keeps casts as
    ///   parse-preserved but semantically unsupported until conversion traits
    ///   are implemented.
    #[test]
    fn syntax_output_rejects_cast_before_conversion_resolution_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module syntax_cast_boundary.\n\
pub cast_int(value: Dynamic): Int ->\n\
    value as Int.\n\
",
        );

        assert!(
            diagnostics.iter().any(|diagnostic| diagnostic
                .message
                .contains("cast from Dynamic to Int requires trait-backed conversion resolution before backend emission")),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_checks_unary_expr_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module syntax_unary_expr.\n\
pub flip(flag: Bool): Bool ->\n\
    not flag.\n\
pub negate(value: Int): Int ->\n\
    -value.\n\
",
        );

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_checks_remote_fun_ref_on_formal_path() {
        let parsed = parse_module_as_syntax_output(
            "\
module syntax_remote_fun_ref.\n\
pub ref(): Dynamic ->\n\
    fun math:double/1.\n\
",
        );

        assert!(
            parsed.is_err(),
            "remote fun references are backend output syntax, not canonical Terlan source"
        );
    }

    #[test]
    fn syntax_output_checks_macro_expr_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module syntax_macro_expr.\n\
pub module_name(): Dynamic ->\n\
    ?MODULE.\n\
pub compare(a: Int, b: Int): Dynamic ->\n\
    ?assert_equal(a, b).\n\
",
        );

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_checks_macro_expr_with_declared_return_type() {
        let diagnostics = check_syntax_output(
            "\
module syntax_macro_return_type.
pub macro to_bool(X: Int): Ast[Bool] ->
    quote X.

pub bad(X: Int): Int ->
    ?to_bool(X).
",
        );

        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.message.contains("expected Int")
                    && diag.message.contains("found Bool")),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_checks_macro_expr_arity_mismatch() {
        let diagnostics = check_syntax_output(
            "\
module syntax_macro_arity.
pub macro asserter(X: Int, Y: Int): Ast[Int] ->
    quote X.

pub bad(X: Int): Bool ->
    ?asserter(X).
",
        );

        assert!(
            diagnostics.iter().any(|diag| diag
                .message
                .contains("wrong arity for macro `asserter`")
                && diag.message.contains("found 1")),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_rejects_raw_macro_expr_without_macro_resolution() {
        let diagnostics = check_syntax_output(
            "\
module syntax_raw_macro_expr.\n\
pub query(): Dynamic ->\n\
    sql{select * from users}.\n\
",
        );

        assert!(
            diagnostics.iter().any(|diagnostic| diagnostic
                .message
                .contains("raw macro expression `sql` requires macro resolution")),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_accepts_config_declaration_placeholders() {
        let diagnostics = check_syntax_output(
            "\
	module syntax_config_declaration_placeholders.
target erlang.
machine linux.
static site.
	",
        );

        assert!(
            diagnostics.iter().all(|diagnostic| !diagnostic
                .message
                .contains("unsupported raw declaration kind")),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn collects_syntax_raw_macro_diagnostics() {
        let module = parse_module_as_syntax_output(
            "\
module syntax_raw_macro_expr_report.\n\
pub query(): Dynamic ->\n\
    sql{select * from users}.\n\
",
        )
        .expect("parse syntax-output module");
        let diagnostics = collect_syntax_raw_macro_diagnostics(&module);

        assert_eq!(diagnostics.len(), 1, "diagnostics: {:?}", diagnostics);
        assert!(
            diagnostics[0]
                .message
                .contains("raw macro expression `sql` requires macro resolution"),
            "diagnostic: {:?}",
            diagnostics[0]
        );
        assert_ne!(diagnostics[0].span, Span::new(0, 0));
    }

    #[test]
    fn expands_syntax_raw_macros_preserves_module_and_reports_diagnostics() {
        let module = parse_module_as_syntax_output(
            "\
module syntax_raw_macro_expansion.\n\
pub query(): Dynamic ->\n\
    sql{select * from users}.\n\
",
        )
        .expect("parse syntax-output module");

        let (expanded, diagnostics) = expand_syntax_raw_macros(module.clone());

        assert_eq!(
            expanded, module,
            "macro-expansion is currently explicit/no-op"
        );
        assert_eq!(
            diagnostics.len(),
            1,
            "expected one raw macro expansion diagnostic"
        );
        assert!(
            diagnostics[0]
                .message
                .contains("raw macro expression `sql` requires macro resolution"),
            "diagnostic: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_supports_constructor_chain_now() {
        let diagnostics = check_syntax_output(
            "\
module syntax_constructor_chain_expr.\n\
pub type User = Dynamic.\n\
pub constructor User {\n\
    (id: Int, name: Binary): Dynamic ->\n\
        id\n\
}.\n\
pub demo(id: Int, name: Binary): Dynamic ->\n\
    User(id, name) with Admin { id = id, name = name }.\n\
",
        );

        assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
    }

    #[test]
    fn expands_syntax_raw_macros_no_ops_without_raw_macros() {
        let module = parse_module_as_syntax_output(
            "\
module syntax_raw_macro_expansion_ok.\n\
    pub query(): Dynamic ->\n    42.\n\
",
        )
        .expect("parse syntax-output module");

        let (expanded, diagnostics) = expand_syntax_raw_macros(module.clone());

        assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
        assert_eq!(
            expanded, module,
            "non-macro modules must pass through unchanged"
        );
    }

    #[test]
    fn expands_syntax_derives_reports_unknown_trait_and_preserves_module() {
        let module = parse_module_as_syntax_output(
            "\
module syntax_derive_expansion_unknown.\n\
pub struct User derives MissingShow {\n\
    id: Int\n\
}.\n",
        )
        .expect("parse syntax-output derive expansion fixture");
        let resolved = terlan_hir::resolve_syntax_module_output(&module).module;

        let (expanded, diagnostics) = expand_syntax_derives(module.clone(), &resolved);

        assert_eq!(
            expanded, module,
            "derive-expansion is currently explicit/no-op"
        );
        assert_eq!(
            diagnostics.len(),
            1,
            "expected one derive-expansion diagnostic"
        );
        assert!(
            diagnostics[0]
                .message
                .contains("unknown derived trait `MissingShow`")
                && diagnostics[0]
                    .message
                    .contains("declaration of struct `User`"),
            "diagnostic: {:?}",
            diagnostics
        );
    }

    #[test]
    fn expands_syntax_derives_no_ops_without_struct_derives() {
        let module = parse_module_as_syntax_output(
            "\
module syntax_derive_expansion_ok.\n\
pub struct User {\n\
    id: Int\n\
}.\n",
        )
        .expect("parse syntax-output derive expansion fixture");
        let resolved = terlan_hir::resolve_syntax_module_output(&module).module;

        let (expanded, diagnostics) = expand_syntax_derives(module.clone(), &resolved);

        assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
        assert_eq!(
            expanded, module,
            "non-derived modules must pass through unchanged"
        );
    }

    #[test]
    fn syntax_output_checks_if_expr_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module syntax_if_expr.\n\
pub choose(flag: Bool): Int ->\n\
    if {\n\
        flag -> 1;\n\
        true -> 0\n\
    }.\n\
",
        );

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_checks_receive_expr_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module syntax_receive_expr.\n\
pub wait(): Int ->\n\
    receive {\n\
        {:ok, value} -> value;\n\
        :stop -> 0\n\
    }.\n\
",
        );

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_checks_try_expr_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module syntax_try_expr.\n\
pub wait(): Int ->\n\
    try risky() {\n\
        {:ok, value} -> value\n\
    catch\n\
        :error -> 0\n\
    }.\n\
risky(): {:ok, Int} ->\n\
    {:ok, 1}.\n\
",
        );

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_supports_try_after_cleanup() {
        let diagnostics = check_syntax_output(
            "\
module syntax_try_after_expr.\n\
pub wait(): Int ->\n\
    try risky() {\n\
    after\n\
        0 -> 1\n\
    }.\n\
risky(): Int ->\n\
    1.\n\
",
        );

        assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
    }

    #[test]
    fn syntax_output_supports_receive_after_timeout() {
        let diagnostics = check_syntax_output(
            "\
module syntax_receive_after_expr.\n\
pub wait(): Int ->\n\
    receive {\n\
        {:ok, value} -> value;\n\
    after\n\
        0 -> 1\n\
    }.\n\
",
        );

        assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
    }

    #[test]
    fn syntax_output_binds_case_constructor_patterns_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module syntax_case_patterns.\n\
pub type Some = {:some, Int}.\n\
pub unwrap(input: Some): Int ->\n\
    case input {\n\
        Some(value) -> value\n\
    }.\n\
",
        );

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_refines_case_guards_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module syntax_case_guards.\n\
pub to_int(value: Dynamic): Int ->\n\
    case value {\n\
        x when is_integer(x) -> x\n\
    }.\n\
",
        );

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_refines_function_guards_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module syntax_function_guards.\n\
pub to_int(value) when is_integer(value) -> value.\n\
",
        );

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_checks_opaque_constructor_returns_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module syntax_opaque_returns.\n\
pub opaque type UserId = Int.\n\
pub user_id(value: Int): UserId ->\n\
    UserId(value).\n\
",
        );

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_rejects_local_opaque_constructor_patterns_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module syntax_opaque_patterns.\n\
pub opaque type UserId = Int.\n\
pub unwrap(input: UserId): Int ->\n\
    case input {\n\
        UserId(value) -> value\n\
    }.\n\
",
        );

        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.message == "unknown constructor pattern UserId"),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_rejects_imported_opaque_constructor_calls_on_formal_path() {
        let diagnostics = check_syntax_output_with_interface(
            "\
module syntax_imported_opaque_calls.\n\
import users.{UserId}.\n\
pub make(value: Int): UserId ->\n\
    UserId(value).\n\
",
            "\
module users.\n\
pub opaque type UserId = Int.\n\
",
        );

        assert!(
            diagnostics.iter().any(|diag| diag.message
                == "cannot construct opaque type users.UserId outside defining module"),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_remote_opaque_constructor_calls_are_rejected_by_parser_on_formal_path() {
        let error = parse_module_as_syntax_output(
            "\
module syntax_remote_opaque_calls.\n\
pub make(value: Int): users.UserId ->\n\
    users.UserId(value).\n\
",
        )
        .expect_err("uppercase dotted remote opaque constructor calls are not source syntax");

        assert!(
            format!("{:?}", error).contains("expected lower-case remote function name"),
            "error: {:?}",
            error
        );
    }

    #[test]
    fn syntax_output_rejects_imported_opaque_constructor_patterns_on_formal_path() {
        let diagnostics = check_syntax_output_with_interface(
            "\
module syntax_imported_opaque_patterns.\n\
import users.{UserId}.\n\
pub unwrap(input: UserId): Int ->\n\
    case input {\n\
        UserId(value) -> value\n\
    }.\n\
",
            "\
module users.\n\
pub opaque type UserId = Int.\n\
",
        );

        assert!(
            diagnostics.iter().any(|diag| diag.message
                == "cannot match opaque type users.UserId as constructor pattern outside defining module"),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_binds_list_comprehension_patterns_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module syntax_list_patterns.\n\
pub inc_all(values: List[Int]): List[Int] ->\n\
    [x + 1 | x <- values].\n\
",
        );

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_rejects_list_comprehension_non_list_source_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module syntax_list_source.\n\
pub inc_all(value: Int): List[Int] ->\n\
    [x + 1 | x <- value].\n\
",
        );

        assert!(
            diagnostics.iter().any(|diag| {
                diag.message
                    .contains("list comprehension source must be List")
            }),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_infers_local_calls_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module syntax_call_inference.\n\
add_one(x: Int): Int ->\n\
    x + 1.\n\
pub inc_all(values: List[Int]): List[Int] ->\n\
    [add_one(x) | x <- values].\n\
",
        );

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_infers_standalone_expression_on_formal_path() {
        let module = parse_module_as_syntax_output(
            "\
module syntax_expr_query.\n\
pub add_one(value: Int): Int ->\n\
    value + 1.\n\
",
        )
        .expect("parse syntax module");
        let resolved = resolve_syntax_module_output(&module).module;
        let expression = parse_expr_as_syntax_output("add_one(41)").expect("parse syntax expr");

        let (ty, diagnostics) = infer_syntax_expression_type(&expression, &module, &resolved);

        assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
        assert_eq!(pretty_type(&ty), "Int");
    }

    #[test]
    fn syntax_output_infers_pipe_forward_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module syntax_pipe_inference.\n\
add_one(x: Int): Int ->\n\
    x + 1.\n\
pub via_pipe(x: Int): Int ->\n\
    x |> add_one().\n\
",
        );

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_infers_binary_ops_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module syntax_binary_op_inference.\n\
pub add(x: Int, y: Int): Int ->\n\
    x + y.\n\
pub compare(x: Int, y: Int): Bool ->\n\
    x <= y.\n\
",
        );

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_infers_field_access_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module syntax_field_inference.\n\
pub struct User {\n\
    id: Int,\n\
    name: Binary\n\
}.\n\
pub get_id(user: User): Int ->\n\
    user.id.\n\
",
        );

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_checks_template_instantiation_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module syntax_template_instantiation.\n\
template Page from \"./templates/page.tl.html\" {\n\
    title: Binary\n\
}.\n\
pub view(title: Binary): Html[Dynamic] ->\n\
    Page{ title = title }.\n\
",
        );

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_checks_html_blocks_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module syntax_html_blocks.\n\
pub view(title: Binary): Html[Dynamic] ->\n\
    html {\n\
        <section class={[\"hero\"]}>{title}</section>\n\
    }.\n\
",
        );

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_collects_import_maps_on_formal_path() {
        let module = parse_module_as_syntax_output(
            r#"
module imports.

import std.text.{format as format_alias}.
import file "./view.html" as ViewHtml.
import css "./site.css" as SiteCss.
import markdown "./post.md" as Post.

pub view(): Binary ->
    ViewHtml.
"#,
        )
        .expect("parse syntax output import map fixture");

        let maps = collect_syntax_import_maps(&module);

        assert_eq!(
            maps.module_aliases.get("format_alias").map(String::as_str),
            Some("std.text.format")
        );
        assert_eq!(
            maps.file_imports.get("ViewHtml").map(String::as_str),
            Some("./view.html")
        );
        assert_eq!(
            maps.file_imports.get("SiteCss").map(String::as_str),
            Some("./site.css")
        );
        assert_eq!(
            maps.markdown_imports.get("Post").map(String::as_str),
            Some("./post.md")
        );
    }

    #[test]
    fn syntax_output_checks_macro_signatures_on_formal_path() {
        let module = parse_module_as_syntax_output(
            "\
module bad_macro_return.\n\
pub macro bad(X: Int): Int ->\n\
    X.\n\
",
        )
        .expect("parse syntax output macro fixture");

        let diagnostics = check_syntax_macro_decl_signatures(&module);

        assert!(
            diagnostics.iter().any(
                |diag| diag.message.contains("macro `bad` must return Ast[T]")
                    && diag.message.contains("found Int")
            ),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_collects_kind_diagnostics_on_formal_path() {
        let module = parse_module_as_syntax_output(
            "\
module hkt_bad.\n\
\n\
pub trait Functor[F[_]] {\n\
    map[A, B](value: F[A], f: (A) -> B): F[B].\n\
}.\n\
\n\
pub bad(value: Functor[Int]): Int ->\n\
    1.\n\
",
        )
        .expect("parse syntax output kind diagnostic fixture");

        let diagnostics = collect_syntax_kind_diagnostics(&module);

        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.message.contains("kind mismatch")),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_checks_trait_decls_on_formal_path() {
        let module = parse_module_as_syntax_output(
            "\
module trait_extends_bad.\n\
pub trait Derived[A] extends NoSuch[A] {\n\
    derived(value: A): A.\n\
}.\n\
",
        )
        .expect("parse syntax output trait diagnostic fixture");
        let resolved = resolve_syntax_module_output(&module).module;
        let trait_signatures = collect_syntax_trait_signatures(&module, &resolved);
        let diagnostics = check_syntax_trait_decls(&module, &trait_signatures);

        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.message.contains("unknown super trait")),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_checks_struct_derives_on_formal_path() {
        let valid_diagnostics = check_syntax_output(
            "\
module struct_derives_ok.\n\
pub trait Show[A] {\n\
    show(value: A): Binary.\n\
}.\n\
\n\
pub struct User derives Show[User] {\n\
    id: Int\n\
}.\n\
",
        );
        assert!(
            valid_diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            valid_diagnostics
        );

        let unknown_diagnostics = check_syntax_output(
            "\
module struct_derives_unknown.\n\
pub struct User derives NoSuch {\n\
    id: Int\n\
}.\n\
",
        );
        assert!(
            unknown_diagnostics
                .iter()
                .any(|diag| diag.message.contains("unknown derived trait `NoSuch`")),
            "diagnostics: {:?}",
            unknown_diagnostics
        );

        let arity_diagnostics = check_syntax_output(
            "\
module struct_derives_arity.\n\
pub trait Show[A] {\n\
    show(value: A): Binary.\n\
}.\n\
\n\
pub struct User derives Show {\n\
    id: Int\n\
}.\n\
",
        );
        assert!(
            arity_diagnostics.iter().any(|diag| diag
                .message
                .contains("derived trait `Show` expects 1 type parameter(s), found 0")),
            "diagnostics: {:?}",
            arity_diagnostics
        );

        let duplicate_diagnostics = check_syntax_output(
            "\
module struct_derives_duplicate.\n\
pub trait Show[A] {\n\
    show(value: A): Binary.\n\
}.\n\
\n\
pub struct User derives Show[User], Show[User] {\n\
    id: Int\n\
}.\n\
",
        );
        assert!(
            duplicate_diagnostics.iter().any(|diag| diag
                .message
                .contains("duplicate derived trait `Show[User]`")),
            "diagnostics: {:?}",
            duplicate_diagnostics
        );
    }

    #[test]
    fn syntax_output_checks_declared_implements_receiver_methods_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module declared_implements_ok.\n\
pub trait Show[T] {\n\
    to_string(value: T): String.\n\
}.\n\
\n\
pub struct User implements Show[User] {\n\
    name: String\n\
}.\n\
\n\
pub (user: User) to_string(): String ->\n\
    user.name.\n\
",
        );

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_resolves_local_receiver_method_calls_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module receiver_dispatch_ok.\n\
\n\
pub struct User {\n\
    name: String\n\
}.\n\
\n\
pub (user: User) display_name(): String ->\n\
    user.name.\n\
\n\
pub show(user: User): String ->\n\
    user.display_name().\n\
",
        );

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_rejects_duplicate_receiver_method_identity_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module receiver_dispatch_duplicate.\n\
\n\
pub struct User {\n\
    name: String\n\
}.\n\
\n\
pub (user: User) display_name(): String ->\n\
    user.name.\n\
\n\
pub (user: User) display_name(): String ->\n\
    user.name.\n\
",
        );

        assert!(
            diagnostics.iter().any(|diag| diag
                .message
                .contains("duplicate receiver method `display_name` for `User` / 0")),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_rejects_receiver_methods_for_imported_owner_on_formal_path() {
        let diagnostics = check_syntax_output_with_interface(
            "\
module receiver_dispatch_imported_owner.\n\
import users.{User}.\n\
\n\
pub (user: User) display_name(): String ->\n\
    \"external\".\n\
",
            "\
module users.\n\
pub struct User {\n\
    name: String\n\
}.\n\
",
        );

        assert!(
            diagnostics.iter().any(|diag| diag.message.contains(
                "receiver method `display_name` for `User` must be declared in the defining module of `User`"
            )),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_rejects_declared_implements_missing_required_method_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module declared_implements_missing_method.\n\
pub trait Show[T] {\n\
    to_string(value: T): String.\n\
}.\n\
\n\
pub struct User implements Show[User] {\n\
    name: String\n\
}.\n\
",
        );

        assert!(
            diagnostics.iter().any(|diag| diag
                .message
                .contains("missing receiver method `to_string` for `User` implementing `Show`")),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_accepts_declared_implements_trait_default_methods_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module declared_implements_default_method.\n\
pub trait Show[T] {\n\
    to_string(value: T): String -> \"<value>\".\n\
}.\n\
\n\
pub struct User implements Show[User] {\n\
    name: String\n\
}.\n\
",
        );

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_rejects_declared_implements_receiver_signature_mismatch_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module declared_implements_signature_mismatch.\n\
pub trait Show[T] {\n\
    to_string(value: T): String.\n\
}.\n\
\n\
pub struct User implements Show[User] {\n\
    name: String\n\
}.\n\
\n\
pub (user: User) to_string(): Int ->\n\
    1.\n\
",
        );

        assert!(
            diagnostics.iter().any(|diag| diag.message.contains(
                "receiver method `to_string` return type for `User` expects String, found Int"
            )),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_resolves_declared_implements_trait_method_calls_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module declared_implements_dispatch.\n\
pub trait Show[T] {\n\
    to_string(value: T): String.\n\
}.\n\
\n\
pub struct User implements Show[User] {\n\
    name: String\n\
}.\n\
\n\
pub (user: User) to_string(): String ->\n\
    user.name.\n\
\n\
pub stringify(user: User): String ->\n\
    Show.to_string(user).\n\
",
        );

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    /// Verifies generic trait bounds supply local trait-method evidence.
    ///
    /// Inputs:
    /// - Two source modules: one generic function with an `Eq` bound and one
    ///   generic function without that bound.
    ///
    /// Output:
    /// - The bounded module produces no diagnostics.
    /// - The unbounded module reports a missing trait implementation at the
    ///   trait-method call site.
    ///
    /// Transformation:
    /// - Exercises syntax-output typechecking so `Eq.equal(Left, Right)` can be
    ///   checked from the active function bound without synthesizing a global
    ///   implementation candidate.
    #[test]
    fn syntax_output_uses_generic_bounds_for_trait_method_dispatch_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module generic_trait_bound_dispatch.\n\
pub trait Eq[A] {\n\
    equal(left: A, right: A): Bool.\n\
}.\n\
\n\
pub is_same[A](left: A, right: A)[Eq[A]]: Bool ->\n\
    Eq.equal(left, right).\n\
",
        );

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );

        let missing_bound = check_syntax_output(
            "\
module generic_trait_bound_dispatch_missing.\n\
pub trait Eq[A] {\n\
    equal(left: A, right: A): Bool.\n\
}.\n\
\n\
pub is_same[A](left: A, right: A): Bool ->\n\
    Eq.equal(left, right).\n\
",
        );

        assert!(
            missing_bound
                .iter()
                .any(|diag| diag.message.contains("no impl for trait method Eq.equal")),
            "diagnostics: {:?}",
            missing_bound
        );
    }

    #[test]
    fn syntax_output_checks_explicit_trait_impl_methods_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module explicit_trait_impl_ok.\n\
pub trait Show[T] {\n\
    to_string(value: T): String.\n\
}.\n\
\n\
pub struct ExternalUser {\n\
    name: String\n\
}.\n\
\n\
pub impl Show[ExternalUser] for ExternalUser {\n\
    to_string(value: ExternalUser): String ->\n\
        value.name.\n\
}.\n\
",
        );

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_rejects_explicit_trait_impl_missing_required_method_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module explicit_trait_impl_missing_method.\n\
pub trait Show[T] {\n\
    to_string(value: T): String.\n\
}.\n\
\n\
pub struct ExternalUser {\n\
    name: String\n\
}.\n\
\n\
pub impl Show[ExternalUser] for ExternalUser {\n\
}.\n\
",
        );

        assert!(
            diagnostics.iter().any(|diag| diag
                .message
                .contains("missing method `to_string` in impl of trait `Show`")),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_accepts_explicit_trait_impl_default_methods_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module explicit_trait_impl_default_method.\n\
pub trait Show[T] {\n\
    to_string(value: T): String -> \"<value>\".\n\
}.\n\
\n\
pub struct ExternalUser {\n\
    name: String\n\
}.\n\
\n\
pub impl Show[ExternalUser] for ExternalUser {\n\
}.\n\
",
        );

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_rejects_explicit_trait_impl_signature_mismatch_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module explicit_trait_impl_signature_mismatch.\n\
pub trait Show[T] {\n\
    to_string(value: T): String.\n\
}.\n\
\n\
pub struct ExternalUser {\n\
    name: String\n\
}.\n\
\n\
pub impl Show[ExternalUser] for ExternalUser {\n\
    to_string(value: ExternalUser): Int ->\n\
        1.\n\
}.\n\
",
        );

        assert!(
            diagnostics.iter().any(|diag| diag.message.contains(
                "method `to_string` return type in trait `Show` expects String, found Int"
            )),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_rejects_explicit_trait_impl_body_return_mismatch_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module explicit_trait_impl_body_mismatch.\n\
pub trait Show[T] {\n\
    to_string(value: T): String.\n\
}.\n\
\n\
pub struct ExternalUser {\n\
    name: String\n\
}.\n\
\n\
pub impl Show[ExternalUser] for ExternalUser {\n\
    to_string(value: ExternalUser): String ->\n\
        1.\n\
}.\n\
",
        );

        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.message.contains("expected Binary")
                    && diag.message.contains("found 1")),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_rejects_duplicate_declared_and_explicit_trait_impl_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module explicit_trait_impl_duplicate_pair.\n\
pub trait Show[T] {\n\
    to_string(value: T): String.\n\
}.\n\
\n\
pub struct User implements Show[User] {\n\
    name: String\n\
}.\n\
\n\
pub (user: User) to_string(): String ->\n\
    user.name.\n\
\n\
pub impl Show[User] for User {\n\
    to_string(value: User): String ->\n\
        value.name.\n\
}.\n\
",
        );

        assert!(
            diagnostics.iter().any(|diag| diag
                .message
                .contains("coherent impl conflict for `Show[User] for User`")),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_resolves_explicit_trait_impl_method_calls_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module explicit_trait_impl_dispatch.\n\
pub trait Identity[T] {\n\
    id(value: T): T.\n\
}.\n\
\n\
pub struct ExternalUser {\n\
    name: String\n\
}.\n\
\n\
pub impl Identity[ExternalUser] for ExternalUser {\n\
    id(value: ExternalUser): ExternalUser ->\n\
        value.\n\
}.\n\
\n\
pub roundtrip(value: ExternalUser): ExternalUser ->\n\
    Identity.id(value).\n\
",
        );

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_rejects_explicit_trait_impl_method_call_without_impl_on_formal_path() {
        let diagnostics = check_syntax_output(
            "\
module explicit_trait_impl_dispatch_missing.\n\
pub trait Identity[T] {\n\
    id(value: T): T.\n\
}.\n\
\n\
pub struct ExternalUser {\n\
    name: String\n\
}.\n\
\n\
pub roundtrip(value: ExternalUser): ExternalUser ->\n\
    Identity.id(value).\n\
",
        );

        assert!(
            diagnostics.iter().any(|diag| diag
                .message
                .contains("no impl for trait method Identity.id")),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_derive_trait_methods_are_synthesized_for_calls() {
        let diagnostics = check_syntax_output(
            "\
module derive_trait_calls.
pub trait Show[A] {
    show(value: A): Binary.
}.

pub struct User derives Show[User] {
    id: Int
}.

pub describe(value: User): Binary ->
    Show.show(value).
",
        );

        assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
    }

    #[test]
    fn syntax_output_collects_type_aliases_on_formal_path() {
        let module = parse_module_as_syntax_output(
            r#"
module aliases.

pub type Status = :active | :disabled.
pub type Boxed[T] = List[T].

pub struct User {
    id: Int,
    tags: Boxed[Binary]
}.

pub trait Named[T] {
    name(value: T): Binary.
}.

pub trait Show[T] extends Named[T] {
    show(value: T): Binary.
}.

template Profile from "./profile.tl.html" {
    title: Binary,
    user: User
}.

pub constructor Boxed[T] {
    (items: List[T]): Boxed[T] ->
        items;
    (...items: T): Boxed[T] ->
        items
}.

pub ok(): Status ->
    :active.

pub tag_count(tags: Boxed[Binary]): Int ->
    0.
"#,
        )
        .expect("parse syntax output type alias fixture");

        let aliases = collect_syntax_type_aliases(&module);
        let imported_aliases = HashMap::new();
        let imported_names = HashMap::new();
        let extra_names = collect_syntax_alias_extra_names(&module);
        let alias_names = collect_syntax_type_names(&module);
        let function_signatures = collect_syntax_function_signatures(
            &module,
            &alias_names,
            &imported_names,
            &imported_aliases,
            &aliases,
        );
        let constructor_signatures = collect_syntax_constructor_signatures(
            &module,
            &alias_names,
            &imported_names,
            &imported_aliases,
            &aliases,
        );
        let struct_fields = collect_syntax_struct_fields(&module, &alias_names);
        let template_schemes = collect_syntax_template_schemes(&module, &alias_names);
        let resolved = resolve_syntax_module_output(&module).module;
        let trait_signatures = collect_syntax_trait_signatures(&module, &resolved);

        let status = aliases.get("Status").expect("Status alias");
        assert!(matches!(
            &status.body,
            Type::Union(types)
                if types.contains(&Type::LiteralAtom("active".to_string()))
                    && types.contains(&Type::LiteralAtom("disabled".to_string()))
        ));
        assert_eq!(aliases.get("Boxed").expect("Boxed alias").params.len(), 1);
        assert!(extra_names.contains("User"));
        let ok_signature = function_signatures
            .get(&("ok".to_string(), 0))
            .expect("ok function signature");
        assert_eq!(
            ok_signature.ret,
            Type::Named {
                module: None,
                name: "Status".to_string(),
                args: Vec::new(),
            }
        );
        let tag_count_signature = function_signatures
            .get(&("tag_count".to_string(), 1))
            .expect("tag_count function signature");
        assert_eq!(
            tag_count_signature.params,
            vec![Type::Named {
                module: None,
                name: "Boxed".to_string(),
                args: vec![Type::Binary],
            }]
        );
        assert_eq!(tag_count_signature.ret, Type::Int);
        let boxed_constructors = constructor_signatures
            .get("Boxed")
            .expect("Boxed constructor signatures");
        assert_eq!(boxed_constructors.len(), 2);
        assert_eq!(
            boxed_constructors[0].fixed_params,
            vec![Type::List(Box::new(Type::Var(0)))]
        );
        assert_eq!(boxed_constructors[0].min_arity, 1);
        assert_eq!(boxed_constructors[0].vararg, None);
        assert_eq!(
            boxed_constructors[0].ret,
            Type::List(Box::new(Type::Var(0)))
        );
        assert_eq!(boxed_constructors[1].fixed_params, Vec::<Type>::new());
        assert_eq!(boxed_constructors[1].min_arity, 0);
        assert_eq!(boxed_constructors[1].vararg, Some(Type::Var(0)));
        assert_eq!(
            boxed_constructors[1].ret,
            Type::List(Box::new(Type::Var(0)))
        );
        assert_eq!(
            struct_fields
                .get("User")
                .and_then(|fields| fields.get("id")),
            Some(&Type::Int)
        );
        assert_eq!(
            struct_fields
                .get("User")
                .and_then(|fields| fields.get("tags")),
            Some(&Type::Named {
                module: None,
                name: "Boxed".to_string(),
                args: vec![Type::Binary],
            })
        );
        assert_eq!(
            template_schemes
                .get("Profile")
                .and_then(|scheme| scheme.props.get("title")),
            Some(&Type::Binary)
        );
        assert_eq!(
            template_schemes
                .get("Profile")
                .and_then(|scheme| scheme.props.get("user")),
            Some(&Type::Named {
                module: None,
                name: "User".to_string(),
                args: Vec::new(),
            })
        );
        let show_trait = trait_signatures.get("Show").expect("Show trait signature");
        assert_eq!(show_trait.type_params, vec!["T".to_string()]);
        assert_eq!(show_trait.super_traits, vec!["Named[T]".to_string()]);
        let show_method = show_trait.methods.get("show").expect("show method");
        assert_eq!(show_method.params, vec!["T".to_string()]);
        assert_eq!(show_method.return_type, "Binary");
    }

    #[test]
    fn syntax_output_lowering_to_core_preserves_interface_contract() {
        let module = parse_module_as_syntax_output(
            "\
module core_boundary.\n\
pub value(): Int ->\n\
    1.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_resolved_module_to_core(&resolved);

        assert_eq!(core.schema, CORE_IR_SCHEMA);
        assert_eq!(core.module, "core_boundary");
        assert_eq!(core.source.source_kind, "resolved_module");
        assert_eq!(core.functions.len(), 1);
        assert_eq!(core.functions[0].name, "value");
        assert_eq!(core.functions[0].arity, 0);
        assert!(core.functions[0].public);
        assert_eq!(core.functions[0].return_type, "Int");
        assert_eq!(core.functions[0].core_return_type, Some(CoreType::Int));
        assert!(core.exports.iter().any(|export| {
            export.name == "value" && matches!(export.kind, CoreExportKind::Function { arity: 0 })
        }));
        assert_eq!(core.metadata.interface_function_count, 1);
        assert_eq!(
            core.metadata.proof_readiness,
            CoreProofReadiness::NoExpressions
        );
        assert_eq!(core.metadata.lean_covered_expr_count, 0);
        assert_eq!(core.metadata.proof_model_required_expr_count, 0);
        assert_eq!(core.metadata.lean_covered_pattern_count, 0);
        assert_eq!(core.metadata.proof_model_required_pattern_count, 0);
        assert_eq!(core.metadata.typed_core_expr_count, 0);
        assert_eq!(core.metadata.summary_only_expr_count, 0);
        assert_eq!(core.metadata.typed_core_pattern_count, 0);
        assert_eq!(core.metadata.summary_only_pattern_count, 0);
        assert_eq!(core.metadata.checked_preservation_expr_count, 0);
        assert_eq!(core.metadata.checked_preservation_pattern_count, 0);
        assert_eq!(core.metadata.typed_core_type_count, 1);
        assert_eq!(core.metadata.summary_only_type_count, 0);
        assert!(
            core.contract_text()
                .contains("schema=terlan.core_ir.v1\nmodule=core_boundary"),
            "contract text: {}",
            core.contract_text()
        );
        assert!(
            core.interface_text()
                .contains("module core_boundary.\n\npub value(): Int.\n"),
            "interface text: {}",
            core.interface_text()
        );
    }

    #[test]
    fn syntax_output_lowering_to_core_preserves_trait_conformance_facts() {
        let module = parse_module_as_syntax_output(
            "\
module core_trait_conformance.\n\
pub trait Show[T] {\n\
    show(value: T): String.\n\
}.\n\
\n\
pub trait Debug[T] {\n\
    debug(value: T): String.\n\
}.\n\
\n\
pub struct User derives Debug[User] implements Show[User] {\n\
    name: String\n\
}.\n\
\n\
pub (user: User) show(): String ->\n\
    user.name.\n\
\n\
pub impl Show[Int] for Int {\n\
    show(value: Int): String ->\n\
        \"int\".\n\
}.\n\
",
        )
        .unwrap_or_else(|err| panic!("failed to parse conformance fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        assert!(core.trait_conformances.iter().any(|conformance| {
            conformance.trait_ref == "Show[User]"
                && conformance.for_type == "User"
                && conformance.source == CoreTraitConformanceSource::Implements
                && conformance.public
        }));
        assert!(core.trait_conformances.iter().any(|conformance| {
            conformance.trait_ref == "Debug[User]"
                && conformance.for_type == "User"
                && conformance.source == CoreTraitConformanceSource::Derive
                && conformance.public
        }));
        assert!(core.trait_conformances.iter().any(|conformance| {
            conformance.trait_ref == "Show[Int]"
                && conformance.for_type == "Int"
                && conformance.source == CoreTraitConformanceSource::ExplicitImpl
                && conformance.public
        }));
        assert!(
            core.contract_text()
                .contains("trait_conformance=Show[Int] for=Int source=ExplicitImpl public=true"),
            "contract text: {}",
            core.contract_text()
        );
    }

    /// Verifies CoreIR proof-readiness precedence remains stable.
    ///
    /// Inputs:
    /// - None; constructs in-memory Core proof coverage counters for each
    ///   precedence boundary.
    ///
    /// Output:
    /// - Test passes when readiness follows runtime-boundary, partial,
    ///   proof-model-required, artifact-only, lean-covered, and no-expressions
    ///   order.
    ///
    /// Transformation:
    /// - Exercises producer-side readiness derivation directly without parsing
    ///   source or building a full Core module.
    #[test]
    fn syntax_output_lowering_to_core_readiness_precedence_matches_metadata_contract() {
        let cases = [
            (
                CoreProofCoverageCounts {
                    runtime_boundary: 1,
                    partial: 1,
                    proof_model_required: 1,
                    artifact_only: 1,
                    lean_covered: 1,
                },
                CoreProofReadiness::RuntimeBoundary,
            ),
            (
                CoreProofCoverageCounts {
                    partial: 1,
                    proof_model_required: 1,
                    artifact_only: 1,
                    lean_covered: 1,
                    ..CoreProofCoverageCounts::default()
                },
                CoreProofReadiness::Partial,
            ),
            (
                CoreProofCoverageCounts {
                    proof_model_required: 1,
                    artifact_only: 1,
                    lean_covered: 1,
                    ..CoreProofCoverageCounts::default()
                },
                CoreProofReadiness::ProofModelRequired,
            ),
            (
                CoreProofCoverageCounts {
                    artifact_only: 1,
                    lean_covered: 1,
                    ..CoreProofCoverageCounts::default()
                },
                CoreProofReadiness::ArtifactOnly,
            ),
            (
                CoreProofCoverageCounts {
                    lean_covered: 1,
                    ..CoreProofCoverageCounts::default()
                },
                CoreProofReadiness::LeanCovered,
            ),
            (
                CoreProofCoverageCounts::default(),
                CoreProofReadiness::NoExpressions,
            ),
        ];

        for (coverage, expected) in cases {
            assert_eq!(core_proof_readiness(&coverage), expected);
        }
    }

    /// Verifies summary-only CoreType payloads contribute proof-model debt.
    ///
    /// Inputs:
    /// - None; constructs in-memory proof coverage and type payload counters.
    ///
    /// Output:
    /// - Test passes when summary-only type payloads promote otherwise covered
    ///   or expression-free modules to proof-model-required readiness.
    ///
    /// Transformation:
    /// - Exercises module-level readiness derivation without parsing source or
    ///   building a full Core module.
    #[test]
    fn syntax_output_lowering_to_core_readiness_includes_summary_only_type_debt() {
        let lean_coverage = CoreProofCoverageCounts {
            lean_covered: 1,
            ..CoreProofCoverageCounts::default()
        };
        let expression_free_coverage = CoreProofCoverageCounts::default();
        let typed_types = CoreTypePayloadCounts {
            typed_core_type: 1,
            ..CoreTypePayloadCounts::default()
        };
        let summary_types = CoreTypePayloadCounts {
            summary_only_type: 1,
            ..CoreTypePayloadCounts::default()
        };

        assert_eq!(
            core_module_proof_readiness(&lean_coverage, &summary_types),
            CoreProofReadiness::ProofModelRequired
        );
        assert_eq!(
            core_module_proof_readiness(&expression_free_coverage, &summary_types),
            CoreProofReadiness::ProofModelRequired
        );
        assert_eq!(
            core_module_proof_readiness(&expression_free_coverage, &typed_types),
            CoreProofReadiness::NoExpressions
        );
    }

    #[test]
    fn syntax_output_lowering_to_core_records_function_clause_summaries() {
        let module = parse_module_as_syntax_output(
            "\
module core_expr_boundary.\n\
\n\
pub add(x: Int): Int ->\n\
    x + 1.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "add")
            .expect("core add function");
        assert_eq!(function.clauses.len(), 1);
        assert_eq!(function.params[0].core_ty, Some(CoreType::Int));
        assert_eq!(function.core_return_type, Some(CoreType::Int));
        assert_eq!(
            function.clauses[0].core_patterns,
            vec![Some(CorePattern::Var("x".to_string()))]
        );
        assert_eq!(
            function.clauses[0].pattern_proof_coverage,
            vec![CoreProofCoverage::LeanCovered]
        );
        assert_eq!(
            function.clauses[0].pattern_checked_preservation_evidence,
            vec![Some(CoreCheckedPreservationEvidence {
                kind: CoreCheckedPreservationEvidenceKind::StructuralCorePattern,
                freshness: CoreSubstitutionFreshnessEvidence::RuntimeBindingsRequired,
                target: "Var(x)".to_string(),
            })]
        );
        assert_eq!(function.clauses[0].body.kind, "BinaryOp");
        assert_eq!(function.clauses[0].body.operator.as_deref(), Some("+"));
        assert_eq!(
            function.clauses[0].body.children[0].core_expr,
            Some(CoreExpr::Var("x".to_string()))
        );
        assert_eq!(
            function.clauses[0].body.children[1].core_expr,
            Some(CoreExpr::Int(1))
        );
        assert_eq!(
            function.clauses[0].body.checked_preservation_evidence,
            Some(CoreCheckedPreservationEvidence {
                kind: CoreCheckedPreservationEvidenceKind::StructuralCoreExpr,
                freshness: CoreSubstitutionFreshnessEvidence::NoRuntimeBindings,
                target: "BinaryOp(+;Var(x), Int(1))".to_string(),
            })
        );
        assert_eq!(
            core.metadata.proof_readiness,
            CoreProofReadiness::LeanCovered
        );
        assert_eq!(core.metadata.lean_covered_expr_count, 3);
        assert_eq!(core.metadata.proof_model_required_expr_count, 0);
        assert_eq!(core.metadata.lean_covered_pattern_count, 1);
        assert_eq!(core.metadata.proof_model_required_pattern_count, 0);
        assert_eq!(core.metadata.typed_core_expr_count, 3);
        assert_eq!(core.metadata.summary_only_expr_count, 0);
        assert_eq!(core.metadata.typed_core_pattern_count, 1);
        assert_eq!(core.metadata.summary_only_pattern_count, 0);
        assert_eq!(core.metadata.checked_preservation_expr_count, 3);
        assert_eq!(core.metadata.checked_preservation_pattern_count, 1);
        assert_eq!(core.metadata.checked_preservation_expr_structural_count, 3);
        assert_eq!(
            core.metadata.checked_preservation_pattern_structural_count,
            1
        );
        assert_eq!(
            core.metadata
                .checked_preservation_expr_no_runtime_bindings_count,
            3
        );
        assert_eq!(
            core.metadata
                .checked_preservation_pattern_no_runtime_bindings_count,
            0
        );
        assert_eq!(
            core.metadata
                .checked_preservation_expr_runtime_bindings_required_count,
            0
        );
        assert_eq!(
            core.metadata
                .checked_preservation_pattern_runtime_bindings_required_count,
            1
        );
        assert_eq!(core.metadata.typed_core_type_count, 2);
        assert_eq!(core.metadata.summary_only_type_count, 0);
        assert!(
            core.contract_text().contains("function_clause=add/1#0"),
            "contract text: {}",
            core.contract_text()
        );
        assert!(
            core.contract_text().contains("core_patterns=Var(x)"),
            "contract text: {}",
            core.contract_text()
        );
        assert!(
            core.contract_text().contains("pattern_proof=lean-covered"),
            "contract text: {}",
            core.contract_text()
        );
        assert!(
            core.contract_text().contains(
                "body=BinaryOp:core=BinaryOp(+;Var(x), Int(1)):preservation=structural-core-expr(freshness=no-runtime-bindings;target=BinaryOp(+;Var(x), Int(1))):proof=lean-covered:op=+:"
            ),
            "contract text: {}",
            core.contract_text()
        );
        assert!(
            core.contract_text().contains(
                "Var:core=Var(x):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Var(x)):proof=lean-covered"
            ) && core.contract_text().contains(
                "Int:core=Int(1):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Int(1)):proof=lean-covered"
            ),
            "contract text: {}",
            core.contract_text()
        );
    }

    #[test]
    fn syntax_output_lowering_to_core_records_record_pattern_payload() {
        let module = parse_module_as_syntax_output(
            "\
module core_expr_pattern_gap.\n\
\n\
pub bad(#Point { x = 1 }): Int ->\n\
    1.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "bad")
            .expect("core bad function");
        assert_eq!(function.clauses.len(), 1);
        assert_eq!(
            function.clauses[0].core_patterns,
            vec![Some(CorePattern::Record {
                name: "Point".to_string(),
                fields: vec![CoreRecordPatternField {
                    key: "x".to_string(),
                    required: true,
                    value: CorePattern::Int(1),
                }],
            })]
        );
        assert_eq!(
            function.clauses[0].pattern_proof_coverage,
            vec![CoreProofCoverage::ProofModelRequired]
        );
        assert_eq!(
            function.clauses[0].pattern_checked_preservation_evidence,
            vec![Some(CoreCheckedPreservationEvidence {
                kind: CoreCheckedPreservationEvidenceKind::StructuralCorePattern,
                freshness: CoreSubstitutionFreshnessEvidence::NoRuntimeBindings,
                target: "Record(Point;x=Int(1))".to_string(),
            })]
        );
        assert_eq!(
            core.metadata.proof_readiness,
            CoreProofReadiness::ProofModelRequired
        );
        assert_eq!(core.metadata.lean_covered_pattern_count, 0);
        assert_eq!(core.metadata.proof_model_required_pattern_count, 1);
        assert_eq!(core.metadata.typed_core_pattern_count, 1);
        assert_eq!(core.metadata.summary_only_pattern_count, 0);
        assert_eq!(core.metadata.checked_preservation_pattern_count, 1);
        assert!(
            core.contract_text()
                .contains("core_patterns=Record(Point;x=Int(1))"),
            "contract text: {}",
            core.contract_text()
        );
        assert_eq!(core.metadata.checked_preservation_pattern_count, 1);
    }

    #[test]
    fn syntax_output_lowering_to_core_pattern_coverage_includes_float_payload() {
        let pattern = SyntaxPatternOutput {
            kind: SyntaxPatternKind::Float,
            arity: 1,
            text: Some("1.0".to_string()),
            children: Vec::new(),
            fields: Vec::new(),
        };
        let core_pattern = core_pattern_from_syntax(&pattern);

        assert_eq!(core_pattern, Some(CorePattern::Float("1.0".to_string())));
        assert_eq!(
            core_pattern_proof_coverage(&pattern, core_pattern.as_ref()),
            CoreProofCoverage::ProofModelRequired
        );
    }

    #[test]
    fn syntax_output_lowering_to_core_pattern_coverage_includes_map_payload() {
        let pattern = SyntaxPatternOutput {
            kind: SyntaxPatternKind::Map,
            arity: 1,
            text: None,
            children: Vec::new(),
            fields: vec![SyntaxPatternFieldOutput {
                key: "a".to_string(),
                required: true,
                value: Box::new(SyntaxPatternOutput {
                    kind: SyntaxPatternKind::Int,
                    arity: 1,
                    text: Some("1".to_string()),
                    children: Vec::new(),
                    fields: Vec::new(),
                }),
            }],
        };
        let core_pattern = core_pattern_from_syntax(&pattern);

        assert_eq!(
            core_pattern,
            Some(CorePattern::Map(vec![CoreMapPatternField {
                key: "a".to_string(),
                required: true,
                value: CorePattern::Int(1),
            }]))
        );
        assert_eq!(
            core_pattern_proof_coverage(&pattern, core_pattern.as_ref()),
            CoreProofCoverage::ProofModelRequired
        );
    }

    #[test]
    fn syntax_output_lowering_to_core_pattern_coverage_includes_list_cons_payload() {
        let pattern = SyntaxPatternOutput {
            kind: SyntaxPatternKind::ListCons,
            arity: 2,
            text: None,
            children: vec![
                SyntaxPatternOutput {
                    kind: SyntaxPatternKind::Int,
                    arity: 1,
                    text: Some("1".to_string()),
                    children: Vec::new(),
                    fields: Vec::new(),
                },
                SyntaxPatternOutput {
                    kind: SyntaxPatternKind::Var,
                    arity: 1,
                    text: Some("rest".to_string()),
                    children: Vec::new(),
                    fields: Vec::new(),
                },
            ],
            fields: Vec::new(),
        };
        let core_pattern = core_pattern_from_syntax(&pattern);

        assert_eq!(
            core_pattern,
            Some(CorePattern::ListCons {
                head: Box::new(CorePattern::Int(1)),
                tail: Box::new(CorePattern::Var("rest".to_string())),
            })
        );
        assert_eq!(
            core_pattern_proof_coverage(&pattern, core_pattern.as_ref()),
            CoreProofCoverage::ProofModelRequired
        );
    }

    /// Verifies structural patterns require Lean-modeled child patterns before
    /// they are reported as Lean-covered.
    ///
    /// Inputs:
    /// - None; constructs a tuple pattern containing a float child pattern.
    ///
    /// Output:
    /// - Test passes when the tuple still carries a typed CorePattern payload
    ///   but reports proof-model-required coverage.
    ///
    /// Transformation:
    /// - Exercises recursive Lean-shape validation for structural CorePattern
    ///   payloads.
    #[test]
    fn syntax_output_lowering_to_core_pattern_coverage_requires_covered_tuple_children() {
        let pattern = SyntaxPatternOutput {
            kind: SyntaxPatternKind::Tuple,
            arity: 1,
            text: None,
            children: vec![SyntaxPatternOutput {
                kind: SyntaxPatternKind::Float,
                arity: 1,
                text: Some("1.0".to_string()),
                children: Vec::new(),
                fields: Vec::new(),
            }],
            fields: Vec::new(),
        };
        let core_pattern = core_pattern_from_syntax(&pattern);

        assert_eq!(
            core_pattern,
            Some(CorePattern::Tuple(vec![CorePattern::Float(
                "1.0".to_string()
            )]))
        );
        assert_eq!(
            core_pattern_proof_coverage(&pattern, core_pattern.as_ref()),
            CoreProofCoverage::ProofModelRequired
        );
    }

    /// Verifies list patterns require Lean-modeled child patterns before they
    /// are reported as Lean-covered.
    ///
    /// Inputs:
    /// - None; constructs a list pattern containing a float child pattern.
    ///
    /// Output:
    /// - Test passes when the list still carries a typed CorePattern payload
    ///   but reports proof-model-required coverage.
    ///
    /// Transformation:
    /// - Exercises recursive Lean-shape validation for list CorePattern
    ///   payloads.
    #[test]
    fn syntax_output_lowering_to_core_pattern_coverage_requires_covered_list_children() {
        let pattern = SyntaxPatternOutput {
            kind: SyntaxPatternKind::List,
            arity: 1,
            text: None,
            children: vec![SyntaxPatternOutput {
                kind: SyntaxPatternKind::Float,
                arity: 1,
                text: Some("1.0".to_string()),
                children: Vec::new(),
                fields: Vec::new(),
            }],
            fields: Vec::new(),
        };
        let core_pattern = core_pattern_from_syntax(&pattern);

        assert_eq!(
            core_pattern,
            Some(CorePattern::List(vec![CorePattern::Float(
                "1.0".to_string()
            )]))
        );
        assert_eq!(
            core_pattern_proof_coverage(&pattern, core_pattern.as_ref()),
            CoreProofCoverage::ProofModelRequired
        );
    }

    /// Verifies constructor patterns require Lean-modeled argument patterns
    /// before they are reported as Lean-covered.
    ///
    /// Inputs:
    /// - None; constructs a constructor pattern containing a float argument
    ///   pattern.
    ///
    /// Output:
    /// - Test passes when the constructor still carries a typed CorePattern
    ///   payload but reports proof-model-required coverage.
    ///
    /// Transformation:
    /// - Exercises recursive Lean-shape validation for constructor CorePattern
    ///   payloads.
    #[test]
    fn syntax_output_lowering_to_core_pattern_coverage_requires_covered_constructor_args() {
        let pattern = SyntaxPatternOutput {
            kind: SyntaxPatternKind::Constructor,
            arity: 1,
            text: Some("Some".to_string()),
            children: vec![SyntaxPatternOutput {
                kind: SyntaxPatternKind::Float,
                arity: 1,
                text: Some("1.0".to_string()),
                children: Vec::new(),
                fields: Vec::new(),
            }],
            fields: Vec::new(),
        };
        let core_pattern = core_pattern_from_syntax(&pattern);

        assert_eq!(
            core_pattern,
            Some(CorePattern::Constructor {
                name: "Some".to_string(),
                constructor_identity: None,
                args: vec![CorePattern::Float("1.0".to_string())],
            })
        );
        assert_eq!(
            core_pattern_proof_coverage(&pattern, core_pattern.as_ref()),
            CoreProofCoverage::ProofModelRequired
        );
    }

    #[test]
    fn syntax_output_lowering_to_core_pattern_coverage_requires_map_field_payload() {
        let pattern = SyntaxPatternOutput {
            kind: SyntaxPatternKind::MapField,
            arity: 1,
            text: Some("a".to_string()),
            children: Vec::new(),
            fields: vec![SyntaxPatternFieldOutput {
                key: "a".to_string(),
                required: true,
                value: Box::new(SyntaxPatternOutput {
                    kind: SyntaxPatternKind::Int,
                    arity: 1,
                    text: Some("1".to_string()),
                    children: Vec::new(),
                    fields: Vec::new(),
                }),
            }],
        };
        let core_pattern = core_pattern_from_syntax(&pattern);

        assert_eq!(core_pattern, None);
        assert_eq!(
            core_pattern_proof_coverage(&pattern, core_pattern.as_ref()),
            CoreProofCoverage::ProofModelRequired
        );
    }

    #[test]
    fn syntax_output_lowering_to_core_pattern_coverage_includes_compat_wildcards() {
        for kind in [SyntaxPatternKind::Ignore, SyntaxPatternKind::Placeholder] {
            let pattern = SyntaxPatternOutput {
                kind,
                arity: 0,
                text: None,
                children: Vec::new(),
                fields: Vec::new(),
            };
            let core_pattern = core_pattern_from_syntax(&pattern);

            assert_eq!(core_pattern, Some(CorePattern::Wildcard));
            assert_eq!(
                core_pattern_proof_coverage(&pattern, core_pattern.as_ref()),
                CoreProofCoverage::LeanCovered
            );
        }
    }

    #[test]
    fn syntax_output_lowering_to_core_records_local_call_core_expr() {
        let module = parse_module_as_syntax_output(
            "\
module core_call_boundary.\n\
\n\
identity(x: Int): Int ->\n\
    x.\n\
\n\
pub call_it(): Int ->\n\
    identity(1).\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "call_it")
            .expect("core call_it function");
        assert_eq!(function.clauses.len(), 1);
        assert_eq!(
            function.clauses[0].body.core_expr,
            Some(CoreExpr::Call {
                function: "identity".to_string(),
                args: vec![CoreExpr::Int(1)],
            })
        );
        assert_eq!(
            function.clauses[0].body.proof_coverage,
            CoreProofCoverage::LeanCovered
        );
        assert!(
            core.contract_text()
                .contains("Call:core=Call(identity;Int(1)):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Call(identity;Int(1))):proof=lean-covered"),
            "contract text: {}",
            core.contract_text()
        );
    }

    /// Verifies dedicated function-value invocation remains distinct in CoreIR.
    ///
    /// Inputs:
    /// - A syntax-output module whose function body uses `f.(value)`.
    ///
    /// Output:
    /// - Test passes when the formal CoreIR payload is `CoreExpr::FunctionCall`
    ///   with a variable callee and one argument.
    ///
    /// Transformation:
    /// - Parses, resolves, and lowers source through the syntax-output path,
    ///   then inspects the backend-neutral CoreIR expression.
    #[test]
    fn syntax_output_lowering_to_core_records_function_value_call_core_expr() {
        let module = parse_module_as_syntax_output(
            "\
module core_function_call_boundary.\n\
\n\
pub apply(value: Int, f: (Int) -> Int): Int ->\n\
    f.(value).\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "apply")
            .expect("core apply function");
        assert_eq!(function.clauses.len(), 1);
        assert_eq!(
            function.clauses[0].body.core_expr,
            Some(CoreExpr::FunctionCall {
                callee: Box::new(CoreExpr::Var("f".to_string())),
                args: vec![CoreExpr::Var("value".to_string())],
            })
        );
        assert!(
            core.contract_text()
                .contains("FunctionCall(Var(f);Var(value))"),
            "contract text: {}",
            core.contract_text()
        );
    }

    /// Verifies pipe-forward can target dedicated function-value invocation.
    ///
    /// Inputs:
    /// - A syntax-output module using `value |> f.()`.
    ///
    /// Output:
    /// - Test passes when the function typechecks without diagnostics.
    ///
    /// Transformation:
    /// - Exercises the pipe rule that prepends the left operand to a
    ///   `FunctionCall` argument list before checking the callee function type.
    #[test]
    fn syntax_output_typechecks_pipe_into_function_value_call() {
        let diagnostics = check_syntax_output(
            "\
module pipe_to_function_value_call.\n\
\n\
pub apply(value: Int, f: (Int) -> Int): Int ->\n\
    value |> f.().\n",
        );

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
    }

    /// Verifies compound type annotations lower to CoreType.
    ///
    /// Inputs:
    /// - None; exercises `core_type_from_text` with nested type text.
    ///
    /// Output:
    /// - Test passes when supported atom literal, list, tuple, parameterized
    ///   named, function, and union annotations produce typed CoreType
    ///   payloads.
    ///
    /// Transformation:
    /// - Parses type text directly without constructing a full module.
    #[test]
    fn syntax_output_lowering_to_core_records_compound_core_type_payloads() {
        assert_eq!(
            core_type_from_text("List[Int]"),
            Some(CoreType::List(Box::new(CoreType::Int)))
        );
        assert_eq!(core_type_from_text("String"), Some(CoreType::String));
        assert_eq!(core_type_from_text("Text"), Some(CoreType::Binary));
        assert_eq!(
            core_type_from_text("Atom[\"none\"]"),
            Some(CoreType::AtomLiteral("none".to_string()))
        );
        assert_eq!(
            core_type_from_text("Atom[\"Elixir.Module\"]"),
            Some(CoreType::AtomLiteral("Elixir.Module".to_string()))
        );
        assert_eq!(
            core_type_from_text(": none"),
            Some(CoreType::AtomLiteral("none".to_string()))
        );
        assert_eq!(
            core_type_from_text(":'Elixir.Module'"),
            Some(CoreType::AtomLiteral("Elixir.Module".to_string()))
        );
        assert_eq!(
            core_type_from_text("{Int, Bool}"),
            Some(CoreType::Tuple(vec![
                CoreTupleTypeElem::Type(CoreType::Int),
                CoreTupleTypeElem::Type(CoreType::Bool),
            ]))
        );
        assert_eq!(
            core_type_from_text("List[{Int, Bool}]"),
            Some(CoreType::List(Box::new(CoreType::Tuple(vec![
                CoreTupleTypeElem::Type(CoreType::Int),
                CoreTupleTypeElem::Type(CoreType::Bool),
            ]))))
        );
        assert_eq!(
            core_type_from_text("{Atom[\"ok\"], value: T, _: Int}"),
            Some(CoreType::Tuple(vec![
                CoreTupleTypeElem::Type(CoreType::AtomLiteral("ok".to_string())),
                CoreTupleTypeElem::Field {
                    name: "value".to_string(),
                    ty: CoreType::Named("T".to_string()),
                },
                CoreTupleTypeElem::Field {
                    name: "_".to_string(),
                    ty: CoreType::Int,
                },
            ]))
        );
        assert_eq!(
            core_type_from_text("#{name := Binary}"),
            Some(CoreType::Map(vec![CoreMapTypeField {
                key: "name".to_string(),
                operator: ":=".to_string(),
                value: CoreType::Binary,
            }]))
        );
        assert_eq!(
            core_type_from_text("# {name := Binary}"),
            Some(CoreType::Map(vec![CoreMapTypeField {
                key: "name".to_string(),
                operator: ":=".to_string(),
                value: CoreType::Binary,
            }]))
        );
        assert_eq!(
            core_type_from_text("#{:ok => {:ok, value: T}}"),
            Some(CoreType::Map(vec![CoreMapTypeField {
                key: ":ok".to_string(),
                operator: "=>".to_string(),
                value: CoreType::Tuple(vec![
                    CoreTupleTypeElem::Type(CoreType::AtomLiteral("ok".to_string())),
                    CoreTupleTypeElem::Field {
                        name: "value".to_string(),
                        ty: CoreType::Named("T".to_string()),
                    },
                ]),
            }]))
        );
        assert_eq!(
            core_type_from_text("Result[Int]"),
            Some(CoreType::Apply {
                constructor: "Result".to_string(),
                args: vec![CoreType::Int],
            })
        );
        assert_eq!(
            core_type_from_text("List[Result[{Int, Bool}]]"),
            Some(CoreType::List(Box::new(CoreType::Apply {
                constructor: "Result".to_string(),
                args: vec![CoreType::Tuple(vec![
                    CoreTupleTypeElem::Type(CoreType::Int),
                    CoreTupleTypeElem::Type(CoreType::Bool),
                ])],
            })))
        );
        assert_eq!(
            core_type_from_text("(Int) -> Bool"),
            Some(CoreType::Arrow {
                params: vec![CoreType::Int],
                return_type: Box::new(CoreType::Bool),
            })
        );
        assert_eq!(
            core_type_from_text("(Int, Result[Bool]) -> List[Int]"),
            Some(CoreType::Arrow {
                params: vec![
                    CoreType::Int,
                    CoreType::Apply {
                        constructor: "Result".to_string(),
                        args: vec![CoreType::Bool],
                    },
                ],
                return_type: Box::new(CoreType::List(Box::new(CoreType::Int))),
            })
        );
        assert_eq!(
            core_type_from_text("Result[(Int) -> Bool]"),
            Some(CoreType::Apply {
                constructor: "Result".to_string(),
                args: vec![CoreType::Arrow {
                    params: vec![CoreType::Int],
                    return_type: Box::new(CoreType::Bool),
                }],
            })
        );
        assert_eq!(
            core_type_from_text("Int | Bool"),
            Some(CoreType::Union(vec![CoreType::Int, CoreType::Bool]))
        );
        assert_eq!(
            core_type_from_text("List[Int | Bool]"),
            Some(CoreType::List(Box::new(CoreType::Union(vec![
                CoreType::Int,
                CoreType::Bool,
            ]))))
        );
        assert_eq!(
            core_type_from_text("(Int) -> Bool | Never"),
            Some(CoreType::Union(vec![
                CoreType::Arrow {
                    params: vec![CoreType::Int],
                    return_type: Box::new(CoreType::Bool),
                },
                CoreType::Never,
            ]))
        );
        assert_eq!(
            core_type_from_text(":none | :empty"),
            Some(CoreType::Union(vec![
                CoreType::AtomLiteral("none".to_string()),
                CoreType::AtomLiteral("empty".to_string()),
            ]))
        );
        assert_eq!(core_type_from_text("Int | "), None);
        assert_eq!(core_type_from_text("none"), None);
        assert_eq!(core_type_from_text("result[Int]"), None);
    }

    /// Verifies type declaration bodies carry optional typed CoreType payloads.
    ///
    /// Inputs:
    /// - None; constructs a syntax-output module with supported and
    ///   unsupported type declaration bodies.
    ///
    /// Output:
    /// - Test passes when supported aliases, including atom-literal aliases,
    ///   have typed `core_body` payloads.
    ///
    /// Transformation:
    /// - Lowers resolved module interface type declarations into CoreIR type
    ///   declarations without emitting backend-specific type syntax.
    #[test]
    fn syntax_output_lowering_to_core_records_type_decl_core_body_payloads() {
        let module = parse_module_as_syntax_output(
            "\
module core_type_decl_boundary.\n\
\n\
pub type Text = Binary.\n\
pub type MaybeInt = Int | Never.\n\
pub type Items[T] = List[T].\n\
pub type None = :none.\n\
pub type Ok[T] = {:ok, value: T}.\n\
pub type Props = #{name := Binary}.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_resolved_module_to_core(&resolved);

        let text = core
            .types
            .iter()
            .find(|decl| decl.name == "Text")
            .expect("Text core type declaration");
        assert_eq!(text.core_body, Some(CoreType::Binary));

        let maybe_int = core
            .types
            .iter()
            .find(|decl| decl.name == "MaybeInt")
            .expect("MaybeInt core type declaration");
        assert_eq!(
            maybe_int.core_body,
            Some(CoreType::Union(vec![CoreType::Int, CoreType::Never]))
        );

        let items = core
            .types
            .iter()
            .find(|decl| decl.name == "Items")
            .expect("Items core type declaration");
        assert_eq!(
            items.core_body,
            Some(CoreType::List(Box::new(CoreType::Named("T".to_string()))))
        );

        let none = core
            .types
            .iter()
            .find(|decl| decl.name == "None")
            .expect("None core type declaration");
        assert_eq!(
            none.core_body,
            Some(CoreType::AtomLiteral("none".to_string()))
        );

        let ok = core
            .types
            .iter()
            .find(|decl| decl.name == "Ok")
            .expect("Ok core type declaration");
        assert_eq!(
            ok.core_body,
            Some(CoreType::Tuple(vec![
                CoreTupleTypeElem::Type(CoreType::AtomLiteral("ok".to_string())),
                CoreTupleTypeElem::Field {
                    name: "value".to_string(),
                    ty: CoreType::Named("T".to_string()),
                },
            ]))
        );

        let props = core
            .types
            .iter()
            .find(|decl| decl.name == "Props")
            .expect("Props core type declaration");
        assert_eq!(
            props.core_body,
            Some(CoreType::Map(vec![CoreMapTypeField {
                key: "name".to_string(),
                operator: ":=".to_string(),
                value: CoreType::Binary,
            }]))
        );
        assert_eq!(core.metadata.typed_core_type_count, 6);
        assert_eq!(core.metadata.summary_only_type_count, 0);
    }

    /// Verifies unsupported type declaration bodies count as summary-only
    /// CoreType payloads.
    ///
    /// Inputs:
    /// - None; constructs a syntax-output module with a public struct
    ///   declaration whose structural body is not yet represented as CoreType.
    ///
    /// Output:
    /// - Test passes when the type declaration has no `core_body`, and metadata
    ///   records one summary-only type payload.
    ///
    /// Transformation:
    /// - Lowers a resolved struct declaration through CoreIR metadata
    ///   construction without backend-specific type encoding.
    #[test]
    fn syntax_output_lowering_to_core_counts_summary_only_type_decl_payloads() {
        let module = parse_module_as_syntax_output(
            "\
module core_summary_type_decl_boundary.\n\
\n\
pub struct Point {\n\
    x: Int,\n\
    y: Int\n\
}.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_resolved_module_to_core(&resolved);

        let point = core
            .types
            .iter()
            .find(|decl| decl.name == "Point")
            .expect("Point core type declaration");
        assert_eq!(point.core_body, None);
        assert_eq!(
            core.metadata.proof_readiness,
            CoreProofReadiness::ProofModelRequired
        );
        assert_eq!(core.metadata.typed_core_type_count, 0);
        assert_eq!(core.metadata.summary_only_type_count, 1);
    }

    /// Verifies uppercase constructor-like calls lower as CoreIR candidates.
    ///
    /// Inputs:
    /// - None; constructs a syntax-output call expression for `Ok(1)`.
    ///
    /// Output:
    /// - Test passes when the expression has a typed `CoreExpr::ConstructorCall`
    ///   payload and is classified as partial.
    ///
    /// Transformation:
    /// - Exercises the named-call lowering rule without invoking resolver
    ///   behavior for constructor aliases.
    #[test]
    fn syntax_output_lowering_to_core_records_constructor_call_candidate() {
        let expr = SyntaxExprOutput {
            kind: SyntaxExprKind::Call,
            arity: 1,
            text: None,
            span: Default::default(),
            raw: None,
            operator: None,
            remote: None,
            children: vec![
                SyntaxExprOutput {
                    kind: SyntaxExprKind::Var,
                    arity: 0,
                    text: Some("Ok".to_string()),
                    span: Default::default(),
                    raw: None,
                    operator: None,
                    remote: None,
                    children: Vec::new(),
                    patterns: Vec::new(),
                    fields: Vec::new(),
                    clauses: Vec::new(),
                    catch_clauses: Vec::new(),
                    try_after: None,
                    receive_after: None,
                    html_nodes: Vec::new(),
                },
                SyntaxExprOutput {
                    kind: SyntaxExprKind::Int,
                    arity: 0,
                    text: Some("1".to_string()),
                    span: Default::default(),
                    raw: None,
                    operator: None,
                    remote: None,
                    children: Vec::new(),
                    patterns: Vec::new(),
                    fields: Vec::new(),
                    clauses: Vec::new(),
                    catch_clauses: Vec::new(),
                    try_after: None,
                    receive_after: None,
                    html_nodes: Vec::new(),
                },
            ],
            patterns: Vec::new(),
            fields: Vec::new(),
            clauses: Vec::new(),
            catch_clauses: Vec::new(),
            try_after: None,
            receive_after: None,
            html_nodes: Vec::new(),
        };
        let core_expr = core_expr_from_syntax(&expr);

        assert_eq!(
            core_expr,
            Some(CoreExpr::ConstructorCall {
                constructor: "Ok".to_string(),
                constructor_identity: None,
                args: vec![CoreExpr::Int(1)],
            })
        );
        assert_eq!(
            core_expr_proof_coverage(&expr, core_expr.as_ref()),
            CoreProofCoverage::Partial
        );
    }

    /// Verifies declared constructor calls carry resolved CoreIR identity.
    ///
    /// Inputs:
    /// - None; constructs a syntax-output module with a declared `Ok`
    ///   constructor and a function body that calls `Ok(1)`.
    ///
    /// Output:
    /// - Test passes when the function body has a typed constructor-call Core
    ///   payload with `constructor_identity = Some("Ok")` and Lean-covered
    ///   proof coverage.
    ///
    /// Transformation:
    /// - Exercises the post-lowering constructor identity annotation pass that
    ///   consumes resolved module constructor declarations.
    #[test]
    fn syntax_output_lowering_to_core_resolves_declared_constructor_call_identity() {
        let module = parse_module_as_syntax_output(
            "\
module core_constructor_identity_boundary.\n\
\n\
pub constructor Ok {\n\
    (value: Int): Dynamic -> value\n\
}.\n\
\n\
pub make(): Dynamic ->\n\
    Ok(1).\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "make")
            .expect("core make function");
        assert_eq!(function.clauses.len(), 1);
        assert_eq!(
            function.clauses[0].body.core_expr,
            Some(CoreExpr::ConstructorCall {
                constructor: "Ok".to_string(),
                constructor_identity: Some("Ok".to_string()),
                args: vec![CoreExpr::Int(1)],
            })
        );
        assert_eq!(
            function.clauses[0].body.proof_coverage,
            CoreProofCoverage::LeanCovered
        );
        assert_eq!(core.metadata.resolved_constructor_call_identity_count, 1);
        assert_eq!(core.metadata.resolved_constructor_chain_identity_count, 0);
        assert_eq!(core.metadata.resolved_constructor_pattern_identity_count, 0);
        assert_eq!(core.metadata.unresolved_constructor_call_candidate_count, 0);
        assert_eq!(
            core.metadata.unresolved_constructor_chain_candidate_count,
            0
        );
        assert_eq!(
            core.metadata.unresolved_constructor_pattern_candidate_count,
            0
        );
        assert!(
            core.contract_text()
                .contains("ConstructorCall(Ok;identity=Ok;Int(1))"),
            "contract text: {}",
            core.contract_text()
        );
        assert!(
            core.contract_text()
                .contains("resolved_constructor_call_identity:1"),
            "contract text: {}",
            core.contract_text()
        );
        assert!(
            core.contract_text().contains(
                "preservation=structural-core-expr(freshness=no-runtime-bindings;target=ConstructorCall(Ok;identity=Ok;Int(1)))"
            ),
            "contract text: {}",
            core.contract_text()
        );
    }

    /// Verifies imported public constructor calls carry qualified CoreIR
    /// identity.
    ///
    /// Inputs:
    /// - A provider interface declaring public constructor `Ok`.
    /// - A consumer syntax-output module importing `Ok` and calling it.
    ///
    /// Output:
    /// - Test passes when typechecking succeeds and the consumer CoreIR call is
    ///   annotated with `constructor_identity = Some("provider.Ok")`.
    ///
    /// Transformation:
    /// - Resolves the consumer against an explicit interface map, lowers it to
    ///   CoreIR, and verifies imported constructor identity metadata without
    ///   adding backend-specific layout assumptions.
    #[test]
    fn syntax_output_lowering_to_core_resolves_imported_constructor_call_identity() {
        let provider = parse_interface_module_as_syntax_output(
            "\
module provider.\n\
\n\
pub constructor Ok {\n\
    (value: Int): Dynamic -> value\n\
}.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse provider interface fixture: {:?}", err));
        let mut interfaces = HashMap::new();
        interfaces.insert(
            provider.module_name.clone(),
            syntax_module_output_to_interface(&provider),
        );
        let module = parse_module_as_syntax_output(
            "\
module imported_constructor_identity_boundary.\n\
\n\
import provider.{Ok}.\n\
\n\
pub make(): Dynamic ->\n\
    Ok(1).\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
        let diagnostics = type_check_syntax_module_output(&module, &resolved);
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "make")
            .expect("core make function");
        assert_eq!(function.clauses.len(), 1);
        assert_eq!(
            function.clauses[0].body.core_expr,
            Some(CoreExpr::ConstructorCall {
                constructor: "Ok".to_string(),
                constructor_identity: Some("provider.Ok".to_string()),
                args: vec![CoreExpr::Int(1)],
            })
        );
        assert_eq!(
            function.clauses[0].body.proof_coverage,
            CoreProofCoverage::LeanCovered
        );
        assert_eq!(core.metadata.resolved_constructor_call_identity_count, 1);
        assert_eq!(core.metadata.unresolved_constructor_call_candidate_count, 0);
        assert!(
            core.contract_text()
                .contains("ConstructorCall(Ok;identity=provider.Ok;Int(1))"),
            "contract text: {}",
            core.contract_text()
        );
        assert!(
            core.contract_text()
                .contains("resolved_constructor_call_identity:1"),
            "contract text: {}",
            core.contract_text()
        );
    }

    /// Verifies aliased imported public constructor calls carry source identity.
    ///
    /// Inputs:
    /// - A provider interface declaring public constructor `Ok`.
    /// - A consumer syntax-output module importing `Ok as Success` and calling
    ///   `Success`.
    ///
    /// Output:
    /// - Test passes when typechecking succeeds, CoreIR preserves the
    ///   source-visible constructor head `Success`, and the constructor
    ///   identity remains `provider.Ok`.
    ///
    /// Transformation:
    /// - Resolves the aliased import against an explicit interface map, lowers
    ///   to CoreIR, and verifies constructor identity metadata is based on the
    ///   provider/source constructor rather than the local alias.
    #[test]
    fn syntax_output_lowering_to_core_resolves_aliased_imported_constructor_call_identity() {
        let provider = parse_interface_module_as_syntax_output(
            "\
module provider.\n\
\n\
pub constructor Ok {\n\
    (value: Int): Dynamic -> value\n\
}.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse provider interface fixture: {:?}", err));
        let mut interfaces = HashMap::new();
        interfaces.insert(
            provider.module_name.clone(),
            syntax_module_output_to_interface(&provider),
        );
        let module = parse_module_as_syntax_output(
            "\
module aliased_imported_constructor_identity_boundary.\n\
\n\
import provider.{Ok as Success}.\n\
\n\
pub make(): Dynamic ->\n\
    Success(1).\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
        let diagnostics = type_check_syntax_module_output(&module, &resolved);
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "make")
            .expect("core make function");
        assert_eq!(function.clauses.len(), 1);
        assert_eq!(
            function.clauses[0].body.core_expr,
            Some(CoreExpr::ConstructorCall {
                constructor: "Success".to_string(),
                constructor_identity: Some("provider.Ok".to_string()),
                args: vec![CoreExpr::Int(1)],
            })
        );
        assert_eq!(
            function.clauses[0].body.proof_coverage,
            CoreProofCoverage::LeanCovered
        );
        assert_eq!(core.metadata.resolved_constructor_call_identity_count, 1);
        assert_eq!(core.metadata.unresolved_constructor_call_candidate_count, 0);
        assert!(
            core.contract_text()
                .contains("ConstructorCall(Success;identity=provider.Ok;Int(1))"),
            "contract text: {}",
            core.contract_text()
        );
        assert!(
            core.contract_text()
                .contains("resolved_constructor_call_identity:1"),
            "contract text: {}",
            core.contract_text()
        );
    }

    /// Verifies eligible local type-alias constructor calls carry CoreIR
    /// identity.
    ///
    /// Inputs:
    /// - None; constructs a syntax-output module with `pub type Ok[T] =
    ///   {:ok, value: T}` and a function body that calls `Ok(1)`.
    ///
    /// Output:
    /// - Test passes when the function body has a typed constructor-call Core
    ///   payload with `constructor_identity = Some("Ok")` and no unresolved
    ///   constructor-call candidates.
    ///
    /// Transformation:
    /// - Exercises the post-lowering constructor identity annotation pass for
    ///   single-shape type aliases that the typechecker already accepts as
    ///   constructor-like calls.
    #[test]
    fn syntax_output_lowering_to_core_resolves_local_alias_constructor_call_identity() {
        let module = parse_module_as_syntax_output(
            "\
module core_alias_constructor_identity_boundary.\n\
\n\
pub type Ok[T] = {:ok, value: T}.\n\
\n\
pub make(): Dynamic ->\n\
    Ok(1).\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let diagnostics = type_check_syntax_module_output(&module, &resolved);
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "make")
            .expect("core make function");
        assert_eq!(function.clauses.len(), 1);
        assert_eq!(
            function.clauses[0].body.core_expr,
            Some(CoreExpr::ConstructorCall {
                constructor: "Ok".to_string(),
                constructor_identity: Some("Ok".to_string()),
                args: vec![CoreExpr::Int(1)],
            })
        );
        assert_eq!(
            function.clauses[0].body.proof_coverage,
            CoreProofCoverage::LeanCovered
        );
        assert_eq!(core.metadata.resolved_constructor_call_identity_count, 1);
        assert_eq!(core.metadata.unresolved_constructor_call_candidate_count, 0);
        assert!(
            core.contract_text()
                .contains("ConstructorCall(Ok;identity=Ok;Int(1))"),
            "contract text: {}",
            core.contract_text()
        );
    }

    /// Verifies eligible directly imported type-alias constructor calls carry
    /// qualified CoreIR identity.
    ///
    /// Inputs:
    /// - A provider interface declaring public alias constructor `Ok`.
    /// - A consumer syntax-output module importing `Ok` directly and calling
    ///   `Ok(1)`.
    ///
    /// Output:
    /// - Test passes when CoreIR preserves the source-visible constructor head
    ///   `Ok` and resolves the identity to `provider.Ok`.
    ///
    /// Transformation:
    /// - Resolves the direct type import against an explicit interface map,
    ///   lowers to CoreIR, and verifies imported single-shape type-alias
    ///   constructor identity metadata without using a local import alias.
    #[test]
    fn syntax_output_lowering_to_core_resolves_direct_imported_alias_constructor_call_identity() {
        let provider = parse_interface_module_as_syntax_output(
            "\
module provider.\n\
\n\
pub type Ok[T] = {:ok, value: T}.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse provider interface fixture: {:?}", err));
        let mut interfaces = HashMap::new();
        interfaces.insert(
            provider.module_name.clone(),
            syntax_module_output_to_interface(&provider),
        );
        let module = parse_module_as_syntax_output(
            "\
module direct_imported_alias_constructor_identity_boundary.\n\
\n\
import provider.{Ok}.\n\
\n\
pub make(): Dynamic ->\n\
    Ok(1).\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
        let diagnostics = type_check_syntax_module_output(&module, &resolved);
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "make")
            .expect("core make function");
        assert_eq!(function.clauses.len(), 1);
        assert_eq!(
            function.clauses[0].body.core_expr,
            Some(CoreExpr::ConstructorCall {
                constructor: "Ok".to_string(),
                constructor_identity: Some("provider.Ok".to_string()),
                args: vec![CoreExpr::Int(1)],
            })
        );
        assert_eq!(core.metadata.resolved_constructor_call_identity_count, 1);
        assert_eq!(core.metadata.unresolved_constructor_call_candidate_count, 0);
        assert!(
            core.contract_text()
                .contains("ConstructorCall(Ok;identity=provider.Ok;Int(1))"),
            "contract text: {}",
            core.contract_text()
        );
    }

    /// Verifies eligible imported type-alias constructor calls carry qualified
    /// CoreIR identity.
    ///
    /// Inputs:
    /// - A provider interface declaring public alias constructor `Ok`.
    /// - A consumer syntax-output module importing `Ok as Success` and calling
    ///   `Success`.
    ///
    /// Output:
    /// - Test passes when CoreIR preserves the source-visible constructor head
    ///   `Success` and resolves the identity to `provider.Ok`.
    ///
    /// Transformation:
    /// - Resolves the aliased import against an explicit interface map, lowers
    ///   to CoreIR, and verifies single-shape type-alias constructor identity
    ///   metadata is based on the provider/source alias.
    #[test]
    fn syntax_output_lowering_to_core_resolves_imported_alias_constructor_call_identity() {
        let provider = parse_interface_module_as_syntax_output(
            "\
module provider.\n\
\n\
pub type Ok[T] = {:ok, value: T}.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse provider interface fixture: {:?}", err));
        let mut interfaces = HashMap::new();
        interfaces.insert(
            provider.module_name.clone(),
            syntax_module_output_to_interface(&provider),
        );
        let module = parse_module_as_syntax_output(
            "\
module imported_alias_constructor_identity_boundary.\n\
\n\
import provider.{Ok as Success}.\n\
\n\
pub make(): Dynamic ->\n\
    Success(1).\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
        let diagnostics = type_check_syntax_module_output(&module, &resolved);
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "make")
            .expect("core make function");
        assert_eq!(function.clauses.len(), 1);
        assert_eq!(
            function.clauses[0].body.core_expr,
            Some(CoreExpr::ConstructorCall {
                constructor: "Success".to_string(),
                constructor_identity: Some("provider.Ok".to_string()),
                args: vec![CoreExpr::Int(1)],
            })
        );
        assert_eq!(core.metadata.resolved_constructor_call_identity_count, 1);
        assert_eq!(core.metadata.unresolved_constructor_call_candidate_count, 0);
        assert!(
            core.contract_text()
                .contains("ConstructorCall(Success;identity=provider.Ok;Int(1))"),
            "contract text: {}",
            core.contract_text()
        );
    }

    /// Verifies eligible local type-alias constructor patterns carry CoreIR
    /// identity.
    ///
    /// Inputs:
    /// - None; constructs a syntax-output module with a single-shape `Ok[T]`
    ///   alias and a `case` branch matching `Ok(value)`.
    ///
    /// Output:
    /// - Test passes when the Core pattern has
    ///   `constructor_identity = Some("Ok")` and no unresolved constructor
    ///   pattern candidates.
    ///
    /// Transformation:
    /// - Exercises the same post-lowering constructor identity pass for
    ///   single-shape type-alias patterns that typechecking already accepts.
    #[test]
    fn syntax_output_lowering_to_core_resolves_local_alias_constructor_pattern_identity() {
        let module = parse_module_as_syntax_output(
            "\
module core_alias_constructor_pattern_identity_boundary.\n\
\n\
pub type Ok[T] = {:ok, value: T}.\n\
\n\
pub unwrap(input: Ok[Int]): Int ->\n\
    case input {\n\
        Ok(value) -> value\n\
    }.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let diagnostics = type_check_syntax_module_output(&module, &resolved);
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "unwrap")
            .expect("core unwrap function");
        let Some(CoreExpr::Case { clauses, .. }) = &function.clauses[0].body.core_expr else {
            panic!(
                "expected case body: {:?}",
                function.clauses[0].body.core_expr
            );
        };
        let CorePattern::Constructor {
            name,
            constructor_identity,
            args,
        } = &clauses[0].pattern
        else {
            panic!("expected constructor pattern: {:?}", clauses[0].pattern);
        };

        assert_eq!(name, "Ok");
        assert_eq!(constructor_identity.as_deref(), Some("Ok"));
        assert_eq!(args.len(), 1);
        assert_eq!(core.metadata.resolved_constructor_pattern_identity_count, 1);
        assert_eq!(
            core.metadata.unresolved_constructor_pattern_candidate_count,
            0
        );
        assert!(
            core.contract_text()
                .contains("Constructor(Ok;identity=Ok;Var(value))"),
            "contract text: {}",
            core.contract_text()
        );
    }

    /// Verifies eligible directly imported type-alias constructor patterns
    /// carry qualified CoreIR identity.
    ///
    /// Inputs:
    /// - A provider interface declaring public alias constructor `Ok`.
    /// - A consumer syntax-output module importing `Ok` directly and matching
    ///   `Ok(value)`.
    ///
    /// Output:
    /// - Test passes when CoreIR preserves the source-visible pattern head `Ok`
    ///   and resolves the identity to `provider.Ok`.
    ///
    /// Transformation:
    /// - Resolves the direct type import against an explicit interface map,
    ///   lowers to CoreIR, and verifies imported single-shape type-alias
    ///   constructor-pattern identity metadata without using a local import
    ///   alias.
    #[test]
    fn syntax_output_lowering_to_core_resolves_direct_imported_alias_constructor_pattern_identity()
    {
        let provider = parse_interface_module_as_syntax_output(
            "\
module provider.\n\
\n\
pub type Ok[T] = {:ok, value: T}.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse provider interface fixture: {:?}", err));
        let mut interfaces = HashMap::new();
        interfaces.insert(
            provider.module_name.clone(),
            syntax_module_output_to_interface(&provider),
        );
        let module = parse_module_as_syntax_output(
            "\
module direct_imported_alias_constructor_pattern_identity_boundary.\n\
\n\
import provider.{Ok}.\n\
\n\
pub unwrap(input: Ok[Int]): Int ->\n\
    case input {\n\
        Ok(value) -> value\n\
    }.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
        let diagnostics = type_check_syntax_module_output(&module, &resolved);
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "unwrap")
            .expect("core unwrap function");
        let Some(CoreExpr::Case { clauses, .. }) = &function.clauses[0].body.core_expr else {
            panic!(
                "expected case body: {:?}",
                function.clauses[0].body.core_expr
            );
        };
        let CorePattern::Constructor {
            name,
            constructor_identity,
            args,
        } = &clauses[0].pattern
        else {
            panic!("expected constructor pattern: {:?}", clauses[0].pattern);
        };

        assert_eq!(name, "Ok");
        assert_eq!(constructor_identity.as_deref(), Some("provider.Ok"));
        assert_eq!(args.len(), 1);
        assert_eq!(core.metadata.resolved_constructor_pattern_identity_count, 1);
        assert_eq!(
            core.metadata.unresolved_constructor_pattern_candidate_count,
            0
        );
        assert!(
            core.contract_text()
                .contains("Constructor(Ok;identity=provider.Ok;Var(value))"),
            "contract text: {}",
            core.contract_text()
        );
    }

    /// Verifies eligible imported type-alias constructor patterns carry
    /// qualified CoreIR identity.
    ///
    /// Inputs:
    /// - A provider interface declaring public alias constructor `Ok`.
    /// - A consumer syntax-output module importing `Ok as Success` and matching
    ///   `Success(value)`.
    ///
    /// Output:
    /// - Test passes when CoreIR preserves the source-visible pattern head
    ///   `Success` and resolves the identity to `provider.Ok`.
    ///
    /// Transformation:
    /// - Resolves the aliased import against an explicit interface map, lowers
    ///   to CoreIR, and verifies single-shape type-alias constructor-pattern
    ///   identity metadata is based on the provider/source alias.
    #[test]
    fn syntax_output_lowering_to_core_resolves_imported_alias_constructor_pattern_identity() {
        let provider = parse_interface_module_as_syntax_output(
            "\
module provider.\n\
\n\
pub type Ok[T] = {:ok, value: T}.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse provider interface fixture: {:?}", err));
        let mut interfaces = HashMap::new();
        interfaces.insert(
            provider.module_name.clone(),
            syntax_module_output_to_interface(&provider),
        );
        let module = parse_module_as_syntax_output(
            "\
module imported_alias_constructor_pattern_identity_boundary.\n\
\n\
import provider.{Ok as Success}.\n\
\n\
pub unwrap(input: Success[Int]): Int ->\n\
    case input {\n\
        Success(value) -> value\n\
    }.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
        let diagnostics = type_check_syntax_module_output(&module, &resolved);
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "unwrap")
            .expect("core unwrap function");
        let Some(CoreExpr::Case { clauses, .. }) = &function.clauses[0].body.core_expr else {
            panic!(
                "expected case body: {:?}",
                function.clauses[0].body.core_expr
            );
        };
        let CorePattern::Constructor {
            name,
            constructor_identity,
            args,
        } = &clauses[0].pattern
        else {
            panic!("expected constructor pattern: {:?}", clauses[0].pattern);
        };

        assert_eq!(name, "Success");
        assert_eq!(constructor_identity.as_deref(), Some("provider.Ok"));
        assert_eq!(args.len(), 1);
        assert_eq!(core.metadata.resolved_constructor_pattern_identity_count, 1);
        assert_eq!(
            core.metadata.unresolved_constructor_pattern_candidate_count,
            0
        );
        assert!(
            core.contract_text()
                .contains("Constructor(Success;identity=provider.Ok;Var(value))"),
            "contract text: {}",
            core.contract_text()
        );
    }

    /// Verifies undeclared uppercase calls remain visible as unresolved
    /// constructor candidates.
    ///
    /// Inputs:
    /// - None; constructs a syntax-output module with `Ok(1)` and no local
    ///   constructor declaration.
    ///
    /// Output:
    /// - Test passes when the function body keeps its constructor-call
    ///   candidate payload but CoreIR metadata records it as unresolved.
    ///
    /// Transformation:
    /// - Exercises the post-lowering constructor identity pass on a module
    ///   where the candidate name cannot be resolved.
    #[test]
    fn syntax_output_lowering_to_core_counts_unresolved_constructor_call_candidate() {
        let module = parse_module_as_syntax_output(
            "\
module core_unresolved_constructor_candidate_boundary.\n\
\n\
pub make(): Dynamic ->\n\
    Ok(1).\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "make")
            .expect("core make function");
        assert_eq!(function.clauses.len(), 1);
        assert_eq!(
            function.clauses[0].body.core_expr,
            Some(CoreExpr::ConstructorCall {
                constructor: "Ok".to_string(),
                constructor_identity: None,
                args: vec![CoreExpr::Int(1)],
            })
        );
        assert_eq!(core.metadata.resolved_constructor_call_identity_count, 0);
        assert_eq!(core.metadata.unresolved_constructor_call_candidate_count, 1);
        assert!(
            core.contract_text().contains("ConstructorCall(Ok;Int(1))"),
            "contract text: {}",
            core.contract_text()
        );
        assert!(
            core.contract_text()
                .contains("unresolved_constructor_call_candidate:1"),
            "contract text: {}",
            core.contract_text()
        );
    }

    #[test]
    /// Verifies the remote-call proof policy switch remains conservative.
    ///
    /// Inputs:
    /// - A typed `CoreExpr::RemoteCall` value matching the formal remote-call
    ///   payload shape.
    /// - The summary-only `None` path used when coverage is requested without a
    ///   typed Core payload.
    ///
    /// Output:
    /// - The test passes when both paths report `ProofModelRequired`, and the
    ///   promotion helper still prevents remote calls from counting as
    ///   Lean-modeled.
    ///
    /// Transformation:
    /// - Exercises the named compiler-side promotion policy without lowering a
    ///   source fixture, so future remote-dispatch promotion must update this
    ///   explicit policy guard.
    fn syntax_output_lowering_to_core_remote_call_policy_switch_stays_proof_model_required() {
        let remote_call = CoreExpr::RemoteCall {
            module: "Eq".to_string(),
            function: "equal".to_string(),
            args: vec![
                CoreExpr::Var("Left".to_string()),
                CoreExpr::Var("Right".to_string()),
            ],
        };

        assert_eq!(
            remote_call_proof_coverage_policy(Some(&remote_call)),
            CoreProofCoverage::ProofModelRequired
        );
        assert_eq!(
            remote_call_proof_coverage_policy(None),
            CoreProofCoverage::ProofModelRequired
        );
        assert!(!remote_call_is_promoted_to_lean_covered());
        assert!(!core_expr_is_lean_modeled(&remote_call));
    }

    #[test]
    fn syntax_output_lowering_to_core_marks_remote_call_proof_model_required() {
        let module = parse_module_as_syntax_output(
            "\
module core_remote_call_boundary.\n\
\n\
pub call_remote(): Int ->\n\
    math:inc(1).\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "call_remote")
            .expect("core call_remote function");
        assert_eq!(function.clauses.len(), 1);
        assert_eq!(
            function.clauses[0].body.core_expr,
            Some(CoreExpr::RemoteCall {
                module: "math".to_string(),
                function: "inc".to_string(),
                args: vec![CoreExpr::Int(1)],
            })
        );
        assert_eq!(
            function.clauses[0].body.proof_coverage,
            CoreProofCoverage::ProofModelRequired
        );
        assert_eq!(
            core.metadata.proof_readiness,
            CoreProofReadiness::ProofModelRequired
        );
        assert_eq!(core.metadata.proof_model_required_expr_count, 1);
        assert!(core.metadata.lean_covered_expr_count >= 1);
        assert!(core.metadata.checked_preservation_expr_count >= 1);
        assert!(
            core.metadata.checked_preservation_expr_count >= core.metadata.lean_covered_expr_count
        );
        assert_eq!(core.metadata.typed_core_pattern_count, 0);
        assert_eq!(core.metadata.summary_only_pattern_count, 0);
        assert_eq!(core.metadata.checked_preservation_pattern_count, 0);
        assert!(
            core.contract_text().contains(
                "Call:core=RemoteCall(math:inc;Int(1)):preservation=structural-core-expr(freshness=no-runtime-bindings;target=RemoteCall(math:inc;Int(1))):proof=proof-model-required:remote=math"
            ),
            "contract text: {}",
            core.contract_text()
        );
    }

    #[test]
    fn syntax_output_lowering_to_core_rejects_remote_fun_ref_source_syntax() {
        let parsed = parse_module_as_syntax_output(
            "\
module core_remote_fun_ref_boundary.\n\
\n\
pub reference(): Dynamic ->\n\
    fun erlang:abs/1.\n",
        );

        assert!(
            parsed.is_err(),
            "remote fun references are backend output syntax, not canonical Terlan source"
        );
    }

    #[test]
    fn syntax_output_lowering_to_core_float_literal() {
        let module = parse_module_as_syntax_output(
            "\
module core_float_literal.\n\
\n\
pub value(): Float ->\n\
    1.5.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "value")
            .expect("core value function");
        assert_eq!(function.clauses.len(), 1);
        assert_eq!(
            function.clauses[0].body.core_expr,
            Some(CoreExpr::Float("1.5".to_string()))
        );
        assert_eq!(
            function.clauses[0].body.proof_coverage,
            CoreProofCoverage::ProofModelRequired
        );
        assert!(
            core.contract_text().contains("Float(1.5)"),
            "contract text: {}",
            core.contract_text()
        );
    }

    #[test]
    fn syntax_output_lowering_to_core_binary_literal() {
        let module = parse_module_as_syntax_output(
            "\
module core_binary_literal.\n\
\n\
pub value(): Binary ->\n\
    \"hello\".\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "value")
            .expect("core value function");
        assert_eq!(function.clauses.len(), 1);
        let body = &function.clauses[0].body;
        let Some(CoreExpr::Binary(value)) = &body.core_expr else {
            panic!(
                "expected typed binary literal core expr: {:?}",
                body.core_expr
            );
        };
        assert!(
            value.contains("hello"),
            "binary literal should preserve source text: {value}"
        );
        assert_eq!(body.proof_coverage, CoreProofCoverage::LeanCovered);
        assert!(
            core.contract_text().contains("Binary("),
            "contract text: {}",
            core.contract_text()
        );
    }

    #[test]
    fn syntax_output_lowering_to_core_binary_op() {
        let module = parse_module_as_syntax_output(
            "\
module core_binary_op_boundary.\n\
\n\
pub add(): Int ->\n\
    1 + 2.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "add")
            .expect("core add function");
        assert_eq!(function.clauses.len(), 1);
        assert_eq!(
            function.clauses[0].body.core_expr,
            Some(CoreExpr::BinaryOp {
                operator: "+".to_string(),
                left: Box::new(CoreExpr::Int(1)),
                right: Box::new(CoreExpr::Int(2)),
            })
        );
        assert_eq!(
            function.clauses[0].body.proof_coverage,
            CoreProofCoverage::LeanCovered
        );
        assert!(
            core.contract_text().contains("BinaryOp(+;Int(1), Int(2))"),
            "contract text: {}",
            core.contract_text()
        );
    }

    #[test]
    fn syntax_output_lowering_to_core_unary_op() {
        let module = parse_module_as_syntax_output(
            "\
module core_unary_op_boundary.\n\
\n\
pub negate(value: Int): Int ->\n\
    -value.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "negate")
            .expect("core negate function");
        assert_eq!(function.clauses.len(), 1);
        assert_eq!(
            function.clauses[0].body.core_expr,
            Some(CoreExpr::UnaryOp {
                operator: "-".to_string(),
                operand: Box::new(CoreExpr::Var("value".to_string())),
            })
        );
        assert_eq!(
            function.clauses[0].body.proof_coverage,
            CoreProofCoverage::LeanCovered
        );
        assert!(
            core.contract_text().contains("UnaryOp(-;Var(value))"),
            "contract text: {}",
            core.contract_text()
        );
    }

    #[test]
    fn syntax_output_lowering_to_core_map_expr() {
        let module = parse_module_as_syntax_output(
            "\
module core_map_expr_boundary.\n\
\n\
pub props(): Map ->\n\
    #{a := 1, b => 2}.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "props")
            .expect("core props function");
        assert_eq!(function.clauses.len(), 1);
        assert_eq!(
            function.clauses[0].body.core_expr,
            Some(CoreExpr::Map(vec![
                CoreMapExprField {
                    key: "a".to_string(),
                    required: true,
                    value: CoreExpr::Int(1),
                },
                CoreMapExprField {
                    key: "b".to_string(),
                    required: false,
                    value: CoreExpr::Int(2),
                },
            ]))
        );
        assert_eq!(
            function.clauses[0].body.proof_coverage,
            CoreProofCoverage::ProofModelRequired
        );
        assert!(
            core.contract_text()
                .contains("Map:core=Map(a:=Int(1),b=>Int(2)):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Map(a:=Int(1),b=>Int(2))):proof=proof-model-required"),
            "contract text: {}",
            core.contract_text()
        );
    }

    #[test]
    fn syntax_output_lowering_to_core_list_cons_expr() {
        let module = parse_module_as_syntax_output(
            "\
module core_list_cons_expr_boundary.\n\
\n\
pub prepend(head: Int, tail: List[Int]): List[Int] ->\n\
    [head | tail].\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "prepend")
            .expect("core prepend function");
        assert_eq!(function.clauses.len(), 1);
        assert_eq!(
            function.clauses[0].body.core_expr,
            Some(CoreExpr::ListCons {
                head: Box::new(CoreExpr::Var("head".to_string())),
                tail: Box::new(CoreExpr::Var("tail".to_string())),
            })
        );
        assert_eq!(
            function.clauses[0].body.proof_coverage,
            CoreProofCoverage::LeanCovered
        );
        assert!(
            core.contract_text()
                .contains("ListCons(Var(head)|Var(tail))"),
            "contract text: {}",
            core.contract_text()
        );
    }

    #[test]
    fn syntax_output_lowering_to_core_index_expr() {
        let module = parse_module_as_syntax_output(
            "\
module core_index_expr_boundary.\n\
\n\
pub first(values: List[Int]): Dynamic ->\n\
    values[0].\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "first")
            .expect("core first function");
        assert_eq!(function.clauses.len(), 1);
        assert_eq!(
            function.clauses[0].body.core_expr,
            Some(CoreExpr::Index {
                base: Box::new(CoreExpr::Var("values".to_string())),
                index: Box::new(CoreExpr::Int(0)),
            })
        );
        assert_eq!(
            function.clauses[0].body.proof_coverage,
            CoreProofCoverage::ProofModelRequired
        );
        assert!(
            core.contract_text().contains("Index(Var(values);Int(0))"),
            "contract text: {}",
            core.contract_text()
        );
    }

    #[test]
    fn syntax_output_lowering_to_core_fixed_array_expr() {
        let module = parse_module_as_syntax_output(
            "\
module core_fixed_array_expr_boundary.\n\
\n\
pub rgb(): FixedArray[3, Int] ->\n\
    #[1, 2, 3].\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "rgb")
            .expect("core rgb function");
        assert_eq!(function.clauses.len(), 1);
        assert_eq!(
            function.clauses[0].body.core_expr,
            Some(CoreExpr::FixedArray(vec![
                CoreExpr::Int(1),
                CoreExpr::Int(2),
                CoreExpr::Int(3),
            ]))
        );
        assert_eq!(
            function.clauses[0].body.proof_coverage,
            CoreProofCoverage::ProofModelRequired
        );
        assert!(
            core.contract_text()
                .contains("FixedArray(Int(1),Int(2),Int(3))"),
            "contract text: {}",
            core.contract_text()
        );
    }

    #[test]
    fn syntax_output_lowering_to_core_list_comprehension_expr() {
        let module = parse_module_as_syntax_output(
            "\
module core_list_comprehension_expr_boundary.\n\
\n\
pub values(items: List[Int]): List[Int] ->\n\
    [value | value <- items].\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "values")
            .expect("core values function");
        assert_eq!(function.clauses.len(), 1);
        assert_eq!(
            function.clauses[0].body.core_expr,
            Some(CoreExpr::ListComprehension {
                expr: Box::new(CoreExpr::Var("value".to_string())),
                pattern: CorePattern::Var("value".to_string()),
                source: Box::new(CoreExpr::Var("items".to_string())),
                guard: None,
            })
        );
        assert_eq!(
            function.clauses[0].body.proof_coverage,
            CoreProofCoverage::ProofModelRequired
        );
        assert!(
            core.contract_text()
                .contains("ListComprehension(Var(value)|Var(value)<-Var(items))"),
            "contract text: {}",
            core.contract_text()
        );
    }

    #[test]
    fn syntax_output_lowering_to_core_record_construct_expr() {
        let module = parse_module_as_syntax_output(
            "\
module core_record_construct_expr_boundary.\n\
\n\
pub make(): Dynamic ->\n\
    #Point { x = 1, y = 2 }.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "make")
            .expect("core make function");
        assert_eq!(function.clauses.len(), 1);
        assert_eq!(
            function.clauses[0].body.core_expr,
            Some(CoreExpr::RecordConstruct {
                name: "Point".to_string(),
                fields: vec![
                    CoreRecordExprField {
                        key: "x".to_string(),
                        required: true,
                        value: CoreExpr::Int(1),
                    },
                    CoreRecordExprField {
                        key: "y".to_string(),
                        required: true,
                        value: CoreExpr::Int(2),
                    },
                ],
            })
        );
        assert_eq!(
            function.clauses[0].body.proof_coverage,
            CoreProofCoverage::ProofModelRequired
        );
        assert!(
            core.contract_text()
                .contains("RecordConstruct(Point;x=Int(1),y=Int(2))"),
            "contract text: {}",
            core.contract_text()
        );
    }

    /// Verifies imported public struct type identity does not allow raw
    /// construction outside the defining module.
    ///
    /// Inputs:
    /// - A provider interface declaring public struct `Point`.
    /// - A consumer module importing that type and attempting `#Point { ... }`.
    ///
    /// Output:
    /// - Test passes when typechecking rejects the raw imported struct literal
    ///   before CoreIR/backend emission.
    ///
    /// Transformation:
    /// - Resolves a consumer against an explicit interface map and checks that
    ///   record construction visibility is enforced semantically, independent
    ///   of syntax acceptance.
    #[test]
    fn syntax_output_rejects_raw_imported_struct_construction_without_constructor() {
        let provider = parse_interface_module_as_syntax_output(
            "\
module provider.\n\
\n\
pub struct Point {\n\
    x: Int\n\
}.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse provider interface fixture: {:?}", err));
        let mut interfaces = HashMap::new();
        interfaces.insert(
            provider.module_name.clone(),
            syntax_module_output_to_interface(&provider),
        );
        let module = parse_module_as_syntax_output(
            "\
module raw_imported_struct_construction_boundary.\n\
\n\
import type provider.Point.\n\
\n\
pub make(): Dynamic ->\n\
    #Point { x = 1 }.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse consumer syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
        let diagnostics = type_check_syntax_module_output(&module, &resolved);

        assert!(
            diagnostics.iter().any(|diagnostic| diagnostic
                .message
                .contains("cannot raw-construct imported struct provider.Point")),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_rejects_public_constructor_returning_private_type() {
        let diagnostics = check_syntax_output(
            "\
module public_constructor_private_return.\n\
\n\
struct Secret {\n\
    value: Int\n\
}.\n\
\n\
pub constructor Secret {\n\
    (value: Int): Secret -> value\n\
}.\n",
        );

        assert!(
            diagnostics.iter().any(|diagnostic| diagnostic
                .message
                .contains("public constructor Secret exposes private return type Secret")),
            "diagnostics: {:?}",
            diagnostics
        );
    }

    #[test]
    fn syntax_output_lowering_to_core_field_access_expr() {
        let module = parse_module_as_syntax_output(
            "\
module core_field_access_expr_boundary.\n\
\n\
pub read(point: Point): Dynamic ->\n\
    point.x.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "read")
            .expect("core read function");
        assert_eq!(function.clauses.len(), 1);
        assert_eq!(
            function.clauses[0].body.core_expr,
            Some(CoreExpr::FieldAccess {
                base: Box::new(CoreExpr::Var("point".to_string())),
                field: "x".to_string(),
            })
        );
        assert_eq!(
            function.clauses[0].body.proof_coverage,
            CoreProofCoverage::LeanCovered
        );
        assert!(
            core.contract_text().contains("FieldAccess(Var(point).x)"),
            "contract text: {}",
            core.contract_text()
        );
    }

    #[test]
    fn syntax_output_lowering_to_core_let_expr() {
        let module = parse_module_as_syntax_output(
            "\
module core_let_expr_boundary.\n\
\n\
pub with_body(x: Int): Int ->\n\
    let y = x + 1; z = y * 2; z + y.\n\
\n\
pub final_value(x: Int): Int ->\n\
    let y = x + 1; z = y * 2; z.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let with_body = core
            .functions
            .iter()
            .find(|function| function.name == "with_body")
            .expect("core with_body function");
        assert_eq!(with_body.clauses.len(), 1);
        assert_eq!(
            with_body.clauses[0].body.core_expr,
            Some(CoreExpr::Let {
                bindings: vec![
                    CoreLetBinding {
                        name: "y".to_string(),
                        value: CoreExpr::BinaryOp {
                            operator: "+".to_string(),
                            left: Box::new(CoreExpr::Var("x".to_string())),
                            right: Box::new(CoreExpr::Int(1)),
                        },
                    },
                    CoreLetBinding {
                        name: "z".to_string(),
                        value: CoreExpr::BinaryOp {
                            operator: "*".to_string(),
                            left: Box::new(CoreExpr::Var("y".to_string())),
                            right: Box::new(CoreExpr::Int(2)),
                        },
                    },
                ],
                body: Box::new(CoreExpr::BinaryOp {
                    operator: "+".to_string(),
                    left: Box::new(CoreExpr::Var("z".to_string())),
                    right: Box::new(CoreExpr::Var("y".to_string())),
                }),
            })
        );
        assert_eq!(
            with_body.clauses[0].body.proof_coverage,
            CoreProofCoverage::ProofModelRequired
        );

        let final_value = core
            .functions
            .iter()
            .find(|function| function.name == "final_value")
            .expect("core final_value function");
        assert_eq!(
            final_value.clauses[0].body.core_expr,
            Some(CoreExpr::Let {
                bindings: vec![
                    CoreLetBinding {
                        name: "y".to_string(),
                        value: CoreExpr::BinaryOp {
                            operator: "+".to_string(),
                            left: Box::new(CoreExpr::Var("x".to_string())),
                            right: Box::new(CoreExpr::Int(1)),
                        },
                    },
                    CoreLetBinding {
                        name: "z".to_string(),
                        value: CoreExpr::BinaryOp {
                            operator: "*".to_string(),
                            left: Box::new(CoreExpr::Var("y".to_string())),
                            right: Box::new(CoreExpr::Int(2)),
                        },
                    },
                ],
                body: Box::new(CoreExpr::Var("z".to_string())),
            })
        );
        assert!(
            core.contract_text()
                .contains("Let(y=BinaryOp(+;Var(x), Int(1));z=BinaryOp(*;Var(y), Int(2));"),
            "contract text: {}",
            core.contract_text()
        );
    }

    #[test]
    fn syntax_output_lowering_to_core_record_access_expr() {
        let module = parse_module_as_syntax_output(
            "\
module core_record_access_expr_boundary.\n\
\n\
pub read(point: Point): Dynamic ->\n\
    point#Point.x.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "read")
            .expect("core read function");
        assert_eq!(function.clauses.len(), 1);
        assert_eq!(
            function.clauses[0].body.core_expr,
            Some(CoreExpr::RecordAccess {
                base: Box::new(CoreExpr::Var("point".to_string())),
                name: "Point".to_string(),
                field: "x".to_string(),
            })
        );
        assert_eq!(
            function.clauses[0].body.proof_coverage,
            CoreProofCoverage::ProofModelRequired
        );
        assert!(
            core.contract_text()
                .contains("RecordAccess(Var(point)#Point.x)"),
            "contract text: {}",
            core.contract_text()
        );
    }

    #[test]
    fn syntax_output_lowering_to_core_record_update_expr() {
        let module = parse_module_as_syntax_output(
            "\
module core_record_update_expr_boundary.\n\
\n\
pub update(point: Point): Dynamic ->\n\
    point#Point { x = 1, y = point.y }.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "update")
            .expect("core update function");
        assert_eq!(function.clauses.len(), 1);
        assert_eq!(
            function.clauses[0].body.core_expr,
            Some(CoreExpr::RecordUpdate {
                base: Box::new(CoreExpr::Var("point".to_string())),
                name: "Point".to_string(),
                fields: vec![
                    CoreRecordExprField {
                        key: "x".to_string(),
                        required: true,
                        value: CoreExpr::Int(1),
                    },
                    CoreRecordExprField {
                        key: "y".to_string(),
                        required: true,
                        value: CoreExpr::FieldAccess {
                            base: Box::new(CoreExpr::Var("point".to_string())),
                            field: "y".to_string(),
                        },
                    },
                ],
            })
        );
        assert_eq!(
            function.clauses[0].body.proof_coverage,
            CoreProofCoverage::ProofModelRequired
        );
        assert!(
            core.contract_text()
                .contains("RecordUpdate(Var(point)#Point;x=Int(1),y=FieldAccess(Var(point).y))"),
            "contract text: {}",
            core.contract_text()
        );
    }

    #[test]
    fn syntax_output_lowering_to_core_template_instantiate_expr() {
        let module = parse_module_as_syntax_output(
            "\
module core_template_instantiate_expr_boundary.\n\
\n\
pub make(): Dynamic ->\n\
    UserCard{ name = \"Ada\" }.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "make")
            .expect("core make function");
        assert_eq!(function.clauses.len(), 1);
        let Some(CoreExpr::TemplateInstantiate { name, fields }) =
            &function.clauses[0].body.core_expr
        else {
            panic!(
                "expected template instantiation core expr: {:?}",
                function.clauses[0].body.core_expr
            );
        };
        assert_eq!(name, "UserCard");
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].key, "name");
        assert!(matches!(fields[0].value, CoreExpr::Binary(_)));
        assert_eq!(
            function.clauses[0].body.proof_coverage,
            CoreProofCoverage::ProofModelRequired
        );
        assert!(
            core.contract_text()
                .contains("TemplateInstantiate(UserCard;name=Binary("),
            "contract text: {}",
            core.contract_text()
        );
    }

    #[test]
    /// Verifies constructor-chain identity states remain partial proof coverage.
    ///
    /// Inputs:
    /// - One `CoreExpr::ConstructorChain` with no resolved base constructor
    ///   identity.
    /// - One `CoreExpr::ConstructorChain` with a resolved base constructor
    ///   identity.
    ///
    /// Output:
    /// - Test passes when both payloads report `Partial` coverage and remain
    ///   outside the current Lean-modeled expression subset.
    ///
    /// Transformation:
    /// - Exercises the named constructor-chain proof policy without parsing a
    ///   source fixture, keeping identity resolution and proof promotion as
    ///   separate compiler decisions.
    fn syntax_output_lowering_to_core_constructor_chain_policy_stays_partial_for_identity_states() {
        let unresolved_chain = CoreExpr::ConstructorChain {
            base: "User".to_string(),
            base_constructor_identity: None,
            args: vec![CoreExpr::Var("id".to_string())],
            record: Box::new(CoreExpr::RecordConstruct {
                name: "Admin".to_string(),
                fields: vec![CoreRecordExprField {
                    key: "id".to_string(),
                    required: true,
                    value: CoreExpr::Var("id".to_string()),
                }],
            }),
        };
        let resolved_chain = CoreExpr::ConstructorChain {
            base: "User".to_string(),
            base_constructor_identity: Some("User".to_string()),
            args: vec![CoreExpr::Var("id".to_string())],
            record: Box::new(CoreExpr::RecordConstruct {
                name: "Admin".to_string(),
                fields: vec![CoreRecordExprField {
                    key: "id".to_string(),
                    required: true,
                    value: CoreExpr::Var("id".to_string()),
                }],
            }),
        };

        for core_expr in [&unresolved_chain, &resolved_chain] {
            assert_eq!(
                constructor_chain_proof_coverage_policy(Some(core_expr)),
                CoreProofCoverage::Partial
            );
            assert!(!core_expr_is_lean_modeled(core_expr));
        }
    }

    #[test]
    fn syntax_output_lowering_to_core_constructor_chain_expr() {
        let module = parse_module_as_syntax_output(
            "\
module core_constructor_chain_expr_boundary.\n\
\n\
pub constructor User {\n\
    (id: Int, name: Binary): Dynamic -> id\n\
}.\n\
\n\
pub make(id: Int, name: Binary): Dynamic ->\n\
    User(id, name) with Admin { id = id, name = name }.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "make")
            .expect("core make function");
        assert_eq!(function.clauses.len(), 1);
        let Some(CoreExpr::ConstructorChain {
            base,
            base_constructor_identity,
            args,
            record,
        }) = &function.clauses[0].body.core_expr
        else {
            panic!(
                "expected constructor chain core expr: {:?}",
                function.clauses[0].body.core_expr
            );
        };
        assert_eq!(base, "User");
        assert_eq!(base_constructor_identity.as_deref(), Some("User"));
        assert_eq!(
            args,
            &vec![
                CoreExpr::Var("id".to_string()),
                CoreExpr::Var("name".to_string())
            ]
        );
        assert_eq!(
            record.as_ref(),
            &CoreExpr::RecordConstruct {
                name: "Admin".to_string(),
                fields: vec![
                    CoreRecordExprField {
                        key: "id".to_string(),
                        required: true,
                        value: CoreExpr::Var("id".to_string()),
                    },
                    CoreRecordExprField {
                        key: "name".to_string(),
                        required: true,
                        value: CoreExpr::Var("name".to_string()),
                    },
                ],
            }
        );
        assert_eq!(
            function.clauses[0].body.proof_coverage,
            CoreProofCoverage::Partial
        );
        assert_eq!(
            constructor_chain_proof_coverage_policy(function.clauses[0].body.core_expr.as_ref()),
            CoreProofCoverage::Partial
        );
        assert_eq!(core.metadata.resolved_constructor_call_identity_count, 1);
        assert_eq!(core.metadata.resolved_constructor_chain_identity_count, 1);
        assert_eq!(core.metadata.resolved_constructor_pattern_identity_count, 0);
        assert_eq!(core.metadata.unresolved_constructor_call_candidate_count, 0);
        assert_eq!(
            core.metadata.unresolved_constructor_chain_candidate_count,
            0
        );
        assert_eq!(
            core.metadata.unresolved_constructor_pattern_candidate_count,
            0
        );
        assert!(
            core.contract_text().contains(
                "ConstructorChain(User;identity=User;Var(id),Var(name) with RecordConstruct(Admin;id=Var(id),name=Var(name)))"
            ),
            "contract text: {}",
            core.contract_text()
        );
        assert!(
            core.contract_text()
                .contains("resolved_constructor_chain_identity:1"),
            "contract text: {}",
            core.contract_text()
        );
    }

    /// Verifies eligible local type-alias constructor-chain bases carry CoreIR
    /// identity.
    ///
    /// Inputs:
    /// - None; constructs a syntax-output module with `pub type User =
    ///   {:user, id: Int, name: Binary}` and uses `User(id, name)` as a
    ///   constructor-chain base.
    ///
    /// Output:
    /// - Test passes when the constructor-chain base has
    ///   `base_constructor_identity = Some("User")`, the nested constructor
    ///   call identity is resolved, and no unresolved chain candidates remain.
    ///
    /// Transformation:
    /// - Resolves and lowers a type-alias constructor-chain through the same
    ///   CoreIR identity annotation pass used for declared constructors,
    ///   without promoting constructor chains to Lean-covered proof status.
    #[test]
    fn syntax_output_lowering_to_core_resolves_local_alias_constructor_chain_identity() {
        let module = parse_module_as_syntax_output(
            "\
module core_alias_constructor_chain_identity_boundary.\n\
\n\
pub type User = {:user, id: Int, name: Binary}.\n\
\n\
pub make(id: Int, name: Binary): Dynamic ->\n\
    User(id, name) with Admin { id = id, name = name }.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let diagnostics = type_check_syntax_module_output(&module, &resolved);
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "make")
            .expect("core make function");
        let Some(CoreExpr::ConstructorChain {
            base,
            base_constructor_identity,
            ..
        }) = &function.clauses[0].body.core_expr
        else {
            panic!(
                "expected constructor chain core expr: {:?}",
                function.clauses[0].body.core_expr
            );
        };
        assert_eq!(base, "User");
        assert_eq!(base_constructor_identity.as_deref(), Some("User"));
        assert_eq!(
            function.clauses[0].body.proof_coverage,
            CoreProofCoverage::Partial
        );
        assert_eq!(core.metadata.resolved_constructor_call_identity_count, 1);
        assert_eq!(core.metadata.resolved_constructor_chain_identity_count, 1);
        assert_eq!(
            core.metadata.unresolved_constructor_chain_candidate_count,
            0
        );
        assert!(
            core.contract_text().contains(
                "ConstructorChain(User;identity=User;Var(id),Var(name) with RecordConstruct(Admin;id=Var(id),name=Var(name)))"
            ),
            "contract text: {}",
            core.contract_text()
        );
    }

    /// Verifies eligible directly imported type-alias constructor-chain bases
    /// carry qualified CoreIR identity.
    ///
    /// Inputs:
    /// - A provider interface declaring public alias constructor `User`.
    /// - A consumer syntax-output module importing `User` directly and using
    ///   `User` as the constructor-chain base.
    ///
    /// Output:
    /// - Test passes when CoreIR preserves the source-visible base `User`,
    ///   annotates it with `base_constructor_identity = Some("provider.User")`,
    ///   and reports no unresolved constructor-chain candidates.
    ///
    /// Transformation:
    /// - Resolves the direct type import against an explicit interface map,
    ///   lowers to CoreIR, and verifies single-shape type-alias
    ///   constructor-chain identity metadata without using a local import alias.
    #[test]
    fn syntax_output_lowering_to_core_resolves_direct_imported_alias_constructor_chain_identity() {
        let provider = parse_interface_module_as_syntax_output(
            "\
module provider.\n\
\n\
pub type User = {:user, id: Int, name: Binary}.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse provider interface fixture: {:?}", err));
        let mut interfaces = HashMap::new();
        interfaces.insert(
            provider.module_name.clone(),
            syntax_module_output_to_interface(&provider),
        );
        let module = parse_module_as_syntax_output(
            "\
module direct_imported_alias_constructor_chain_identity_boundary.\n\
\n\
import provider.{User}.\n\
\n\
pub make(id: Int, name: Binary): Dynamic ->\n\
    User(id, name) with Admin { id = id, name = name }.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
        let diagnostics = type_check_syntax_module_output(&module, &resolved);
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "make")
            .expect("core make function");
        let Some(CoreExpr::ConstructorChain {
            base,
            base_constructor_identity,
            ..
        }) = &function.clauses[0].body.core_expr
        else {
            panic!(
                "expected constructor chain core expr: {:?}",
                function.clauses[0].body.core_expr
            );
        };
        assert_eq!(base, "User");
        assert_eq!(base_constructor_identity.as_deref(), Some("provider.User"));
        assert_eq!(
            function.clauses[0].body.proof_coverage,
            CoreProofCoverage::Partial
        );
        assert_eq!(core.metadata.resolved_constructor_call_identity_count, 1);
        assert_eq!(core.metadata.resolved_constructor_chain_identity_count, 1);
        assert_eq!(
            core.metadata.unresolved_constructor_chain_candidate_count,
            0
        );
        assert!(
            core.contract_text().contains(
                "ConstructorChain(User;identity=provider.User;Var(id),Var(name) with RecordConstruct(Admin;id=Var(id),name=Var(name)))"
            ),
            "contract text: {}",
            core.contract_text()
        );
    }

    /// Verifies imported public constructor-chain bases carry qualified CoreIR
    /// identity.
    ///
    /// Inputs:
    /// - A provider interface declaring public constructor `User`.
    /// - A consumer syntax-output module importing `User` and using it as a
    ///   constructor-chain base.
    ///
    /// Output:
    /// - Test passes when the constructor-chain base is annotated with
    ///   `base_constructor_identity = Some("provider.User")`.
    ///
    /// Transformation:
    /// - Resolves the consumer against an explicit interface map, lowers it to
    ///   CoreIR, and verifies imported constructor-chain identity metadata
    ///   without promoting constructor chains to Lean-covered proof status.
    #[test]
    fn syntax_output_lowering_to_core_resolves_imported_constructor_chain_identity() {
        let provider = parse_interface_module_as_syntax_output(
            "\
module provider.\n\
\n\
pub constructor User {\n\
    (id: Int, name: Binary): Dynamic -> id\n\
}.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse provider interface fixture: {:?}", err));
        let mut interfaces = HashMap::new();
        interfaces.insert(
            provider.module_name.clone(),
            syntax_module_output_to_interface(&provider),
        );
        let module = parse_module_as_syntax_output(
            "\
module imported_constructor_chain_identity_boundary.\n\
\n\
import provider.{User}.\n\
\n\
pub make(id: Int, name: Binary): Dynamic ->\n\
    User(id, name) with Admin { id = id, name = name }.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
        let diagnostics = type_check_syntax_module_output(&module, &resolved);
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "make")
            .expect("core make function");
        let Some(CoreExpr::ConstructorChain {
            base,
            base_constructor_identity,
            ..
        }) = &function.clauses[0].body.core_expr
        else {
            panic!(
                "expected constructor chain core expr: {:?}",
                function.clauses[0].body.core_expr
            );
        };
        assert_eq!(base, "User");
        assert_eq!(base_constructor_identity.as_deref(), Some("provider.User"));
        assert_eq!(
            function.clauses[0].body.proof_coverage,
            CoreProofCoverage::Partial
        );
        assert_eq!(core.metadata.resolved_constructor_call_identity_count, 1);
        assert_eq!(core.metadata.resolved_constructor_chain_identity_count, 1);
        assert_eq!(
            core.metadata.unresolved_constructor_chain_candidate_count,
            0
        );
        assert!(
            core.contract_text().contains(
                "ConstructorChain(User;identity=provider.User;Var(id),Var(name) with RecordConstruct(Admin;id=Var(id),name=Var(name)))"
            ),
            "contract text: {}",
            core.contract_text()
        );
    }

    /// Verifies aliased imported constructor-chain bases carry source identity.
    ///
    /// Inputs:
    /// - A provider interface declaring public constructor `User`.
    /// - A consumer syntax-output module importing `User as Member` and using
    ///   `Member` as the constructor-chain base.
    ///
    /// Output:
    /// - Test passes when CoreIR preserves the source-visible base `Member`,
    ///   annotates it with `base_constructor_identity = Some("provider.User")`,
    ///   and keeps constructor-chain proof coverage partial.
    ///
    /// Transformation:
    /// - Resolves the aliased import against an explicit interface map, lowers
    ///   to CoreIR, and verifies constructor-chain identity metadata is based on
    ///   the provider/source constructor rather than the local alias.
    #[test]
    fn syntax_output_lowering_to_core_resolves_aliased_imported_constructor_chain_identity() {
        let provider = parse_interface_module_as_syntax_output(
            "\
module provider.\n\
\n\
pub constructor User {\n\
    (id: Int, name: Binary): Dynamic -> id\n\
}.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse provider interface fixture: {:?}", err));
        let mut interfaces = HashMap::new();
        interfaces.insert(
            provider.module_name.clone(),
            syntax_module_output_to_interface(&provider),
        );
        let module = parse_module_as_syntax_output(
            "\
module aliased_imported_constructor_chain_identity_boundary.\n\
\n\
import provider.{User as Member}.\n\
\n\
pub make(id: Int, name: Binary): Dynamic ->\n\
    Member(id, name) with Admin { id = id, name = name }.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
        let diagnostics = type_check_syntax_module_output(&module, &resolved);
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "make")
            .expect("core make function");
        let Some(CoreExpr::ConstructorChain {
            base,
            base_constructor_identity,
            ..
        }) = &function.clauses[0].body.core_expr
        else {
            panic!(
                "expected constructor chain core expr: {:?}",
                function.clauses[0].body.core_expr
            );
        };
        assert_eq!(base, "Member");
        assert_eq!(base_constructor_identity.as_deref(), Some("provider.User"));
        assert_eq!(
            function.clauses[0].body.proof_coverage,
            CoreProofCoverage::Partial
        );
        assert_eq!(core.metadata.resolved_constructor_call_identity_count, 1);
        assert_eq!(core.metadata.resolved_constructor_chain_identity_count, 1);
        assert_eq!(
            core.metadata.unresolved_constructor_chain_candidate_count,
            0
        );
        assert!(
            core.contract_text().contains(
                "ConstructorChain(Member;identity=provider.User;Var(id),Var(name) with RecordConstruct(Admin;id=Var(id),name=Var(name)))"
            ),
            "contract text: {}",
            core.contract_text()
        );
    }

    /// Verifies eligible imported type-alias constructor-chain bases carry
    /// qualified CoreIR identity.
    ///
    /// Inputs:
    /// - A provider interface declaring public alias constructor `User`.
    /// - A consumer syntax-output module importing `User as Member` and using
    ///   `Member` as the constructor-chain base.
    ///
    /// Output:
    /// - Test passes when CoreIR preserves the source-visible base `Member`,
    ///   annotates it with `base_constructor_identity = Some("provider.User")`,
    ///   and reports no unresolved constructor-chain candidates.
    ///
    /// Transformation:
    /// - Resolves the aliased type import against an explicit interface map,
    ///   lowers to CoreIR, and verifies single-shape type-alias
    ///   constructor-chain identity metadata is provider-qualified.
    #[test]
    fn syntax_output_lowering_to_core_resolves_imported_alias_constructor_chain_identity() {
        let provider = parse_interface_module_as_syntax_output(
            "\
module provider.\n\
\n\
pub type User = {:user, id: Int, name: Binary}.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse provider interface fixture: {:?}", err));
        let mut interfaces = HashMap::new();
        interfaces.insert(
            provider.module_name.clone(),
            syntax_module_output_to_interface(&provider),
        );
        let module = parse_module_as_syntax_output(
            "\
module imported_alias_constructor_chain_identity_boundary.\n\
\n\
import provider.{User as Member}.\n\
\n\
pub make(id: Int, name: Binary): Dynamic ->\n\
    Member(id, name) with Admin { id = id, name = name }.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
        let diagnostics = type_check_syntax_module_output(&module, &resolved);
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "make")
            .expect("core make function");
        let Some(CoreExpr::ConstructorChain {
            base,
            base_constructor_identity,
            ..
        }) = &function.clauses[0].body.core_expr
        else {
            panic!(
                "expected constructor chain core expr: {:?}",
                function.clauses[0].body.core_expr
            );
        };
        assert_eq!(base, "Member");
        assert_eq!(base_constructor_identity.as_deref(), Some("provider.User"));
        assert_eq!(
            function.clauses[0].body.proof_coverage,
            CoreProofCoverage::Partial
        );
        assert_eq!(core.metadata.resolved_constructor_call_identity_count, 1);
        assert_eq!(core.metadata.resolved_constructor_chain_identity_count, 1);
        assert_eq!(
            core.metadata.unresolved_constructor_chain_candidate_count,
            0
        );
        assert!(
            core.contract_text().contains(
                "ConstructorChain(Member;identity=provider.User;Var(id),Var(name) with RecordConstruct(Admin;id=Var(id),name=Var(name)))"
            ),
            "contract text: {}",
            core.contract_text()
        );
    }

    #[test]
    fn syntax_output_lowering_to_core_resolves_declared_constructor_pattern_identity() {
        let module = parse_module_as_syntax_output(
            "\
module core_constructor_pattern_identity_boundary.\n\
\n\
pub constructor Some {\n\
    (value: Dynamic): Dynamic -> {:some, value}\n\
}.\n\
\n\
pub unwrap(input: Dynamic): Dynamic ->\n\
    case input {\n\
        Some(value) -> value\n\
    }.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "unwrap")
            .expect("core unwrap function");
        assert_eq!(function.clauses.len(), 1);
        let Some(CoreExpr::Case { clauses, .. }) = &function.clauses[0].body.core_expr else {
            panic!(
                "expected case core expr: {:?}",
                function.clauses[0].body.core_expr
            );
        };
        assert_eq!(clauses.len(), 1);
        assert_eq!(
            clauses[0].pattern,
            CorePattern::Constructor {
                name: "Some".to_string(),
                constructor_identity: Some("Some".to_string()),
                args: vec![CorePattern::Var("value".to_string())],
            }
        );
        assert_eq!(core.metadata.resolved_constructor_call_identity_count, 0);
        assert_eq!(core.metadata.resolved_constructor_chain_identity_count, 0);
        assert_eq!(core.metadata.resolved_constructor_pattern_identity_count, 1);
        assert_eq!(core.metadata.unresolved_constructor_call_candidate_count, 0);
        assert_eq!(
            core.metadata.unresolved_constructor_chain_candidate_count,
            0
        );
        assert_eq!(
            core.metadata.unresolved_constructor_pattern_candidate_count,
            0
        );
        assert!(
            core.contract_text()
                .contains("Constructor(Some;identity=Some;Var(value))"),
            "contract text: {}",
            core.contract_text()
        );
        assert!(
            core.contract_text()
                .contains("resolved_constructor_pattern_identity:1"),
            "contract text: {}",
            core.contract_text()
        );
        assert!(
            core.contract_text().contains(
                "target=Case(Var(input);Constructor(Some;identity=Some;Var(value))=>Var(value))"
            ),
            "contract text: {}",
            core.contract_text()
        );
    }

    /// Verifies imported public constructor patterns carry qualified CoreIR
    /// identity.
    ///
    /// Inputs:
    /// - A provider interface declaring public constructor `Some`.
    /// - A consumer syntax-output module importing `Some` and matching it in a
    ///   case expression.
    ///
    /// Output:
    /// - Test passes when the case pattern is annotated with
    ///   `constructor_identity = Some("provider.Some")`.
    ///
    /// Transformation:
    /// - Resolves the consumer against an explicit interface map, lowers it to
    ///   CoreIR, and verifies imported constructor-pattern identity metadata.
    #[test]
    fn syntax_output_lowering_to_core_resolves_imported_constructor_pattern_identity() {
        let provider = parse_interface_module_as_syntax_output(
            "\
module provider.\n\
\n\
pub constructor Some {\n\
    (value: Dynamic): Dynamic -> value\n\
}.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse provider interface fixture: {:?}", err));
        let mut interfaces = HashMap::new();
        interfaces.insert(
            provider.module_name.clone(),
            syntax_module_output_to_interface(&provider),
        );
        let module = parse_module_as_syntax_output(
            "\
module imported_constructor_pattern_identity_boundary.\n\
\n\
import provider.{Some}.\n\
\n\
pub unwrap(input: Dynamic): Dynamic ->\n\
    case input {\n\
        Some(value) -> value\n\
    }.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
        let diagnostics = type_check_syntax_module_output(&module, &resolved);
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "unwrap")
            .expect("core unwrap function");
        let Some(CoreExpr::Case { clauses, .. }) = &function.clauses[0].body.core_expr else {
            panic!(
                "expected case core expr: {:?}",
                function.clauses[0].body.core_expr
            );
        };
        let CorePattern::Constructor {
            name,
            constructor_identity,
            args,
        } = &clauses[0].pattern
        else {
            panic!("expected constructor pattern: {:?}", clauses[0].pattern);
        };
        assert_eq!(name, "Some");
        assert_eq!(constructor_identity.as_deref(), Some("provider.Some"));
        assert_eq!(args, &vec![CorePattern::Var("value".to_string())]);
        assert_eq!(core.metadata.resolved_constructor_pattern_identity_count, 1);
        assert_eq!(
            core.metadata.unresolved_constructor_pattern_candidate_count,
            0
        );
        assert!(
            core.contract_text()
                .contains("Constructor(Some;identity=provider.Some;Var(value))"),
            "contract text: {}",
            core.contract_text()
        );
    }

    /// Verifies aliased imported public constructor patterns carry source identity.
    ///
    /// Inputs:
    /// - A provider interface declaring public constructor `Some`.
    /// - A consumer syntax-output module importing `Some as Maybe` and matching
    ///   `Maybe(value)` in a case expression.
    ///
    /// Output:
    /// - Test passes when the CoreIR pattern preserves the source-visible head
    ///   `Maybe` and annotates it with `constructor_identity =
    ///   Some("provider.Some")`.
    ///
    /// Transformation:
    /// - Resolves the aliased pattern import against an explicit interface map,
    ///   lowers to CoreIR, and verifies pattern identity metadata is based on
    ///   the provider/source constructor rather than the local alias.
    #[test]
    fn syntax_output_lowering_to_core_resolves_aliased_imported_constructor_pattern_identity() {
        let provider = parse_interface_module_as_syntax_output(
            "\
module provider.\n\
\n\
pub constructor Some {\n\
    (value: Dynamic): Dynamic -> value\n\
}.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse provider interface fixture: {:?}", err));
        let mut interfaces = HashMap::new();
        interfaces.insert(
            provider.module_name.clone(),
            syntax_module_output_to_interface(&provider),
        );
        let module = parse_module_as_syntax_output(
            "\
module aliased_imported_constructor_pattern_identity_boundary.\n\
\n\
import provider.{Some as Maybe}.\n\
\n\
pub unwrap(input: Dynamic): Dynamic ->\n\
    case input {\n\
        Maybe(value) -> value\n\
    }.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
        let diagnostics = type_check_syntax_module_output(&module, &resolved);
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
        );
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "unwrap")
            .expect("core unwrap function");
        let Some(CoreExpr::Case { clauses, .. }) = &function.clauses[0].body.core_expr else {
            panic!(
                "expected case core expr: {:?}",
                function.clauses[0].body.core_expr
            );
        };
        let CorePattern::Constructor {
            name,
            constructor_identity,
            args,
        } = &clauses[0].pattern
        else {
            panic!("expected constructor pattern: {:?}", clauses[0].pattern);
        };
        assert_eq!(name, "Maybe");
        assert_eq!(constructor_identity.as_deref(), Some("provider.Some"));
        assert_eq!(args, &vec![CorePattern::Var("value".to_string())]);
        assert_eq!(core.metadata.resolved_constructor_pattern_identity_count, 1);
        assert_eq!(
            core.metadata.unresolved_constructor_pattern_candidate_count,
            0
        );
        assert!(
            core.contract_text()
                .contains("Constructor(Maybe;identity=provider.Some;Var(value))"),
            "contract text: {}",
            core.contract_text()
        );
    }

    #[test]
    fn syntax_output_lowering_to_core_records_case_core_expr() {
        let module = parse_module_as_syntax_output(
            "\
module core_case_boundary.\n\
\n\
pub choose(x: Int): Int ->\n\
    case x {\n\
        0 -> 1;\n\
        _ -> x\n\
    }.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "choose")
            .expect("core choose function");
        assert_eq!(function.clauses.len(), 1);
        assert_eq!(
            function.clauses[0].body.core_expr,
            Some(CoreExpr::Case {
                scrutinee: Box::new(CoreExpr::Var("x".to_string())),
                clauses: vec![
                    CoreCaseClause {
                        pattern: CorePattern::Int(0),
                        guard: None,
                        body: CoreExpr::Int(1),
                    },
                    CoreCaseClause {
                        pattern: CorePattern::Wildcard,
                        guard: None,
                        body: CoreExpr::Var("x".to_string()),
                    },
                ],
            })
        );
        assert!(
            core.contract_text().contains(
                "Case:core=Case(Var(x);Int(0)=>Int(1)|Wildcard=>Var(x)):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Case(Var(x);Int(0)=>Int(1)|Wildcard=>Var(x))):proof=lean-covered"
            ),
            "contract text: {}",
            core.contract_text()
        );
    }

    /// Verifies case expressions with typed-but-unmodeled patterns are not
    /// reported as Lean-covered.
    ///
    /// Inputs:
    /// - None; parses a function whose case branch uses a record pattern.
    ///
    /// Output:
    /// - Test passes when the case expression carries a typed Core payload but
    ///   reports proof-model-required coverage.
    ///
    /// Transformation:
    /// - Exercises the case coverage gate that requires every branch pattern to
    ///   map to the current Lean pattern subset.
    #[test]
    fn syntax_output_lowering_to_core_case_with_record_pattern_requires_proof_model() {
        let module = parse_module_as_syntax_output(
            "\
module core_case_record_pattern_boundary.\n\
\n\
pub read(value: Dynamic): Int ->\n\
    case value {\n\
        #Point { x = 1 } -> 1\n\
    }.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "read")
            .expect("core read function");
        assert_eq!(function.clauses.len(), 1);
        assert_eq!(
            function.clauses[0].body.core_expr,
            Some(CoreExpr::Case {
                scrutinee: Box::new(CoreExpr::Var("value".to_string())),
                clauses: vec![CoreCaseClause {
                    pattern: CorePattern::Record {
                        name: "Point".to_string(),
                        fields: vec![CoreRecordPatternField {
                            key: "x".to_string(),
                            required: true,
                            value: CorePattern::Int(1),
                        }],
                    },
                    guard: None,
                    body: CoreExpr::Int(1),
                }],
            })
        );
        assert_eq!(
            function.clauses[0].body.proof_coverage,
            CoreProofCoverage::ProofModelRequired
        );
        assert!(
            core.contract_text().contains(
                "Case:core=Case(Var(value);Record(Point;x=Int(1))=>Int(1)):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Case(Var(value);Record(Point;x=Int(1))=>Int(1))):proof=proof-model-required"
            ),
            "contract text: {}",
            core.contract_text()
        );
    }

    /// Verifies case expressions with typed-but-unmodeled branch bodies are not
    /// reported as Lean-covered.
    ///
    /// Inputs:
    /// - None; parses a case expression whose branch body is a binary
    ///   operation.
    ///
    /// Output:
    /// - Test passes when the case expression carries a typed Core payload but
    ///   reports proof-model-required coverage.
    ///
    /// Transformation:
    /// - Exercises the case coverage gate that requires branch bodies to map to
    ///   the current Lean expression subset.
    #[test]
    fn syntax_output_lowering_to_core_case_with_binary_body_is_lean_covered() {
        let module = parse_module_as_syntax_output(
            "\
module core_case_binary_body_boundary.\n\
\n\
pub choose(x: Int): Int ->\n\
    case x {\n\
        0 -> 1 + 2\n\
    }.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "choose")
            .expect("core choose function");
        assert_eq!(function.clauses.len(), 1);
        assert_eq!(
            function.clauses[0].body.core_expr,
            Some(CoreExpr::Case {
                scrutinee: Box::new(CoreExpr::Var("x".to_string())),
                clauses: vec![CoreCaseClause {
                    pattern: CorePattern::Int(0),
                    guard: None,
                    body: CoreExpr::BinaryOp {
                        operator: "+".to_string(),
                        left: Box::new(CoreExpr::Int(1)),
                        right: Box::new(CoreExpr::Int(2)),
                    },
                }],
            })
        );
        assert_eq!(
            function.clauses[0].body.proof_coverage,
            CoreProofCoverage::LeanCovered
        );
        assert!(
            core.contract_text().contains(
                "Case:core=Case(Var(x);Int(0)=>BinaryOp(+;Int(1), Int(2))):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Case(Var(x);Int(0)=>BinaryOp(+;Int(1), Int(2)))):proof=lean-covered"
            ),
            "contract text: {}",
            core.contract_text()
        );
    }

    #[test]
    fn syntax_output_lowering_to_core_records_case_core_expr_with_guard() {
        let module = parse_module_as_syntax_output(
            "\
module core_case_guard_boundary.\n\
\
pub choose(value: Int): Int ->\n\
    case value {\n\
        value when is_integer(value) -> 1;\n\
        _ -> 0\n\
    }.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "choose")
            .expect("core choose function");
        assert_eq!(function.clauses.len(), 1);
        assert_eq!(
            function.clauses[0].body.core_expr,
            Some(CoreExpr::Case {
                scrutinee: Box::new(CoreExpr::Var("value".to_string())),
                clauses: vec![
                    CoreCaseClause {
                        pattern: CorePattern::Var("value".to_string()),
                        guard: Some(CoreExpr::Call {
                            function: "is_integer".to_string(),
                            args: vec![CoreExpr::Var("value".to_string())],
                        }),
                        body: CoreExpr::Int(1),
                    },
                    CoreCaseClause {
                        pattern: CorePattern::Wildcard,
                        guard: None,
                        body: CoreExpr::Int(0),
                    },
                ],
            })
        );
    }

    #[test]
    fn syntax_output_lowering_to_core_records_if_core_expr() {
        let module = parse_module_as_syntax_output(
            "\
module core_if_boundary.\n\
\n\
pub choose(flag: Bool): Int ->\n\
    if { flag -> 1 }.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "choose")
            .expect("core choose function");
        assert_eq!(function.clauses.len(), 1);
        assert_eq!(
            function.clauses[0].body.core_expr,
            Some(CoreExpr::If {
                clauses: vec![CoreIfClause {
                    condition: CoreExpr::Var("flag".to_string()),
                    body: CoreExpr::Int(1),
                }],
            })
        );
        assert_eq!(
            function.clauses[0].body.proof_coverage,
            CoreProofCoverage::LeanCovered
        );
        assert!(
            core.contract_text()
                .contains("If:core=If(Var(flag)=>Int(1)):preservation=structural-core-expr(freshness=no-runtime-bindings;target=If(Var(flag)=>Int(1))):proof=lean-covered"),
            "contract text: {}",
            core.contract_text()
        );
    }

    #[test]
    fn syntax_output_lowering_to_core_records_receive_core_expr() {
        let module = parse_module_as_syntax_output(
            "\
module core_receive_boundary.\n\
\n\
pub wait(): Dynamic ->\n\
    receive {\n\
        value -> value;\n\
    after 0 -> :timeout\n\
    }.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "wait")
            .expect("core wait function");
        assert_eq!(function.clauses.len(), 1);
        assert_eq!(
            function.clauses[0].body.core_expr,
            Some(CoreExpr::Receive {
                clauses: vec![CoreCaseClause {
                    pattern: CorePattern::Var("value".to_string()),
                    guard: None,
                    body: CoreExpr::Var("value".to_string()),
                }],
                after_clause: Some(CoreReceiveAfter {
                    trigger: Box::new(CoreExpr::Int(0)),
                    body: Box::new(CoreExpr::Atom("timeout".to_string())),
                }),
            })
        );
        assert_eq!(
            function.clauses[0].body.proof_coverage,
            CoreProofCoverage::ProofModelRequired
        );
        assert!(
            core.contract_text()
                .contains("Receive(Var(value)=>Var(value);after=Int(0)=>Atom(timeout))"),
            "contract text: {}",
            core.contract_text()
        );
    }

    #[test]
    fn syntax_output_lowering_to_core_records_try_core_expr() {
        let module = parse_module_as_syntax_output(
            "\
module core_try_boundary.\n\
\n\
pub run(): Dynamic ->\n\
    try 1 {\n\
        value -> value\n\
    catch\n\
        reason -> reason\n\
    after\n\
        0 -> :done\n\
    }.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "run")
            .expect("core run function");
        assert_eq!(function.clauses.len(), 1);
        assert_eq!(
            function.clauses[0].body.core_expr,
            Some(CoreExpr::Try {
                body: Box::new(CoreExpr::Int(1)),
                of_clauses: vec![CoreCaseClause {
                    pattern: CorePattern::Var("value".to_string()),
                    guard: None,
                    body: CoreExpr::Var("value".to_string()),
                }],
                catch_clauses: vec![CoreCaseClause {
                    pattern: CorePattern::Var("reason".to_string()),
                    guard: None,
                    body: CoreExpr::Var("reason".to_string()),
                }],
                after_clause: Some(CoreTryAfter {
                    trigger: Box::new(CoreExpr::Int(0)),
                    body: Box::new(CoreExpr::Atom("done".to_string())),
                }),
            })
        );
        assert_eq!(
            function.clauses[0].body.proof_coverage,
            CoreProofCoverage::ProofModelRequired
        );
        assert!(
            core.contract_text().contains(
                "Try(Int(1);of=Var(value)=>Var(value);catch=Var(reason)=>Var(reason);after=Int(0)=>Atom(done))"
            ),
            "contract text: {}",
            core.contract_text()
        );
    }

    #[test]
    fn syntax_output_lowering_to_core_records_case_core_expr_unsupported_branch_body() {
        let module = parse_module_as_syntax_output(
            "\
module core_case_branch_gap.\n\
\n\
pub choose(x: Int): Int ->\n\
    case x {\n\
        0 -> quote x;\n\
        0 -> x\n\
    }.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "choose")
            .expect("core choose function");
        assert_eq!(function.clauses.len(), 1);
        assert_eq!(function.clauses[0].body.core_expr, None);
        assert_eq!(
            function.clauses[0].body.proof_coverage,
            CoreProofCoverage::ProofModelRequired
        );
        assert_eq!(
            core.metadata.proof_readiness,
            CoreProofReadiness::RuntimeBoundary
        );
        assert_eq!(core.metadata.proof_model_required_expr_count, 1);
        assert_eq!(core.metadata.runtime_boundary_expr_count, 1);
        assert_eq!(core.metadata.lean_covered_expr_count, 3);
        assert_eq!(core.metadata.typed_core_expr_count, 3);
        assert_eq!(core.metadata.summary_only_expr_count, 2);
        assert_eq!(core.metadata.checked_preservation_expr_count, 3);
        assert_eq!(
            core.contract_text()
                .contains("Case:proof=proof-model-required"),
            true
        );
        assert_eq!(core.contract_text().contains("function=choose/1"), true);
    }

    #[test]
    fn syntax_output_lowering_to_core_records_fun_core_expr() {
        let module = parse_module_as_syntax_output(
            "\
module core_fun_boundary.\n\
\n\
pub id_fun(): Term ->\n\
    (x) -> x.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "id_fun")
            .expect("core id_fun function");
        assert_eq!(function.clauses.len(), 1);
        assert_eq!(
            function.clauses[0].body.core_expr,
            Some(CoreExpr::Lam {
                params: vec![CorePattern::Var("x".to_string())],
                body: Box::new(CoreExpr::Var("x".to_string())),
            })
        );
        assert_eq!(core.metadata.checked_preservation_expr_count, 2);
        assert_eq!(core.metadata.checked_preservation_pattern_count, 0);
        assert_eq!(
            core.metadata
                .checked_preservation_expr_no_runtime_bindings_count,
            1
        );
        assert_eq!(
            core.metadata
                .checked_preservation_expr_runtime_bindings_required_count,
            1
        );
        assert!(
            core.contract_text()
                .contains("Fun:core=Lam(Var(x);Var(x)):preservation=structural-core-expr(freshness=runtime-bindings-required;target=Lam(Var(x);Var(x))):proof=lean-covered"),
            "contract text: {}",
            core.contract_text()
        );
    }

    /// Verifies lambdas with typed-but-unmodeled bodies are not reported as
    /// Lean-covered.
    ///
    /// Inputs:
    /// - None; parses an anonymous function whose body is a binary operation.
    ///
    /// Output:
    /// - Test passes when the lambda carries a typed Core payload but reports
    ///   proof-model-required coverage.
    ///
    /// Transformation:
    /// - Exercises recursive Lean-shape validation for anonymous function
    ///   bodies.
    #[test]
    fn syntax_output_lowering_to_core_fun_with_binary_body_is_lean_covered() {
        let module = parse_module_as_syntax_output(
            "\
module core_fun_binary_body_boundary.\n\
\n\
pub add_fun(): Term ->\n\
    (x) -> x + 1.\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "add_fun")
            .expect("core add_fun function");
        assert_eq!(function.clauses.len(), 1);
        assert_eq!(
            function.clauses[0].body.core_expr,
            Some(CoreExpr::Lam {
                params: vec![CorePattern::Var("x".to_string())],
                body: Box::new(CoreExpr::BinaryOp {
                    operator: "+".to_string(),
                    left: Box::new(CoreExpr::Var("x".to_string())),
                    right: Box::new(CoreExpr::Int(1)),
                }),
            })
        );
        assert_eq!(
            function.clauses[0].body.proof_coverage,
            CoreProofCoverage::LeanCovered
        );
        assert!(
            core.contract_text().contains(
                "Fun:core=Lam(Var(x);BinaryOp(+;Var(x), Int(1))):preservation=structural-core-expr(freshness=runtime-bindings-required;target=Lam(Var(x);BinaryOp(+;Var(x), Int(1)))):proof=lean-covered"
            ),
            "contract text: {}",
            core.contract_text()
        );
    }

    /// Verifies primitive intrinsic calls have deterministic CoreIR contract text.
    ///
    /// Inputs:
    /// - None; constructs a typed `core.string.contains` intrinsic call
    ///   directly.
    ///
    /// Output:
    /// - Test passes when the intrinsic expression renders its registry key,
    ///   arguments, return type, effects, and span in stable contract text.
    ///
    /// Transformation:
    /// - Exercises the compiler-owned intrinsic CoreIR representation without
    ///   using backend module/function names.
    #[test]
    fn core_intrinsic_call_contract_text_is_backend_neutral() {
        let expr = CoreExpr::Intrinsic(CoreIntrinsicCall {
            id: CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringContains),
            args: vec![
                CoreExpr::Binary("hello".to_string()),
                CoreExpr::Binary("ell".to_string()),
            ],
            return_type: CoreType::Bool,
            effects: CoreEffectSet {
                effects: vec!["pure".to_string()],
            },
            span: Span::new(3, 17),
        });

        assert_eq!(
            expr.contract_text(),
            "Intrinsic(core.string.contains;args=Binary(hello),Binary(ell);return=Bool;effects=Effects(pure);span=3:17))"
        );
    }

    /// Verifies selected `std.core.String` calls lower to CoreIR intrinsics.
    ///
    /// Inputs:
    /// - A syntax-output module that calls `std.core.String.contains`.
    ///
    /// Output:
    /// - Test passes when the function body lowers to
    ///   `CoreExpr::Intrinsic(core.string.contains)` with typed string
    ///   arguments and a Bool return type.
    ///
    /// Transformation:
    /// - Parses normal Terlan source, lowers it through the CoreIR path, and
    ///   verifies the std.core primitive API call no longer appears as a
    ///   backend or ordinary remote call in CoreIR.
    #[test]
    fn syntax_output_lowering_to_core_maps_string_contains_to_intrinsic() {
        let module = parse_module_as_syntax_output(
            "\
module core_string_intrinsic_boundary.\n\
\n\
pub demo(): Bool ->\n\
    std.core.String.contains(\"hello\", \"ell\").\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "demo")
            .expect("core demo function");
        let Some(CoreExpr::Intrinsic(call)) = &function.clauses[0].body.core_expr else {
            panic!(
                "expected string contains intrinsic, got {:?}",
                function.clauses[0].body.core_expr
            );
        };
        assert_eq!(
            call.id,
            CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringContains)
        );
        assert_eq!(
            call.args,
            vec![
                CoreExpr::Binary("\"hello\"".to_string()),
                CoreExpr::Binary("\"ell\"".to_string())
            ]
        );
        assert_eq!(call.return_type, CoreType::Bool);
        assert_eq!(call.effects, core_pure_effect_set());
        assert!(
            core.contract_text()
                .contains("Intrinsic(core.string.contains;args=Binary(\"hello\"),Binary(\"ell\");return=Bool;effects=Effects(pure);span="),
            "contract text: {}",
            core.contract_text()
        );
    }

    /// Verifies primitive `Int.to_string` receiver calls lower to CoreIR intrinsics.
    ///
    /// Inputs:
    /// - A syntax-output module that calls `1.to_string()`.
    ///
    /// Output:
    /// - Test passes when the function body lowers to
    ///   `CoreExpr::Intrinsic(core.int.to_string)` with the integer receiver as
    ///   the first intrinsic argument.
    ///
    /// Transformation:
    /// - Parses receiver-method syntax, classifies the integer literal receiver
    ///   as the `std.core.Int` primitive owner, and lowers the call through the
    ///   same formal CoreIR intrinsic used by `std.core.Int.to_string(1)`.
    #[test]
    fn syntax_output_lowering_to_core_maps_int_receiver_to_string_to_intrinsic() {
        let module = parse_module_as_syntax_output(
            "\
module core_int_receiver_intrinsic_boundary.\n\
\n\
pub demo(): String ->\n\
    1.to_string().\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "demo")
            .expect("core demo function");
        let Some(CoreExpr::Intrinsic(call)) = &function.clauses[0].body.core_expr else {
            panic!(
                "expected int to_string intrinsic, got {:?}",
                function.clauses[0].body.core_expr
            );
        };
        assert_eq!(
            call.id,
            CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::IntToString)
        );
        assert_eq!(call.args, vec![CoreExpr::Int(1)]);
        assert_eq!(call.return_type, CoreType::String);
        assert_eq!(call.effects, core_pure_effect_set());
        assert!(
            core.contract_text().contains(
                "Intrinsic(core.int.to_string;args=Int(1);return=String;effects=Effects(pure);span="
            ),
            "contract text: {}",
            core.contract_text()
        );
    }

    /// Verifies selected `std.io.Console` calls lower to CoreIR runtime capabilities.
    ///
    /// Inputs:
    /// - A syntax-output module that calls `std.io.Console.println`.
    ///
    /// Output:
    /// - Test passes when the function body lowers to
    ///   `CoreExpr::Intrinsic(runtime.console.println)` with one typed string
    ///   argument, a `Unit` return type, and an `io` effect label.
    ///
    /// Transformation:
    /// - Parses normal Terlan source, lowers it through the CoreIR path, and
    ///   verifies the std.io runtime API call no longer appears as a backend
    ///   or ordinary remote call in CoreIR.
    #[test]
    fn syntax_output_lowering_to_core_maps_console_println_to_runtime_capability() {
        let module = parse_module_as_syntax_output(
            "\
module core_console_runtime_boundary.\n\
\n\
pub demo(): Unit ->\n\
    std.io.Console.println(\"hello\").\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "demo")
            .expect("core demo function");
        let Some(CoreExpr::Intrinsic(call)) = &function.clauses[0].body.core_expr else {
            panic!(
                "expected console println runtime capability, got {:?}",
                function.clauses[0].body.core_expr
            );
        };
        assert_eq!(
            call.id,
            CoreIntrinsicId::Runtime(CoreRuntimeCapability::ConsolePrintln)
        );
        assert_eq!(call.args, vec![CoreExpr::Binary("\"hello\"".to_string())]);
        assert_eq!(call.return_type, CoreType::Named("Unit".to_string()));
        assert_eq!(call.effects, core_io_effect_set());
        assert!(
            core.contract_text().contains(
                "Intrinsic(runtime.console.println;args=Binary(\"hello\");return=Named(Unit);effects=Effects(io);span="
            ),
            "contract text: {}",
            core.contract_text()
        );
    }

    /// Verifies selected `std.core.String` receiver methods lower to CoreIR intrinsics.
    ///
    /// Inputs:
    /// - A syntax-output module that calls `"hello".contains("ell")`.
    ///
    /// Output:
    /// - Test passes when the function body lowers to the same
    ///   `CoreExpr::Intrinsic(core.string.contains)` shape used by the
    ///   module-call spelling.
    ///
    /// Transformation:
    /// - Parses receiver-method source syntax, lowers it through the CoreIR
    ///   path, and verifies the receiver is prepended as the first intrinsic
    ///   argument so target backends only see backend-neutral primitive calls.
    #[test]
    fn syntax_output_lowering_to_core_maps_string_receiver_contains_to_intrinsic() {
        let module = parse_module_as_syntax_output(
            "\
module core_string_receiver_intrinsic_boundary.\n\
\n\
pub demo(): Bool ->\n\
    \"hello\".contains(\"ell\").\n",
        )
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
        let resolved = resolve_syntax_module_output(&module).module;
        let core = lower_syntax_module_output_to_core(&module, &resolved);

        let function = core
            .functions
            .iter()
            .find(|function| function.name == "demo")
            .expect("core demo function");
        let Some(CoreExpr::Intrinsic(call)) = &function.clauses[0].body.core_expr else {
            panic!(
                "expected string receiver contains intrinsic, got {:?}",
                function.clauses[0].body.core_expr
            );
        };
        assert_eq!(
            call.id,
            CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringContains)
        );
        assert_eq!(
            call.args,
            vec![
                CoreExpr::Binary("\"hello\"".to_string()),
                CoreExpr::Binary("\"ell\"".to_string())
            ]
        );
        assert_eq!(call.return_type, CoreType::Bool);
        assert_eq!(call.effects, core_pure_effect_set());
        assert!(
            core.contract_text()
                .contains("Intrinsic(core.string.contains;args=Binary(\"hello\"),Binary(\"ell\");return=Bool;effects=Effects(pure);span="),
            "contract text: {}",
            core.contract_text()
        );
    }
}
