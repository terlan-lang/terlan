//! Maps parsed TypeScript declaration types to checked Terlan signatures.

use crate::terlan_hir::source_name_to_terlan_identifier;

/// TypeScript primitive type names accepted by the binding mapper.
///
/// Inputs:
/// - Values are produced by the TypeScript declaration parser.
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TsTupleElement {
    pub(super) label: Option<String>,
    pub(super) ty: TsTypeRef,
    pub(super) optional: bool,
    pub(super) rest: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TsConditionalType {
    pub(super) check: TsTypeRef,
    pub(super) constraint: TsTypeRef,
    pub(super) then_type: TsTypeRef,
    pub(super) else_type: TsTypeRef,
    pub(super) distribute_over: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TsConstructorType {
    pub(super) is_abstract: bool,
    pub(super) params: Vec<TsTupleElement>,
    pub(super) instance: TsTypeRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TsConstructorUtility {
    Instance,
    Parameters,
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
    UnresolvedGenericArguments(String),
    StringLiteral(String),
    NumberLiteral(String),
    BooleanLiteral(bool),
    Array(Box<TsTypeRef>),
    Tuple(Vec<TsTupleElement>),
    Union(Vec<TsTypeRef>),
    Record(Vec<TsRecordField>),
    Callback {
        params: Vec<TsTypeRef>,
        return_type: Box<TsTypeRef>,
    },
    RestCallback {
        rest: Box<TsTypeRef>,
        return_type: Box<TsTypeRef>,
    },
    CallableBound,
    JsonParseResult,
    Conditional(Box<TsConditionalType>),
    /// Constructor-only operands can be distinguished from callable types.
    ConstructorType,
    Constructor(Box<TsConstructorType>),
    ConstructorUtility(TsConstructorUtility, Box<TsTypeRef>),
    Deferred {
        reason: &'static str,
        detail: String,
    },
    Never,
    #[cfg(test)]
    OverloadSet(Vec<TsTypeRef>),
    Null,
    Undefined,
    Object,
    Any,
    Unknown,
}

impl TsTypeRef {
    pub(super) fn requires_declaration_graph(&self) -> bool {
        match self {
            Self::Never
            | Self::Tuple(_)
            | Self::Conditional(_)
            | Self::Constructor(_)
            | Self::ConstructorUtility(..)
            | Self::JsonParseResult => true,
            Self::Generic { args, .. } | Self::Union(args) => {
                args.iter().any(Self::requires_declaration_graph)
            }
            Self::Array(item) => item.requires_declaration_graph(),
            Self::Record(fields) => fields
                .iter()
                .any(|field| field.ty.requires_declaration_graph()),
            Self::Callback {
                params,
                return_type,
            } => {
                params.iter().any(Self::requires_declaration_graph)
                    || return_type.requires_declaration_graph()
            }
            _ => false,
        }
    }

    pub(super) fn for_each_child_mut(&mut self, mut visit: impl FnMut(&mut Self)) {
        match self {
            Self::Generic { args, .. } | Self::Union(args) => args.iter_mut().for_each(visit),
            Self::Array(item) => visit(item),
            Self::Tuple(elements) => elements
                .iter_mut()
                .for_each(|element| visit(&mut element.ty)),
            Self::Record(fields) => fields.iter_mut().for_each(|field| visit(&mut field.ty)),
            Self::Callback {
                params,
                return_type,
            } => {
                params.iter_mut().for_each(&mut visit);
                visit(return_type);
            }
            Self::Conditional(conditional) => {
                visit(&mut conditional.check);
                visit(&mut conditional.constraint);
                visit(&mut conditional.then_type);
                visit(&mut conditional.else_type);
            }
            Self::RestCallback { rest, return_type } => {
                visit(rest);
                visit(return_type);
            }
            Self::Constructor(constructor) => {
                constructor
                    .params
                    .iter_mut()
                    .for_each(|parameter| visit(&mut parameter.ty));
                visit(&mut constructor.instance);
            }
            Self::ConstructorUtility(_, argument) => visit(argument),
            _ => {}
        }
    }
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
///   scalar and callback mapping local and deterministic.
fn map_ts_type_inner(ty: &TsTypeRef) -> Result<String, TsTypeSkip> {
    match ty {
        TsTypeRef::Primitive(TsPrimitiveType::String) => Ok("std.js.String.JsString".to_string()),
        TsTypeRef::Primitive(TsPrimitiveType::Number) => Ok("std.js.Number.JsNumber".to_string()),
        TsTypeRef::Primitive(TsPrimitiveType::Boolean) => Ok("Bool".to_string()),
        TsTypeRef::Primitive(TsPrimitiveType::Void) => Ok("Unit".to_string()),
        TsTypeRef::Named(name) => Ok(name.clone()),
        TsTypeRef::Generic { name, args } => map_ts_generic_to_terlan(name, args),
        TsTypeRef::UnresolvedGenericArguments(detail) => Err(TsTypeSkip {
            reason: "ts_bindgen.unresolved_generic_arguments",
            source: detail.clone(),
        }),
        TsTypeRef::Conditional(_) => Err(TsTypeSkip {
            reason: "ts_bindgen.unsupported_type",
            source: "conditional type requires a proven extends test".into(),
        }),
        TsTypeRef::ConstructorType | TsTypeRef::Constructor(_) => {
            Err(skip_type("ts_bindgen.unsupported_type", ty))
        }
        TsTypeRef::ConstructorUtility(utility, _) => Err(TsTypeSkip {
            reason: "ts_bindgen.unsupported_type",
            source: format!("{utility:?} utility requires a known constructor signature"),
        }),
        TsTypeRef::RestCallback { .. } => {
            Err(skip_type("ts_bindgen.unsupported_callback_parameters", ty))
        }
        TsTypeRef::CallableBound | TsTypeRef::JsonParseResult => {
            Err(skip_type("ts_bindgen.unsupported_type", ty))
        }
        TsTypeRef::Deferred { reason, detail } => Err(TsTypeSkip {
            reason,
            source: detail.clone(),
        }),
        TsTypeRef::Never => Ok("Never".into()),
        TsTypeRef::StringLiteral(_)
        | TsTypeRef::NumberLiteral(_)
        | TsTypeRef::BooleanLiteral(_) => Err(skip_type("ts_bindgen.unsupported_literal_type", ty)),
        TsTypeRef::Array(item) => Ok(format!("std.js.Array[{}]", map_ts_type_inner(item)?)),
        TsTypeRef::Tuple(elements)
            if !elements.is_empty()
                && elements
                    .iter()
                    .all(|element| !element.rest && !element.optional) =>
        {
            let elements = elements
                .iter()
                .map(|element| map_ts_type_inner(&element.ty))
                .collect::<Result<Vec<_>, _>>()?;
            if elements.iter().any(|ty| ty.starts_with("Option[")) {
                return Err(skip_type("ts_bindgen.unsupported_tuple_element", ty));
            }
            Ok(format!("{{{}}}", elements.join(", ")))
        }
        TsTypeRef::Tuple(_) => Err(skip_type("ts_bindgen.tuple_requires_binding", ty)),
        TsTypeRef::Union(items) => map_ts_union_to_terlan(items),
        TsTypeRef::Record(fields) => map_ts_record_to_terlan(fields),
        TsTypeRef::Callback {
            params,
            return_type,
        } => {
            let parameters = params
                .iter()
                .map(map_ts_type_inner)
                .collect::<Result<Vec<_>, _>>()?;
            let result = map_ts_type_inner(return_type)?;
            let result = if matches!(return_type.as_ref(), TsTypeRef::Callback { .. }) {
                format!("({result})")
            } else {
                result
            };
            Ok(format!("({}) -> {result}", parameters.join(", ")))
        }
        #[cfg(test)]
        TsTypeRef::OverloadSet(_) => Err(skip_type("ts_bindgen.overload_requires_resolution", ty)),
        TsTypeRef::Null | TsTypeRef::Undefined => {
            Err(skip_type("ts_bindgen.nullish_without_value_type", ty))
        }
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
        mapped.push(format!(
            "{}: {}",
            source_name_to_terlan_identifier(&field.name),
            map_ts_type_inner(&field_ty)?
        ));
    }
    Ok(format!("{{{}}}", mapped.join(", ")))
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
            TsTypeRef::Never => {}
            other if is_simple_union_member(other) => mapped.push(map_ts_type_inner(other)?),
            other => return Err(skip_type("ts_bindgen.complex_union", other)),
        }
    }

    mapped.sort();
    mapped.dedup();

    if mapped.is_empty() && !is_optional && items.contains(&TsTypeRef::Never) {
        return Ok("Never".into());
    }
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
        | TsTypeRef::Tuple(_)
        | TsTypeRef::Callback { .. }
        | TsTypeRef::RestCallback { .. }
        | TsTypeRef::CallableBound
        | TsTypeRef::JsonParseResult
        | TsTypeRef::Conditional(_)
        | TsTypeRef::ConstructorType
        | TsTypeRef::Constructor(_)
        | TsTypeRef::ConstructorUtility(..)
        | TsTypeRef::Deferred { .. }
        | TsTypeRef::Never
        | TsTypeRef::UnresolvedGenericArguments(_)
        | TsTypeRef::Null
        | TsTypeRef::Undefined
        | TsTypeRef::Object
        | TsTypeRef::Any
        | TsTypeRef::Unknown => false,
        #[cfg(test)]
        TsTypeRef::OverloadSet(_) => false,
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
