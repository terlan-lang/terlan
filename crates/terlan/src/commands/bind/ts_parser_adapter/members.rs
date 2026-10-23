use super::declarations::{
    leading_jsdoc, TsInterfaceMember, TsMethodDeclaration, TsParameterDeclaration,
    TsPropertyDeclaration, TsUnsupportedMember,
};
use super::types::{
    interface_member_source, parse_optional_type_annotation, parse_parameter, property_key_name,
    unsupported,
};
use oxc_ast::{
    ast::{BindingPattern, TSMethodSignatureKind, TSSignature},
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
            let mut params = method
                .params
                .items
                .iter()
                .map(parse_parameter)
                .collect::<Result<Vec<_>, _>>()?;
            if let Some(rest) = &method.params.rest {
                let BindingPattern::BindingIdentifier(binding) = &rest.rest.argument else {
                    return Err(unsupported(
                        "ts_bindgen.unsupported_rest_pattern",
                        "rest arguments must use an identifier",
                    ));
                };
                params.push(TsParameterDeclaration {
                    name: binding.name.to_string(),
                    optional: false,
                    rest: true,
                    ty: parse_optional_type_annotation(rest.type_annotation.as_deref())?,
                });
            }
            Ok(TsInterfaceMember::Method(TsMethodDeclaration {
                name: property_key_name(&method.key)?,
                doc: leading_jsdoc(source, comments, method.span),
                optional: method.optional,
                params,
                return_type: parse_optional_type_annotation(method.return_type.as_deref())?,
            }))
        }
        TSSignature::TSIndexSignature(_)
        | TSSignature::TSCallSignatureDeclaration(_)
        | TSSignature::TSConstructSignatureDeclaration(_) => Err(unsupported(
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
