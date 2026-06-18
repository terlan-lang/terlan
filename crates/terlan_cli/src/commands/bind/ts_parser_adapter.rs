use oxc_ast::ast::{
    BindingPattern, FormalParameter, PropertyKey, Statement, TSLiteral, TSSignature, TSType,
    TSTypeAnnotation, TSTypeName,
};

use super::ts_type_mapping::{TsPrimitiveType, TsRecordField, TsTypeRef};

/// Neutral TypeScript declaration file model owned by the binding generator.
///
/// Inputs:
/// - Produced from Oxc's TypeScript parser for committed `.d.ts` files.
///
/// Output:
/// - A stable list of declarations supported by the current generator slice.
///
/// Transformation:
/// - Removes Oxc lifetimes and AST node details so later binding generation
///   stages consume a crate-local contract instead of parser internals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TsDeclarationFile {
    pub(super) declarations: Vec<TsDeclaration>,
}

/// Neutral TypeScript declaration accepted by the generator.
///
/// Inputs:
/// - Extracted from top-level TypeScript declarations.
///
/// Output:
/// - Currently supports interface declarations required by the DOM fixture.
///
/// Transformation:
/// - Gives unsupported declaration kinds a clear adapter boundary until later
///   roadmap slices add them deliberately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TsDeclaration {
    Interface(TsInterfaceDeclaration),
}

/// Neutral TypeScript interface declaration.
///
/// Inputs:
/// - Extracted from an Oxc `TSInterfaceDeclaration`.
///
/// Output:
/// - Interface name and supported members.
///
/// Transformation:
/// - Preserves source-level member order while dropping parser-only span and
///   scope metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TsInterfaceDeclaration {
    pub(super) name: String,
    pub(super) members: Vec<TsInterfaceMember>,
}

/// Neutral TypeScript interface member.
///
/// Inputs:
/// - Extracted from Oxc property and method signatures.
///
/// Output:
/// - Either a property or method contract for later Terlan wrapper generation.
///
/// Transformation:
/// - Separates field-like DOM properties from callable DOM methods before type
///   mapping or name conversion occurs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TsInterfaceMember {
    Property(TsPropertyDeclaration),
    Method(TsMethodDeclaration),
}

/// Neutral TypeScript interface property.
///
/// Inputs:
/// - Extracted from a named Oxc `TSPropertySignature`.
///
/// Output:
/// - Name, readonly flag, optional flag, and neutral type reference.
///
/// Transformation:
/// - Converts optional syntax into metadata while preserving nullability inside
///   the type reference itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TsPropertyDeclaration {
    pub(super) name: String,
    pub(super) readonly: bool,
    pub(super) optional: bool,
    pub(super) ty: TsTypeRef,
}

/// Neutral TypeScript interface method.
///
/// Inputs:
/// - Extracted from a named Oxc `TSMethodSignature`.
///
/// Output:
/// - Name, optional flag, parameters, and return type.
///
/// Transformation:
/// - Keeps method signatures independent from future receiver-function wrapper
///   generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TsMethodDeclaration {
    pub(super) name: String,
    pub(super) optional: bool,
    pub(super) params: Vec<TsParameterDeclaration>,
    pub(super) return_type: TsTypeRef,
}

/// Neutral TypeScript method parameter.
///
/// Inputs:
/// - Extracted from Oxc formal parameters in TypeScript signatures.
///
/// Output:
/// - Name, optional flag, and neutral type reference.
///
/// Transformation:
/// - Rejects destructured/rest parameter shapes until the generator can map
///   them without losing source semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TsParameterDeclaration {
    pub(super) name: String,
    pub(super) optional: bool,
    pub(super) ty: TsTypeRef,
}

/// Stable parser adapter error.
///
/// Inputs:
/// - Produced by Oxc parse failures or unsupported generator model shapes.
///
/// Output:
/// - Reason code and human-readable message for focused tests and future
///   binding manifests.
///
/// Transformation:
/// - Converts parser diagnostics and unsupported AST branches into deterministic
///   generator-contract failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TsParseError {
    pub(super) reason: &'static str,
    pub(super) message: String,
}

/// Parses TypeScript declarations into the generator-owned neutral model.
///
/// Inputs:
/// - `source`: `.d.ts` source text.
///
/// Output:
/// - `Ok(TsDeclarationFile)` when Oxc accepts the source and all encountered
///   declarations are supported by the current adapter.
/// - `Err(TsParseError)` when Oxc rejects the source or the adapter reaches an
///   unsupported declaration/member/type shape.
///
/// Transformation:
/// - Parses with Oxc using TypeScript-definition source mode, then walks only
///   the stable subset needed by the first `std.js.Dom` generator slice.
pub(super) fn parse_ts_declaration_file(source: &str) -> Result<TsDeclarationFile, TsParseError> {
    let allocator = oxc_allocator::Allocator::default();
    let source_type = oxc_span::SourceType::d_ts();
    let parsed = oxc_parser::Parser::new(&allocator, source, source_type).parse();
    if !parsed.errors.is_empty() {
        return Err(TsParseError {
            reason: "ts_bindgen.parse_failed",
            message: format!("{:?}", parsed.errors),
        });
    }

    let mut declarations = Vec::new();
    for statement in &parsed.program.body {
        if let Statement::TSInterfaceDeclaration(interface) = statement {
            declarations.push(TsDeclaration::Interface(parse_interface(interface)?));
        }
    }

    Ok(TsDeclarationFile { declarations })
}

/// Converts one Oxc interface declaration into the neutral model.
///
/// Inputs:
/// - `interface`: parsed Oxc TypeScript interface declaration.
///
/// Output:
/// - `Ok(TsInterfaceDeclaration)` for supported named members.
/// - `Err(TsParseError)` when a member uses an unsupported signature shape.
///
/// Transformation:
/// - Copies the interface name and delegates member conversion while preserving
///   source member order.
fn parse_interface(
    interface: &oxc_ast::ast::TSInterfaceDeclaration<'_>,
) -> Result<TsInterfaceDeclaration, TsParseError> {
    let members = interface
        .body
        .body
        .iter()
        .map(parse_interface_member)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(TsInterfaceDeclaration {
        name: interface.id.name.to_string(),
        members,
    })
}

/// Converts one Oxc interface signature into a neutral member.
///
/// Inputs:
/// - `member`: Oxc TypeScript interface signature.
///
/// Output:
/// - `Ok(TsInterfaceMember)` for property and method signatures.
/// - `Err(TsParseError)` for index, call, and construct signatures.
///
/// Transformation:
/// - Locks the first DOM binding slice to named members before broader
///   TypeScript declarations are admitted.
fn parse_interface_member(member: &TSSignature<'_>) -> Result<TsInterfaceMember, TsParseError> {
    match member {
        TSSignature::TSPropertySignature(property) => {
            Ok(TsInterfaceMember::Property(TsPropertyDeclaration {
                name: property_key_name(&property.key)?,
                readonly: property.readonly,
                optional: property.optional,
                ty: parse_optional_type_annotation(property.type_annotation.as_deref())?,
            }))
        }
        TSSignature::TSMethodSignature(method) => {
            let params = method
                .params
                .items
                .iter()
                .map(parse_parameter)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(TsInterfaceMember::Method(TsMethodDeclaration {
                name: property_key_name(&method.key)?,
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
fn parse_parameter(
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
fn parse_optional_type_annotation(
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
fn parse_type(ty: &TSType<'_>) -> Result<TsTypeRef, TsParseError> {
    match ty {
        TSType::TSStringKeyword(_) => Ok(TsTypeRef::Primitive(TsPrimitiveType::String)),
        TSType::TSNumberKeyword(_) => Ok(TsTypeRef::Primitive(TsPrimitiveType::Number)),
        TSType::TSBooleanKeyword(_) => Ok(TsTypeRef::Primitive(TsPrimitiveType::Boolean)),
        TSType::TSVoidKeyword(_) => Ok(TsTypeRef::Primitive(TsPrimitiveType::Void)),
        TSType::TSNullKeyword(_) => Ok(TsTypeRef::Null),
        TSType::TSUndefinedKeyword(_) => Ok(TsTypeRef::Undefined),
        TSType::TSAnyKeyword(_) => Ok(TsTypeRef::Any),
        TSType::TSUnknownKeyword(_) => Ok(TsTypeRef::Unknown),
        TSType::TSObjectKeyword(_) => Ok(TsTypeRef::Object),
        TSType::TSFunctionType(function) => {
            let params = function
                .params
                .items
                .iter()
                .map(parse_parameter)
                .map(|param| param.map(|param| param.ty))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(TsTypeRef::Callback {
                params,
                return_type: Box::new(parse_type(&function.return_type.type_annotation)?),
            })
        }
        TSType::TSArrayType(array) => {
            Ok(TsTypeRef::Array(Box::new(parse_type(&array.element_type)?)))
        }
        TSType::TSUnionType(union) => {
            let items = union
                .types
                .iter()
                .map(parse_type)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(TsTypeRef::Union(items))
        }
        TSType::TSLiteralType(literal) => parse_literal_type(&literal.literal),
        TSType::TSTypeReference(reference) => parse_type_reference(reference),
        TSType::TSParenthesizedType(parenthesized) => parse_type(&parenthesized.type_annotation),
        TSType::TSTypeLiteral(_) => parse_type_literal(ty),
        TSType::TSBigIntKeyword(_)
        | TSType::TSIntrinsicKeyword(_)
        | TSType::TSNeverKeyword(_)
        | TSType::TSSymbolKeyword(_)
        | TSType::TSConditionalType(_)
        | TSType::TSConstructorType(_)
        | TSType::TSImportType(_)
        | TSType::TSIndexedAccessType(_)
        | TSType::TSInferType(_)
        | TSType::TSIntersectionType(_)
        | TSType::TSMappedType(_)
        | TSType::TSNamedTupleMember(_)
        | TSType::TSTemplateLiteralType(_)
        | TSType::TSThisType(_)
        | TSType::TSTupleType(_)
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
    let args = type_arguments
        .params
        .iter()
        .map(parse_type)
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

/// Converts a TypeScript literal type into a neutral type reference.
///
/// Inputs:
/// - `literal`: Oxc TypeScript literal type node.
///
/// Output:
/// - `Ok(TsTypeRef)` for string, number, and boolean literals.
/// - `Err(TsParseError)` for bigint, template, or unary literals.
///
/// Transformation:
/// - Keeps literal unions representable without admitting computed literal
///   expressions.
fn parse_literal_type(literal: &TSLiteral<'_>) -> Result<TsTypeRef, TsParseError> {
    match literal {
        TSLiteral::BooleanLiteral(value) => Ok(TsTypeRef::BooleanLiteral(value.value)),
        TSLiteral::NumericLiteral(value) => Ok(TsTypeRef::NumberLiteral(
            value
                .raw
                .map_or_else(|| value.value.to_string(), |raw| raw.to_string()),
        )),
        TSLiteral::StringLiteral(value) => Ok(TsTypeRef::StringLiteral(value.value.to_string())),
        TSLiteral::BigIntLiteral(_)
        | TSLiteral::TemplateLiteral(_)
        | TSLiteral::UnaryExpression(_) => Err(unsupported(
            "ts_bindgen.unsupported_literal_type",
            "literal type is outside the first DOM binding slice",
        )),
    }
}

/// Returns the source name for a supported property key.
///
/// Inputs:
/// - `key`: Oxc property key from an interface member.
///
/// Output:
/// - `Ok(String)` for static identifier and string-literal keys.
/// - `Err(TsParseError)` for computed or private keys.
///
/// Transformation:
/// - Normalizes property keys before later snake/camel name conversion.
fn property_key_name(key: &PropertyKey<'_>) -> Result<String, TsParseError> {
    match key {
        PropertyKey::StaticIdentifier(identifier) => Ok(identifier.name.to_string()),
        PropertyKey::StringLiteral(literal) => Ok(literal.value.to_string()),
        _ => Err(unsupported(
            "ts_bindgen.unsupported_property_key",
            "only static identifier and string-literal property keys are supported",
        )),
    }
}

/// Returns the source name for a supported TypeScript type name.
///
/// Inputs:
/// - `name`: Oxc TypeScript type name.
///
/// Output:
/// - `Ok(String)` for identifier and qualified names.
/// - `Err(TsParseError)` for `this` type names.
///
/// Transformation:
/// - Preserves namespace qualification as dot-separated text for later wrapper
///   mapping.
fn type_name(name: &TSTypeName<'_>) -> Result<String, TsParseError> {
    match name {
        TSTypeName::IdentifierReference(identifier) => Ok(identifier.name.to_string()),
        TSTypeName::QualifiedName(qualified) => Ok(format!(
            "{}.{}",
            type_name(&qualified.left)?,
            qualified.right.name
        )),
        TSTypeName::ThisExpression(_) => Err(unsupported(
            "ts_bindgen.unsupported_this_type_name",
            "`this` type names are outside the first DOM binding slice",
        )),
    }
}

/// Builds a stable unsupported-shape parser error.
///
/// Inputs:
/// - `reason`: stable reason code.
/// - `message`: human-readable explanation.
///
/// Output:
/// - `TsParseError` carrying both fields.
///
/// Transformation:
/// - Centralizes generator-contract refusal messages so future manifests can
///   reuse the same reason codes.
fn unsupported(reason: &'static str, message: &str) -> TsParseError {
    TsParseError {
        reason,
        message: message.to_string(),
    }
}
