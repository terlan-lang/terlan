use terlan_hir::ModuleInterface;
use terlan_syntax::span::Span;

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
    pub(crate) fn contract_text(&self) -> String {
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
pub(crate) fn core_type_from_text(text: &str) -> Option<CoreType> {
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
pub(crate) fn atom_type_literal_payload(text: &str) -> Option<String> {
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
pub(crate) fn core_type_from_body_variants(body: &[String]) -> Option<CoreType> {
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
    pub(crate) fn contract_text(&self) -> String {
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
    pub(crate) fn combine(
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
    pub(crate) fn contract_text(&self) -> String {
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
    TypeOf,
    IsType,
    BoolEqual,
    BoolCompare,
    BoolToString,
    BoolFromString,
    AtomToString,
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
    ListNew,
    ListIsEmpty,
    ListLength,
    ListFirst,
    ListIterator,
    ListPush,
    ListClear,
    IteratorNext,
    MapNew,
    MapIsEmpty,
    MapSize,
    MapGet,
    MapContainsKey,
    MapIterator,
    MapPut,
    MapRemove,
    MapClear,
    SetNew,
    SetIsEmpty,
    SetSize,
    SetContains,
    SetIterator,
    SetAdd,
    SetRemove,
    SetClear,
    TaskDone,
    TaskResult,
    BeamAgentStart,
    BeamAgentGet,
    BeamAgentGetAndUpdate,
    BeamAgentUpdate,
    BeamAgentCast,
    BeamAgentStop,
    BeamGenServerStart,
    BeamGenServerCall,
    BeamGenServerCast,
    BeamGenServerStop,
    BeamNativeBridgeStart,
    BeamNativeBridgeCall,
    BeamNativeBridgeDispose,
    BeamNativeBridgeStop,
    BeamSupervisorChildSpec,
    BeamSupervisorStart,
    BeamSupervisorStop,
    BeamTaskStart,
    BeamTaskResult,
    BeamTaskCancel,
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
            Self::TypeOf => "core.type.type_of",
            Self::IsType => "core.type.is_type",
            Self::BoolEqual => "core.bool.equal",
            Self::BoolCompare => "core.bool.compare",
            Self::BoolToString => "core.bool.to_string",
            Self::BoolFromString => "core.bool.from_string",
            Self::AtomToString => "core.atom.to_string",
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
            Self::ListNew => "core.list.new",
            Self::ListIsEmpty => "core.list.is_empty",
            Self::ListLength => "core.list.length",
            Self::ListFirst => "core.list.first",
            Self::ListIterator => "core.list.iterator",
            Self::ListPush => "core.list.push",
            Self::ListClear => "core.list.clear",
            Self::IteratorNext => "core.iterator.next",
            Self::MapNew => "core.map.new",
            Self::MapIsEmpty => "core.map.is_empty",
            Self::MapSize => "core.map.size",
            Self::MapGet => "core.map.get",
            Self::MapContainsKey => "core.map.contains_key",
            Self::MapIterator => "core.map.iterator",
            Self::MapPut => "core.map.put",
            Self::MapRemove => "core.map.remove",
            Self::MapClear => "core.map.clear",
            Self::SetNew => "core.set.new",
            Self::SetIsEmpty => "core.set.is_empty",
            Self::SetSize => "core.set.size",
            Self::SetContains => "core.set.contains",
            Self::SetIterator => "core.set.iterator",
            Self::SetAdd => "core.set.add",
            Self::SetRemove => "core.set.remove",
            Self::SetClear => "core.set.clear",
            Self::TaskDone => "core.task.done",
            Self::TaskResult => "core.task.result",
            Self::BeamAgentStart => "beam.agent.start",
            Self::BeamAgentGet => "beam.agent.get",
            Self::BeamAgentGetAndUpdate => "beam.agent.get_and_update",
            Self::BeamAgentUpdate => "beam.agent.update",
            Self::BeamAgentCast => "beam.agent.cast",
            Self::BeamAgentStop => "beam.agent.stop",
            Self::BeamGenServerStart => "beam.gen_server.start",
            Self::BeamGenServerCall => "beam.gen_server.call",
            Self::BeamGenServerCast => "beam.gen_server.cast",
            Self::BeamGenServerStop => "beam.gen_server.stop",
            Self::BeamNativeBridgeStart => "beam.native_bridge.start",
            Self::BeamNativeBridgeCall => "beam.native_bridge.call",
            Self::BeamNativeBridgeDispose => "beam.native_bridge.dispose",
            Self::BeamNativeBridgeStop => "beam.native_bridge.stop",
            Self::BeamSupervisorChildSpec => "beam.supervisor.child_spec",
            Self::BeamSupervisorStart => "beam.supervisor.start",
            Self::BeamSupervisorStop => "beam.supervisor.stop",
            Self::BeamTaskStart => "beam.task.start",
            Self::BeamTaskResult => "beam.task.result",
            Self::BeamTaskCancel => "beam.task.cancel",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreRuntimeCapability {
    ConsolePrintln,
    FileExists,
    FileReadText,
    FileWriteText,
    FileAppendText,
    FileDelete,
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
            Self::FileExists => "runtime.file.exists",
            Self::FileReadText => "runtime.file.read_text",
            Self::FileWriteText => "runtime.file.write_text",
            Self::FileAppendText => "runtime.file.append_text",
            Self::FileDelete => "runtime.file.delete",
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
    MutableReceiverCall {
        receiver: Box<CoreExpr>,
        method: String,
        args: Vec<CoreExpr>,
        effects: CoreEffectSet,
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
    pub(crate) fn contract_text(&self) -> String {
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
            CoreExpr::MutableReceiverCall {
                receiver,
                method,
                args,
                effects,
            } => format!(
                "MutableReceiverCall({}.{};args={};effects={})",
                receiver.contract_text(),
                method,
                args.iter()
                    .map(CoreExpr::contract_text)
                    .collect::<Vec<_>>()
                    .join(","),
                effects.contract_text()
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
}

/// Backend-neutral trait conformance fact preserved in CoreIR.
///
/// Inputs:
/// - Syntax-output `implements` declarations or explicit `impl Trait for Type`
///   declarations.
///
/// Output:
/// - Stable conformance summary for downstream target-profile validation and
///   future backend lowering.
///
/// Transformation:
/// - Preserves trait reference text, owner type text, source category, and
///   visibility without lowering to target-specific runtime dictionaries.
///   Struct `derives` clauses are intentionally excluded because they derive
///   struct shape, not trait conformance.
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
