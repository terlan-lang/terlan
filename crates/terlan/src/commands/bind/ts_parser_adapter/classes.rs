//! Instance-side class declarations share the interface binding contract.
use oxc_ast::{ast::*, Comment};

use super::declarations::*;
use super::types::{
    parse_optional_type_annotation, parse_parameter, parse_type, property_key_name, unsupported,
};

pub(super) fn heritage(
    expression: &Expression<'_>,
    arguments: Option<&TSTypeParameterInstantiation<'_>>,
) -> Result<super::super::ts_type_mapping::TsTypeRef, TsParseError> {
    use super::super::ts_type_mapping::TsTypeRef;
    let name = match expression {
        Expression::Identifier(id) => id.name.to_string(),
        Expression::StaticMemberExpression(member) => {
            let TsTypeRef::Named(parent) = heritage(&member.object, None)? else {
                unreachable!()
            };
            format!("{parent}.{}", member.property.name)
        }
        _ => {
            return Err(unsupported(
                "ts_bindgen.unsupported_heritage",
                "base type must be a named declaration",
            ))
        }
    };
    match arguments {
        Some(arguments) => Ok(TsTypeRef::Generic {
            name,
            args: arguments
                .params
                .iter()
                .map(parse_type)
                .collect::<Result<_, _>>()?,
        }),
        None => Ok(TsTypeRef::Named(name)),
    }
}

pub(super) fn parse_class(
    source: &str,
    comments: &[Comment],
    namespace: &str,
    class: &Class<'_>,
) -> Result<Vec<TsDeclaration>, TsParseError> {
    let Some(id) = &class.id else {
        return Err(unsupported(
            "ts_bindgen.unnamed_class",
            "binding classes require a public name",
        ));
    };
    let mut members = Vec::new();
    for element in &class.body.body {
        let label = match element {
            ClassElement::PropertyDefinition(value) => property_key_name(&value.key).ok(),
            ClassElement::MethodDefinition(value) => property_key_name(&value.key).ok(),
            _ => None,
        }
        .unwrap_or_else(|| "class_member".into());
        let parsed = match element {
            ClassElement::PropertyDefinition(property)
                if !property.r#static
                    && matches!(property.accessibility, None | Some(TSAccessibility::Public)) =>
            {
                // ECMAScript #private fields have no externally accessible key.
                if matches!(property.key, PropertyKey::PrivateIdentifier(_)) {
                    continue;
                }
                (|| {
                    Ok(TsInterfaceMember::Property(TsPropertyDeclaration {
                        name: property_key_name(&property.key)?,
                        doc: leading_jsdoc(source, comments, property.span),
                        readonly: property.readonly,
                        optional: property.optional,
                        ty: parse_optional_type_annotation(property.type_annotation.as_deref())?,
                    }))
                })()
            }
            ClassElement::MethodDefinition(method)
                if !method.r#static
                    && matches!(method.accessibility, None | Some(TSAccessibility::Public))
                    && method.kind != MethodDefinitionKind::Constructor =>
            {
                if matches!(method.key, PropertyKey::PrivateIdentifier(_)) {
                    continue;
                }
                (|| {
                    let name = property_key_name(&method.key)?;
                    if method.kind == MethodDefinitionKind::Get {
                        return Ok(TsInterfaceMember::Property(TsPropertyDeclaration {
                            name,
                            doc: leading_jsdoc(source, comments, method.span),
                            readonly: true,
                            optional: method.optional,
                            ty: parse_optional_type_annotation(
                                method.value.return_type.as_deref(),
                            )?,
                        }));
                    }
                    if method.kind == MethodDefinitionKind::Set {
                        return Err(unsupported(
                            "ts_bindgen.unsupported_property_setter",
                            "standalone class setters are not emitted",
                        ));
                    }
                    if method
                        .value
                        .type_parameters
                        .as_ref()
                        .is_some_and(|parameters| {
                            parameters
                                .params
                                .iter()
                                .any(|parameter| parameter.constraint.is_some())
                        })
                    {
                        return Err(unsupported(
                            "ts_bindgen.unsupported_generic_constraint",
                            "generic method constraints must not be erased",
                        ));
                    }
                    let mut params = method
                        .value
                        .params
                        .items
                        .iter()
                        .map(parse_parameter)
                        .collect::<Result<Vec<_>, _>>()?;
                    if let Some(rest) = &method.value.params.rest {
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
                        name,
                        doc: leading_jsdoc(source, comments, method.span),
                        optional: method.optional,
                        type_params: parse_type_parameter_names(
                            method.value.type_parameters.as_deref(),
                        ),
                        params,
                        return_type: parse_optional_type_annotation(
                            method.value.return_type.as_deref(),
                        )?,
                    }))
                })()
            }
            ClassElement::TSIndexSignature(_) => Err(unsupported(
                "ts_bindgen.unsupported_interface_signature",
                "class index signatures are not emitted",
            )),
            ClassElement::AccessorProperty(value)
                if matches!(value.accessibility, None | Some(TSAccessibility::Public)) =>
            {
                Err(unsupported(
                    "ts_bindgen.unsupported_class_accessor",
                    "class auto-accessor bindings are not emitted",
                ))
            }
            ClassElement::MethodDefinition(value)
                if matches!(value.accessibility, None | Some(TSAccessibility::Public)) =>
            {
                Err(unsupported(
                    "ts_bindgen.unsupported_class_static_or_constructor",
                    "class constructors and static methods need constructor-side bindings",
                ))
            }
            ClassElement::PropertyDefinition(value)
                if value.r#static
                    && matches!(value.accessibility, None | Some(TSAccessibility::Public)) =>
            {
                Err(unsupported(
                    "ts_bindgen.unsupported_class_static_or_constructor",
                    "static fields need constructor-side bindings",
                ))
            }
            _ => continue,
        };
        members.push(parsed.unwrap_or_else(|error: TsParseError| {
            TsInterfaceMember::Unsupported(TsUnsupportedMember {
                source: label,
                reason: error.reason,
                detail: error.message,
            })
        }));
    }
    let mut parents = Vec::new();
    if let Some(parent) = &class.super_class {
        add_parent(
            parent,
            class.super_type_arguments.as_deref(),
            &mut parents,
            &mut members,
        );
    }
    Ok(vec![TsDeclaration::Interface(TsInterfaceDeclaration {
        parents,
        plain_record: false,
        namespace: namespace.into(),
        name: id.name.to_string(),
        doc: leading_jsdoc(source, comments, class.span),
        type_params: parse_type_parameter_names(class.type_parameters.as_deref()),
        members,
    })])
}

pub(super) fn add_parent(
    expression: &Expression<'_>,
    arguments: Option<&TSTypeParameterInstantiation<'_>>,
    parents: &mut Vec<super::super::ts_type_mapping::TsTypeRef>,
    members: &mut Vec<TsInterfaceMember>,
) {
    match heritage(expression, arguments) {
        Ok(parent) => parents.push(parent),
        Err(error) => members.push(TsInterfaceMember::Unsupported(TsUnsupportedMember {
            source: "extends".into(),
            reason: "ts_bindgen.unsupported_heritage",
            detail: format!("{}: {}", error.reason, error.message),
        })),
    }
}
