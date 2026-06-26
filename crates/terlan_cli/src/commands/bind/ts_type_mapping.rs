#![allow(dead_code)]

// Consumed by the upcoming Oxc TypeScript parser adapter; tests pin the
// contract before the generator command is wired to real `.d.ts` inputs.

/// TypeScript primitive type names accepted by the binding mapper.
///
/// Inputs:
/// - Values are produced by a future TypeScript declaration adapter.
///
/// Output:
/// - A closed primitive vocabulary that can be mapped into Terlan type text.
///
/// Transformation:
/// - Keeps primitive mapping independent from Oxc AST node shapes so the
///   generator owns a stable neutral model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TsPrimitiveType {
    String,
    Number,
    Boolean,
    Void,
}

/// Neutral TypeScript record field used by binding-generation mapping.
///
/// Inputs:
/// - Values model TypeScript object/type-literal fields after parsing.
///
/// Output:
/// - Field name, optional marker, and field type reference.
///
/// Transformation:
/// - Keeps anonymous object fields structured until the mapper emits a Terlan
///   named-tuple type or records a skip diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TsRecordField {
    pub(super) name: String,
    pub(super) optional: bool,
    pub(super) ty: TsTypeRef,
}

/// Neutral TypeScript type reference used by binding-generation mapping.
///
/// Inputs:
/// - Values model selected TypeScript declaration type shapes after parsing.
///
/// Output:
/// - Structured type references for the mapper.
///
/// Transformation:
/// - Separates Oxc parsing from Terlan type generation and lets unsupported
///   shapes carry stable skip reasons before generated files are written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TsTypeRef {
    Primitive(TsPrimitiveType),
    Named(String),
    Generic {
        name: String,
        args: Vec<TsTypeRef>,
    },
    StringLiteral(String),
    NumberLiteral(String),
    BooleanLiteral(bool),
    Array(Box<TsTypeRef>),
    Union(Vec<TsTypeRef>),
    Record(Vec<TsRecordField>),
    Callback {
        params: Vec<TsTypeRef>,
        return_type: Box<TsTypeRef>,
    },
    OverloadSet(Vec<TsTypeRef>),
    Null,
    Undefined,
    Function,
    Object,
    Any,
    Unknown,
}

/// Result of mapping one TypeScript type reference.
///
/// Inputs:
/// - Produced by `map_ts_type_to_terlan`.
///
/// Output:
/// - Terlan type text when mapping is conservative and supported.
/// - Stable skip diagnostics when mapping would be lossy or unsafe.
///
/// Transformation:
/// - Gives the generator a manifest-ready contract: every unsupported type
///   shape has a reason code instead of silently becoming `Dynamic`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TsTypeMapping {
    pub(super) terlan_type: Option<String>,
    pub(super) skipped: Vec<TsTypeSkip>,
}

/// Stable skip diagnostic for a TypeScript type mapping.
///
/// Inputs:
/// - Produced when a TypeScript type cannot be emitted conservatively.
///
/// Output:
/// - Reason code and source type label suitable for generated manifests.
///
/// Transformation:
/// - Converts mapper refusal into deterministic metadata for release review
///   and CI drift checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TsTypeSkip {
    pub(super) reason: &'static str,
    pub(super) source: String,
}

/// Maps a neutral TypeScript type into Terlan type text.
///
/// Inputs:
/// - `ty`: neutral TypeScript type reference from the future parser adapter.
///
/// Output:
/// - `TsTypeMapping` with either a Terlan type or stable skip diagnostics.
///
/// Transformation:
/// - Maps primitive, named, generic, record, callback, array, optional,
///   nullable, and conservative simple-union shapes. Rejects unresolved
///   overload sets and broad dynamic TypeScript shapes with manifest-ready
///   reason codes.
pub(super) fn map_ts_type_to_terlan(ty: &TsTypeRef) -> TsTypeMapping {
    match map_ts_type_inner(ty) {
        Ok(terlan_type) => TsTypeMapping {
            terlan_type: Some(terlan_type),
            skipped: Vec::new(),
        },
        Err(skip) => TsTypeMapping {
            terlan_type: None,
            skipped: vec![skip],
        },
    }
}

/// Maps one TypeScript type or returns one stable skip diagnostic.
///
/// Inputs:
/// - `ty`: neutral TypeScript type reference.
///
/// Output:
/// - `Ok(String)` with Terlan type text for supported shapes.
/// - `Err(TsTypeSkip)` for unsupported or lossy shapes.
///
/// Transformation:
/// - Delegates union handling to the optional/simple-union mapper and keeps
///   scalar mapping local and deterministic. Callback types are skipped until
///   Terlan's public function-type syntax is stable in generated interfaces.
fn map_ts_type_inner(ty: &TsTypeRef) -> Result<String, TsTypeSkip> {
    match ty {
        TsTypeRef::Primitive(TsPrimitiveType::String) => Ok("std.js.String.JsString".to_string()),
        TsTypeRef::Primitive(TsPrimitiveType::Number) => Ok("std.js.Number.JsNumber".to_string()),
        TsTypeRef::Primitive(TsPrimitiveType::Boolean) => Ok("std.core.Bool".to_string()),
        TsTypeRef::Primitive(TsPrimitiveType::Void) => Ok("std.core.Unit".to_string()),
        TsTypeRef::Named(name) => Ok(name.clone()),
        TsTypeRef::Generic { name, args } => map_ts_generic_to_terlan(name, args),
        TsTypeRef::StringLiteral(_)
        | TsTypeRef::NumberLiteral(_)
        | TsTypeRef::BooleanLiteral(_) => Err(skip_type("ts_bindgen.unsupported_literal_type", ty)),
        TsTypeRef::Array(item) => Ok(format!("std.js.Array[{}]", map_ts_type_inner(item)?)),
        TsTypeRef::Union(items) => map_ts_union_to_terlan(items),
        TsTypeRef::Record(fields) => map_ts_record_to_terlan(fields),
        TsTypeRef::Callback {
            params: _,
            return_type: _,
        } => Err(skip_type("ts_bindgen.unsupported_callback_type", ty)),
        TsTypeRef::OverloadSet(_) => Err(skip_type("ts_bindgen.overload_requires_resolution", ty)),
        TsTypeRef::Null | TsTypeRef::Undefined => {
            Err(skip_type("ts_bindgen.nullish_without_value_type", ty))
        }
        TsTypeRef::Function => Err(skip_type("ts_bindgen.unsupported_function_type", ty)),
        TsTypeRef::Object => Err(skip_type("ts_bindgen.unsupported_object_type", ty)),
        TsTypeRef::Any => Err(skip_type("ts_bindgen.unsupported_any", ty)),
        TsTypeRef::Unknown => Err(skip_type("ts_bindgen.unsupported_unknown", ty)),
    }
}

/// Maps a generic TypeScript reference into Terlan type text.
///
/// Inputs:
/// - `name`: TypeScript generic type constructor name.
/// - `args`: neutral type arguments.
///
/// Output:
/// - `Ok(String)` with Terlan generic syntax for supported constructors.
/// - `Err(TsTypeSkip)` when a type argument is unsupported.
///
/// Transformation:
/// - Special-cases source-known JS wrappers such as `Promise` and `Array`, then
///   preserves other generic constructors with Terlan bracket arguments.
fn map_ts_generic_to_terlan(name: &str, args: &[TsTypeRef]) -> Result<String, TsTypeSkip> {
    let mapped_args = args
        .iter()
        .map(map_ts_type_inner)
        .collect::<Result<Vec<_>, _>>()?;
    let terlan_name = match name {
        "Array" | "ReadonlyArray" => "std.js.Array",
        "Promise" => "std.js.Promise",
        other => other,
    };

    Ok(format!("{terlan_name}[{}]", mapped_args.join(", ")))
}

/// Maps an anonymous TypeScript object shape into a Terlan named tuple type.
///
/// Inputs:
/// - `fields`: neutral record fields in source order.
///
/// Output:
/// - `Ok(String)` with Terlan `{name: Type}` syntax.
/// - `Err(TsTypeSkip)` when any field type is unsupported.
///
/// Transformation:
/// - Treats optional fields as `Option[T]` by adding `undefined` to the field
///   type before mapping.
fn map_ts_record_to_terlan(fields: &[TsRecordField]) -> Result<String, TsTypeSkip> {
    let mut mapped = Vec::new();
    for field in fields {
        let field_ty = if field.optional {
            TsTypeRef::Union(vec![field.ty.clone(), TsTypeRef::Undefined])
        } else {
            field.ty.clone()
        };
        mapped.push(format!("{}: {}", field.name, map_ts_type_inner(&field_ty)?));
    }
    Ok(format!("{{{}}}", mapped.join(", ")))
}

/// Maps a TypeScript callback type into a Terlan arrow type.
///
/// Inputs:
/// - `params`: callback parameter types.
/// - `return_type`: callback return type.
///
/// Output:
/// - `Ok(String)` with Terlan `(A, B) -> R` syntax.
/// - `Err(TsTypeSkip)` when any parameter or return type is unsupported.
///
/// Transformation:
/// - Emits the canonical Terlan type-arrow form already defined by EBNF.
fn map_ts_callback_to_terlan(
    params: &[TsTypeRef],
    return_type: &TsTypeRef,
) -> Result<String, TsTypeSkip> {
    let params = params
        .iter()
        .map(map_ts_type_inner)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(format!(
        "({}) -> {}",
        params.join(", "),
        map_ts_type_inner(return_type)?
    ))
}

/// Maps optional, nullable, and conservative TypeScript unions.
///
/// Inputs:
/// - `items`: union members in source order.
///
/// Output:
/// - `Ok(String)` for nullable single-type unions, nullable simple unions, and
///   non-null simple unions.
/// - `Err(TsTypeSkip)` when any member is complex or unsupported.
///
/// Transformation:
/// - Removes `null` and `undefined` into an `Option[...]` wrapper and emits
///   only simple scalar/named/list unions. Complex members fail with a stable
///   reason before generated bindings are written.
fn map_ts_union_to_terlan(items: &[TsTypeRef]) -> Result<String, TsTypeSkip> {
    let mut is_optional = false;
    let mut mapped = Vec::new();

    for item in items {
        match item {
            TsTypeRef::Null | TsTypeRef::Undefined => {
                is_optional = true;
            }
            other if is_simple_union_member(other) => mapped.push(map_ts_type_inner(other)?),
            other => return Err(skip_type("ts_bindgen.complex_union", other)),
        }
    }

    mapped.sort();
    mapped.dedup();

    if mapped.is_empty() {
        return Err(skip_type(
            "ts_bindgen.nullish_without_value_type",
            &TsTypeRef::Union(items.to_vec()),
        ));
    }

    let inner = mapped.join(" | ");

    if is_optional {
        Ok(format!("Option[{inner}]"))
    } else {
        Ok(inner)
    }
}

/// Returns whether a TypeScript type is safe inside a simple union.
///
/// Inputs:
/// - `ty`: neutral TypeScript type reference.
///
/// Output:
/// - `true` for primitive, named, literal, generic, and array members whose
///   nested types are also simple.
///
/// Transformation:
/// - Defines the conservative union subset admitted before the broader DOM
///   type mapper exists.
fn is_simple_union_member(ty: &TsTypeRef) -> bool {
    match ty {
        TsTypeRef::Primitive(_)
        | TsTypeRef::Named(_)
        | TsTypeRef::StringLiteral(_)
        | TsTypeRef::NumberLiteral(_)
        | TsTypeRef::BooleanLiteral(_) => true,
        TsTypeRef::Array(item) => is_simple_union_member(item),
        TsTypeRef::Generic { args, .. } => args.iter().all(is_simple_union_member),
        TsTypeRef::Union(_)
        | TsTypeRef::Record(_)
        | TsTypeRef::Callback { .. }
        | TsTypeRef::OverloadSet(_)
        | TsTypeRef::Null
        | TsTypeRef::Undefined
        | TsTypeRef::Function
        | TsTypeRef::Object
        | TsTypeRef::Any
        | TsTypeRef::Unknown => false,
    }
}

/// Builds one stable TypeScript mapping skip diagnostic.
///
/// Inputs:
/// - `reason`: stable manifest reason code.
/// - `ty`: source type shape that could not be mapped.
///
/// Output:
/// - `TsTypeSkip` containing reason and debug source label.
///
/// Transformation:
/// - Converts mapper failures into deterministic manifest entries without
///   requiring the caller to inspect enum internals.
fn skip_type(reason: &'static str, ty: &TsTypeRef) -> TsTypeSkip {
    TsTypeSkip {
        reason,
        source: format!("{ty:?}"),
    }
}
