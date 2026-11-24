use oxc_ast::{
    ast::{
        BindingPattern, FormalParameter, TSConstructorType, TSSignature, TSTupleElement, TSType,
        TSTypeAnnotation, TSTypeOperatorOperator,
    },
    Comment,
};

use super::super::ts_type_mapping::{
    TsConditionalType, TsConstructorType, TsPrimitiveType, TsRecordField, TsTupleElement, TsTypeRef,
};
use super::declarations::{TsParameterDeclaration, TsParseError};

#[path = "types/literal_types.rs"]
mod literal_types;
use literal_types::{parse_literal_type, type_name};
pub(super) use literal_types::{property_key_name, unsupported};

/// Normalizes one raw TypeScript JSDoc comment body.
///
/// Inputs:
/// - `source`: original `.d.ts` source text.
/// - `comment`: Oxc comment known to be a JSDoc comment.
///
/// Output:
/// - Cleaned doc text without `/**`, leading `*`, or closing `*/`.
/// - `None` when normalization removes all content.
///
/// Transformation:
/// - Slices the comment through Oxc spans and converts TypeScript JSDoc into a
///   language-neutral body that the Terlan generator can re-wrap.
pub(super) fn normalize_jsdoc(source: &str, comment: Comment) -> Option<String> {
    let span = comment.content_span();
    let body = source.get(span.start as usize..span.end as usize)?;
    let normalized = body
        .lines()
        .map(|line| line.trim().strip_prefix('*').unwrap_or(line.trim()).trim())
        .collect::<Vec<_>>();
    let first = normalized
        .iter()
        .position(|line| !line.is_empty())
        .unwrap_or(normalized.len());
    let last = normalized
        .iter()
        .rposition(|line| !line.is_empty())
        .map(|index| index + 1)
        .unwrap_or(first);
    if first >= last {
        None
    } else {
        Some(normalized[first..last].join("\n"))
    }
}

/// Builds a source label for an interface member.
///
/// Inputs:
/// - `member`: Oxc interface member signature.
///
/// Output:
/// - Best-effort source member label.
///
/// Transformation:
/// - Reads static property or method keys when available and otherwise falls
///   back to `member` so unsupported skip rows remain deterministic.
pub(super) fn interface_member_source(member: &TSSignature<'_>) -> String {
    match member {
        TSSignature::TSPropertySignature(property) => {
            property_key_name(&property.key).unwrap_or_else(|_| "property".to_string())
        }
        TSSignature::TSMethodSignature(method) => {
            property_key_name(&method.key).unwrap_or_else(|_| "method".to_string())
        }
        TSSignature::TSIndexSignature(_) => "index_signature".to_string(),
        TSSignature::TSCallSignatureDeclaration(_) => "call_signature".to_string(),
        TSSignature::TSConstructSignatureDeclaration(_) => "construct_signature".to_string(),
    }
}

/// Converts one Oxc TypeScript signature parameter.
///
/// Inputs:
/// - `parameter`: Oxc formal parameter from a method signature.
///
/// Output:
/// - `Ok(TsParameterDeclaration)` for simple identifier parameters.
/// - `Err(TsParseError)` for destructuring or missing type annotations.
///
/// Transformation:
/// - Preserves parameter names and optionality for later wrapper generation.
pub(super) fn parse_parameter(
    parameter: &FormalParameter<'_>,
) -> Result<TsParameterDeclaration, TsParseError> {
    let name = match &parameter.pattern {
        BindingPattern::BindingIdentifier(binding) => binding.name.to_string(),
        BindingPattern::ObjectPattern(_)
        | BindingPattern::ArrayPattern(_)
        | BindingPattern::AssignmentPattern(_) => {
            return Err(unsupported(
                "ts_bindgen.unsupported_parameter_pattern",
                "only identifier parameters are supported",
            ));
        }
    };

    Ok(TsParameterDeclaration {
        name,
        optional: parameter.optional,
        rest: false,
        ty: parse_optional_type_annotation(parameter.type_annotation.as_deref())?,
    })
}

/// Converts an optional Oxc type annotation.
///
/// Inputs:
/// - `annotation`: optional Oxc TypeScript annotation.
///
/// Output:
/// - `Ok(TsTypeRef)` for supported annotations.
/// - `Err(TsParseError)` when the annotation is missing.
///
/// Transformation:
/// - Treats missing declarations as unsupported instead of broadening them to
///   `Any`, keeping generated bindings explicit.
pub(super) fn parse_optional_type_annotation(
    annotation: Option<&TSTypeAnnotation<'_>>,
) -> Result<TsTypeRef, TsParseError> {
    annotation
        .map(|annotation| parse_type(&annotation.type_annotation))
        .unwrap_or_else(|| {
            Err(unsupported(
                "ts_bindgen.missing_type_annotation",
                "generated bindings require explicit TypeScript annotations",
            ))
        })
}

/// Converts an Oxc TypeScript type into the neutral type mapper model.
///
/// Inputs:
/// - `ty`: Oxc TypeScript type node.
///
/// Output:
/// - `Ok(TsTypeRef)` for the supported first DOM slice.
/// - `Err(TsParseError)` for unsupported shapes.
///
/// Transformation:
/// - Reuses the existing neutral `TsTypeRef` vocabulary so parsing and Terlan
///   type mapping share one generator-owned contract.
pub(super) fn parse_type(ty: &TSType<'_>) -> Result<TsTypeRef, TsParseError> {
    match ty {
        TSType::TSStringKeyword(_) => Ok(TsTypeRef::Primitive(TsPrimitiveType::String)),
        TSType::TSNumberKeyword(_) => Ok(TsTypeRef::Primitive(TsPrimitiveType::Number)),
        TSType::TSBooleanKeyword(_) => Ok(TsTypeRef::Primitive(TsPrimitiveType::Boolean)),
        TSType::TSVoidKeyword(_) => Ok(TsTypeRef::Primitive(TsPrimitiveType::Void)),
        TSType::TSNullKeyword(_) => Ok(TsTypeRef::Null),
        TSType::TSUndefinedKeyword(_) => Ok(TsTypeRef::Undefined),
        TSType::TSNeverKeyword(_) => Ok(TsTypeRef::Never),
        TSType::TSAnyKeyword(_) => Ok(TsTypeRef::Any),
        TSType::TSUnknownKeyword(_) => Ok(TsTypeRef::Unknown),
        TSType::TSObjectKeyword(_) => Ok(TsTypeRef::Object),
        TSType::TSThisType(_) => Ok(TsTypeRef::Named("$this".into())),
        TSType::TSFunctionType(function) => {
            if function.type_parameters.is_some() || function.params.rest.is_some() {
                return Err(unsupported(
                    "ts_bindgen.unsupported_callback_parameters",
                    "generic and variadic callbacks require an explicit adapter",
                ));
            }
            let params = function
                .params
                .items
                .iter()
                .map(parse_parameter)
                .map(|param| {
                    let param = param?;
                    if param.optional {
                        return Err(unsupported(
                            "ts_bindgen.unsupported_callback_parameters",
                            "optional callback arguments require a nullable-value adapter",
                        ));
                    }
                    Ok(param.ty)
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(TsTypeRef::Callback {
                params,
                return_type: Box::new(parse_type(&function.return_type.type_annotation)?),
            })
        }
        TSType::TSArrayType(array) => {
            Ok(TsTypeRef::Array(Box::new(parse_type(&array.element_type)?)))
        }
        TSType::TSTupleType(tuple) => Ok(TsTypeRef::Tuple(
            tuple
                .element_types
                .iter()
                .map(parse_tuple_element)
                .collect::<Result<_, _>>()?,
        )),
        TSType::TSTypeOperatorType(operator)
            if operator.operator == TSTypeOperatorOperator::Readonly
                && matches!(operator.type_annotation, TSType::TSTupleType(_)) =>
        {
            parse_type(&operator.type_annotation)
        }
        TSType::TSUnionType(union) => {
            let items = union
                .types
                .iter()
                .map(parse_type_argument)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(TsTypeRef::Union(items))
        }
        TSType::TSLiteralType(literal) => parse_literal_type(&literal.literal),
        TSType::TSTypeReference(reference) => parse_type_reference(reference),
        TSType::TSParenthesizedType(parenthesized) => parse_type(&parenthesized.type_annotation),
        TSType::TSTypeLiteral(_) => parse_type_literal(ty),
        TSType::TSConditionalType(conditional) => {
            Ok(TsTypeRef::Conditional(Box::new(TsConditionalType {
                check: parse_type_operand(&conditional.check_type),
                constraint: parse_type_operand(&conditional.extends_type),
                then_type: parse_type_operand(&conditional.true_type),
                else_type: parse_type_operand(&conditional.false_type),
                distribute_over: None,
            })))
        }
        TSType::TSBigIntKeyword(_)
        | TSType::TSIntrinsicKeyword(_)
        | TSType::TSSymbolKeyword(_)
        | TSType::TSConstructorType(_)
        | TSType::TSImportType(_)
        | TSType::TSIndexedAccessType(_)
        | TSType::TSInferType(_)
        | TSType::TSIntersectionType(_)
        | TSType::TSMappedType(_)
        | TSType::TSNamedTupleMember(_)
        | TSType::TSTemplateLiteralType(_)
        | TSType::TSTypeOperatorType(_)
        | TSType::TSTypePredicate(_)
        | TSType::TSTypeQuery(_)
        | TSType::JSDocNullableType(_)
        | TSType::JSDocNonNullableType(_)
        | TSType::JSDocUnknownType(_) => Err(unsupported(
            "ts_bindgen.unsupported_type",
            "TypeScript type shape is outside the first DOM binding slice",
        )),
    }
}

fn parse_tuple_element(element: &TSTupleElement<'_>) -> Result<TsTupleElement, TsParseError> {
    let (ty, optional, rest) = match element {
        TSTupleElement::TSNamedTupleMember(named) => {
            let mut element = parse_tuple_element(&named.element_type)?;
            element.label = Some(named.label.name.to_string());
            element.optional |= named.optional;
            return Ok(element);
        }
        TSTupleElement::TSOptionalType(value) => (&value.type_annotation, true, false),
        TSTupleElement::TSRestType(value) => (&value.type_annotation, false, true),
        other => (other.to_ts_type(), false, false),
    };
    if let TSType::TSNamedTupleMember(named) = ty {
        let mut element = parse_tuple_element(&named.element_type)?;
        element.label = Some(named.label.name.to_string());
        element.optional |= named.optional || optional;
        element.rest |= rest;
        return Ok(element);
    }
    Ok(TsTupleElement {
        label: None,
        ty: parse_type(ty)?,
        optional,
        rest,
    })
}

pub(super) fn parse_type_operand(ty: &TSType<'_>) -> TsTypeRef {
    match ty {
        TSType::TSParenthesizedType(ty) => parse_type_operand(&ty.type_annotation),
        TSType::TSConstructorType(constructor) => parse_constructor_type(constructor),
        TSType::TSUnionType(union) => {
            TsTypeRef::Union(union.types.iter().map(parse_type_operand).collect())
        }
        TSType::TSFunctionType(function) if is_rest_only(function) => {
            let rest = function.params.rest.as_ref().unwrap();
            match &rest.type_annotation {
                Some(rest) => TsTypeRef::RestCallback {
                    rest: Box::new(parse_type_operand(&rest.type_annotation)),
                    return_type: Box::new(parse_type_operand(
                        &function.return_type.type_annotation,
                    )),
                },
                None => TsTypeRef::Deferred {
                    reason: "ts_bindgen.missing_type_annotation",
                    detail: "rest parameter requires an explicit annotation".into(),
                },
            }
        }
        _ => parse_type(ty).unwrap_or_else(|error| TsTypeRef::Deferred {
            reason: error.reason,
            detail: error.message,
        }),
    }
}

pub(super) fn parse_constructor_type(constructor: &TSConstructorType<'_>) -> TsTypeRef {
    if constructor.type_parameters.is_some() {
        return TsTypeRef::ConstructorType;
    }
    let Ok(params) = super::members::callable_parameters(&constructor.params) else {
        return TsTypeRef::ConstructorType;
    };
    TsTypeRef::Constructor(Box::new(TsConstructorType {
        is_abstract: constructor.r#abstract,
        params: tuple_parameters(params),
        instance: parse_type_operand(&constructor.return_type.type_annotation),
    }))
}

pub(super) fn tuple_parameters(params: Vec<TsParameterDeclaration>) -> Vec<TsTupleElement> {
    params
        .into_iter()
        .map(|parameter| TsTupleElement {
            label: Some(parameter.name),
            ty: parameter.ty,
            optional: parameter.optional,
            rest: parameter.rest,
        })
        .collect()
}

fn parse_type_argument(ty: &TSType<'_>) -> Result<TsTypeRef, TsParseError> {
    match ty {
        TSType::TSParenthesizedType(ty) => parse_type_argument(&ty.type_annotation),
        TSType::TSConstructorType(constructor) => Ok(parse_constructor_type(constructor)),
        TSType::TSFunctionType(function) if is_rest_only(function) => Ok(parse_type_operand(ty)),
        _ => parse_type(ty),
    }
}

fn is_rest_only(function: &oxc_ast::ast::TSFunctionType<'_>) -> bool {
    function.this_param.is_none()
        && function.type_parameters.is_none()
        && function.params.items.is_empty()
        && function.params.rest.is_some()
}

/// Converts an Oxc TypeScript type reference into named or generic form.
///
/// Inputs:
/// - `reference`: Oxc TypeScript type reference.
///
/// Output:
/// - `Ok(TsTypeRef::Named)` when no type arguments exist.
/// - `Ok(TsTypeRef::Generic)` when type arguments are present.
/// - `Err(TsParseError)` when any argument is unsupported.
///
/// Transformation:
/// - Preserves the source constructor name while lowering each type argument
///   through the same neutral type mapper vocabulary.
fn parse_type_reference(
    reference: &oxc_ast::ast::TSTypeReference<'_>,
) -> Result<TsTypeRef, TsParseError> {
    let name = type_name(&reference.type_name)?;
    let Some(type_arguments) = reference.type_arguments.as_deref() else {
        return Ok(TsTypeRef::Named(name));
    };
    if name == "ReturnType" {
        if let [TSType::TSTypeQuery(query)] = type_arguments.params.as_slice() {
            if query.type_arguments.is_none()
                && query
                    .expr_name
                    .as_ts_type_name()
                    .and_then(|name| type_name(name).ok())
                    .as_deref()
                    == Some("JSON.parse")
            {
                return Ok(TsTypeRef::JsonParseResult);
            }
        }
    }
    let args = type_arguments
        .params
        .iter()
        .map(parse_type_argument)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(TsTypeRef::Generic { name, args })
}

/// Converts an Oxc TypeScript object type literal into a neutral record type.
///
/// Inputs:
/// - `ty`: Oxc TypeScript type node expected to be `TSTypeLiteral`.
///
/// Output:
/// - `Ok(TsTypeRef::Record)` for named property signatures.
/// - `Err(TsParseError)` for method, index, call, or construct signatures.
///
/// Transformation:
/// - Keeps anonymous object fields available to the type mapper without
///   treating them as broad dynamic `object`.
fn parse_type_literal(ty: &TSType<'_>) -> Result<TsTypeRef, TsParseError> {
    let TSType::TSTypeLiteral(type_literal) = ty else {
        return Err(unsupported(
            "ts_bindgen.internal_type_literal_mismatch",
            "expected a TypeScript type literal",
        ));
    };

    let fields = type_literal
        .members
        .iter()
        .map(parse_type_literal_field)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(TsTypeRef::Record(fields))
}

/// Converts one TypeScript object-literal member into a neutral record field.
///
/// Inputs:
/// - `member`: Oxc TypeScript type-literal signature.
///
/// Output:
/// - `Ok(TsRecordField)` for named property signatures.
/// - `Err(TsParseError)` for callable or indexed object members.
///
/// Transformation:
/// - Preserves field names, optionality, and field type references for record
///   mapping.
fn parse_type_literal_field(member: &TSSignature<'_>) -> Result<TsRecordField, TsParseError> {
    match member {
        TSSignature::TSPropertySignature(property) => Ok(TsRecordField {
            name: property_key_name(&property.key)?,
            optional: property.optional,
            ty: parse_optional_type_annotation(property.type_annotation.as_deref())?,
        }),
        TSSignature::TSIndexSignature(_)
        | TSSignature::TSCallSignatureDeclaration(_)
        | TSSignature::TSConstructSignatureDeclaration(_)
        | TSSignature::TSMethodSignature(_) => Err(unsupported(
            "ts_bindgen.unsupported_record_member",
            "only property members are supported in record type literals",
        )),
    }
}
