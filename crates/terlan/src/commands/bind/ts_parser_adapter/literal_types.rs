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
