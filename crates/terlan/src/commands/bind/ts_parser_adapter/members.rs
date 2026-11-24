use super::super::ts_type_mapping::TsTypeRef;
use super::declarations::{
    leading_jsdoc, parse_type_parameter_names, TsCallKind, TsInterfaceMember, TsMethodDeclaration,
    TsParameterDeclaration, TsParseError, TsPropertyDeclaration, TsUnsupportedMember,
};
use super::types::{
    interface_member_source, parse_optional_type_annotation, parse_parameter, parse_type,
    property_key_name, unsupported,
};
use oxc_ast::{
    ast::{
        BindingPattern, FormalParameters, TSMethodSignatureKind, TSSignature, TSTypeAnnotation,
        TSTypeParameterDeclaration,
    },
    Comment,
};

/// Converts one Oxc interface signature into a neutral member.
///
/// Inputs:
/// - `member`: Oxc TypeScript interface signature.
///
/// Output:
/// - Property or method members for supported signatures.
/// - Unsupported members carrying stable skip metadata for unsupported shapes.
///
/// Transformation:
/// - Locks the first DOM binding slice to named members before broader
///   TypeScript declarations are admitted while allowing large standard libs to
///   keep generating around unsupported members.
pub(super) fn parse_interface_member(
    source: &str,
    comments: &[Comment],
    member: &TSSignature<'_>,
) -> TsInterfaceMember {
    let parsed = (|| match member {
        TSSignature::TSPropertySignature(property) => {
            Ok(TsInterfaceMember::Property(TsPropertyDeclaration {
                name: property_key_name(&property.key)?,
                doc: leading_jsdoc(source, comments, property.span),
                readonly: property.readonly,
                optional: property.optional,
                ty: parse_optional_type_annotation(property.type_annotation.as_deref())?,
            }))
        }
        TSSignature::TSMethodSignature(method) if method.kind == TSMethodSignatureKind::Get => {
            Ok(TsInterfaceMember::Property(TsPropertyDeclaration {
                name: property_key_name(&method.key)?,
                doc: leading_jsdoc(source, comments, method.span),
                readonly: true,
                optional: method.optional,
                ty: parse_optional_type_annotation(method.return_type.as_deref())?,
            }))
        }
        TSSignature::TSMethodSignature(method) if method.kind == TSMethodSignatureKind::Set => {
            Err(unsupported(
                "ts_bindgen.unsupported_property_setter",
                "property setter bindings are not emitted yet",
            ))
        }
        TSSignature::TSMethodSignature(method) => {
            let type_parameters = method.type_parameters.as_deref();
            Ok(TsInterfaceMember::Method(TsMethodDeclaration {
                call_kind: TsCallKind::Method,
                type_params: parse_type_parameter_names(type_parameters),
                type_constraints: callable_type_constraints(type_parameters)?,
                name: property_key_name(&method.key)?,
                doc: leading_jsdoc(source, comments, method.span),
                optional: method.optional,
                params: callable_parameters(&method.params)?,
                return_type: parse_optional_type_annotation(method.return_type.as_deref())?,
            }))
        }
        TSSignature::TSCallSignatureDeclaration(call) if call.this_param.is_some() => {
            Err(unsupported(
                "ts_bindgen.unsupported_call_receiver",
                "call signatures with an explicit this parameter require a receiver adapter",
            ))
        }
        TSSignature::TSCallSignatureDeclaration(call) => call_signature(
            TsCallKind::Call,
            call.type_parameters.as_deref(),
            &call.params,
            call.return_type.as_deref(),
            leading_jsdoc(source, comments, call.span),
        ),
        TSSignature::TSConstructSignatureDeclaration(call) => call_signature(
            TsCallKind::Construct,
            call.type_parameters.as_deref(),
            &call.params,
            call.return_type.as_deref(),
            leading_jsdoc(source, comments, call.span),
        ),
        TSSignature::TSIndexSignature(_) => Err(unsupported(
            "ts_bindgen.unsupported_interface_signature",
            "only named properties and methods are supported",
        )),
    })();

    parsed.unwrap_or_else(|error| {
        TsInterfaceMember::Unsupported(TsUnsupportedMember {
            source: interface_member_source(member),
            reason: error.reason,
            detail: error.message,
        })
    })
}

pub(super) fn call_signature(
    kind: TsCallKind,
    type_parameters: Option<&TSTypeParameterDeclaration<'_>>,
    parameters: &FormalParameters<'_>,
    return_type: Option<&TSTypeAnnotation<'_>>,
    doc: Option<String>,
) -> Result<TsInterfaceMember, TsParseError> {
    Ok(TsInterfaceMember::Method(TsMethodDeclaration {
        call_kind: kind,
        type_params: parse_type_parameter_names(type_parameters),
        type_constraints: callable_type_constraints(type_parameters)?,
        name: match kind {
            TsCallKind::Call => "call_signature",
            TsCallKind::Construct => "construct_signature",
            TsCallKind::Method | TsCallKind::TuplePack => unreachable!("not a call signature"),
        }
        .into(),
        doc,
        optional: false,
        params: callable_parameters(parameters)?,
        return_type: parse_optional_type_annotation(return_type)?,
    }))
}

pub(super) fn callable_type_constraints(
    parameters: Option<&TSTypeParameterDeclaration<'_>>,
) -> Result<Vec<Option<TsTypeRef>>, TsParseError> {
    parameters
        .into_iter()
        .flat_map(|parameters| &parameters.params)
        .map(|parameter| {
            let Some(constraint) = &parameter.constraint else {
                return Ok(None);
            };
            if let Ok(ty @ TsTypeRef::Record(_)) = parse_type(constraint) {
                if supported_shape(&ty) {
                    return Ok(Some(ty));
                }
            }
            Err(unsupported(
            "ts_bindgen.unsupported_generic_constraint",
            "only closed required-field constraints can be represented as structural implications",
        ))
        })
        .collect()
}

fn supported_shape(ty: &TsTypeRef) -> bool {
    match ty {
        TsTypeRef::Record(fields) => {
            !fields.is_empty()
                && fields.iter().all(|field| {
                    !field.optional
                        && crate::terlan_hir::source_name_to_terlan_identifier(&field.name)
                            == field.name
                        && supported_shape(&field.ty)
                })
        }
        TsTypeRef::Primitive(_) | TsTypeRef::Named(_) => true,
        _ => false,
    }
}

pub(super) fn callable_parameters(
    formal: &FormalParameters<'_>,
) -> Result<Vec<TsParameterDeclaration>, TsParseError> {
    let mut parameters = formal
        .items
        .iter()
        .map(parse_parameter)
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(rest) = &formal.rest {
        let BindingPattern::BindingIdentifier(binding) = &rest.rest.argument else {
            return Err(unsupported(
                "ts_bindgen.unsupported_rest_pattern",
                "rest arguments must use an identifier",
            ));
        };
        parameters.push(TsParameterDeclaration {
            name: binding.name.to_string(),
            optional: false,
            rest: true,
            ty: parse_optional_type_annotation(rest.type_annotation.as_deref())?,
        });
    }
    Ok(parameters)
}
