#[derive(Debug, Clone, PartialEq, Eq)]
/// Backend-neutral type representation in CoreIR.
///
/// Inputs:
/// - Typechecked Terlan type information.
///
/// Outputs:
/// - CoreIR type payload consumed by proof checks and backend lowering.
///
/// Transformation:
/// - Removes source syntax details while preserving semantic type shape.
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
/// Tuple element shape in CoreIR types.
///
/// Inputs:
/// - Positional or named tuple type element from typechecking.
///
/// Outputs:
/// - CoreIR tuple element preserving whether the element is positional or
///   field-like.
///
/// Transformation:
/// - Keeps named tuple fields explicit for backend-neutral contracts.
pub enum CoreTupleTypeElem {
    Type(CoreType),
    Field { name: String, ty: CoreType },
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// CoreIR map type field.
///
/// Inputs:
/// - Map field key, operator, and value type from checked type data.
///
/// Outputs:
/// - Backend-neutral map field type payload.
///
/// Transformation:
/// - Preserves field operator text so required/optional semantics can be
///   validated later without backend syntax.
pub struct CoreMapTypeField {
    pub key: String,
    pub operator: String,
    pub value: CoreType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// CoreIR struct type field.
///
/// Inputs:
/// - Struct field name and checked field type.
///
/// Outputs:
/// - Backend-neutral struct field payload.
///
/// Transformation:
/// - Records field identity, type, and source visibility without committing to
///   backend layout.
pub struct CoreStructTypeField {
    pub name: String,
    pub ty: CoreType,
    pub is_private: bool,
}

impl CoreStructTypeField {
    /// Renders a typed Core struct field as deterministic contract text.
    ///
    /// Inputs:
    /// - `self`: typed struct field payload.
    ///
    /// Output:
    /// - Stable compact `name:type` or `#name:type` text for CoreIR contracts.
    ///
    /// Transformation:
    /// - Serializes field identity and typed payload without backend-specific
    ///   struct layout assumptions.
    pub(crate) fn contract_text(&self) -> String {
        let privacy = if self.is_private { "#" } else { "" };
        format!("{}{}:{}", privacy, self.name, self.ty.contract_text())
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
    pub(crate) fn contract_text(&self) -> String {
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
    pub(super) fn contract_text(&self) -> String {
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
pub(super) fn core_type_contract_text(ty: Option<&CoreType>) -> String {
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
                match escaped {
                    '"' => output.push('"'),
                    '\\' => output.push('\\'),
                    'n' => output.push('\n'),
                    'r' => output.push('\r'),
                    't' => output.push('\t'),
                    other => output.push(other),
                }
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
