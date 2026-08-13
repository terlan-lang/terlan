use super::*;

/// Closed operation payload decoded from immutable image data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ManagedOperation<'a> {
    /// Projects one physical field from an aggregate with the expected identity.
    Project {
        semantic: SemanticTypeId,
        field: usize,
    },
    /// Projects one scalar physical field from an aggregate with the expected identity.
    ProjectScalar {
        semantic: SemanticTypeId,
        field: usize,
    },
    /// Looks up one managed string key and allocates `None` or `Some(value)`.
    StringMapGetOption {
        map_semantic: SemanticTypeId,
        option_semantic: SemanticTypeId,
    },
    /// Allocates an empty persistent list with the admitted collection schema.
    ListEmpty { list_semantic: SemanticTypeId },
    /// Rebuilds an immutable aggregate with one physical field replaced.
    AggregateReplaceField {
        semantic: SemanticTypeId,
        field: usize,
    },
    /// Appends a two-field value to one persistent list field and rebuilds its owner.
    AggregateAppendPair {
        aggregate_semantic: SemanticTypeId,
        list_semantic: SemanticTypeId,
        pair_semantic: SemanticTypeId,
        field: usize,
    },
    /// Appends one checked value to an aggregate-owned persistent list.
    AggregateAppendValue {
        aggregate_semantic: SemanticTypeId,
        list_semantic: SemanticTypeId,
        field: usize,
    },
    /// Compares two validated managed strings by UTF-8 value.
    StringEqual,
    /// Allocates the UTF-8 concatenation of two validated managed strings.
    StringAppend,
    /// Allocates the UTF-8 concatenation of two or more validated managed strings.
    StringConcat,
    /// Prepends one image-owned UTF-8 literal to a validated managed string.
    StringPrependLiteral(&'a str),
    /// Projects a managed string field and prepends one image-owned literal.
    StringPrependProjectedLiteral {
        semantic: SemanticTypeId,
        field: usize,
        literal: &'a str,
    },
    /// Allocates the UTF-8 concatenation of a checked managed string list.
    StringListJoin,
    /// Escapes one checked string for HTML text context.
    StringEscapeHtmlText,
    /// Escapes one checked string for a quoted HTML attribute context.
    StringEscapeHtmlAttribute,
    /// Measures only bytes owned directly by one managed object.
    MemoryShallowSize,
    /// Measures all distinct managed bytes reachable from one object.
    MemoryRetainedSize,
}

/// Builds the common immutable operation header.
pub(super) fn header(operation: u8) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(MAP_GET_BYTES);
    encoded.extend_from_slice(MAGIC);
    encoded.extend_from_slice(&VERSION.to_le_bytes());
    encoded.push(operation);
    encoded.push(0);
    encoded
}

/// Decodes one exact operation payload and rejects extensions or truncation.
pub(super) fn decode_operation(encoded: &[u8]) -> Result<ManagedOperation<'_>, ManagedMemoryError> {
    if encoded.len() < HEADER_BYTES
        || encoded.get(..4) != Some(MAGIC)
        || encoded.get(4..6) != Some(&VERSION.to_le_bytes())
        || encoded[7] != 0
    {
        return Err(ManagedMemoryError::InvalidAggregateAbi);
    }
    match encoded[6] {
        PROJECT if encoded.len() == PROJECT_BYTES => {
            let semantic = semantic_at(encoded, HEADER_BYTES)?;
            let field = encoded
                .get(HEADER_BYTES + SEMANTIC_BYTES..PROJECT_BYTES)
                .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
                .map(u32::from_le_bytes)
                .and_then(|value| usize::try_from(value).ok())
                .ok_or(ManagedMemoryError::InvalidAggregateAbi)?;
            Ok(ManagedOperation::Project { semantic, field })
        }
        PROJECT_SCALAR if encoded.len() == PROJECT_BYTES => {
            let semantic = semantic_at(encoded, HEADER_BYTES)?;
            let field = field_at(encoded, HEADER_BYTES + SEMANTIC_BYTES)?;
            Ok(ManagedOperation::ProjectScalar { semantic, field })
        }
        STRING_MAP_GET_OPTION if encoded.len() == MAP_GET_BYTES => {
            Ok(ManagedOperation::StringMapGetOption {
                map_semantic: semantic_at(encoded, HEADER_BYTES)?,
                option_semantic: semantic_at(encoded, HEADER_BYTES + SEMANTIC_BYTES)?,
            })
        }
        LIST_EMPTY if encoded.len() == LIST_EMPTY_BYTES => Ok(ManagedOperation::ListEmpty {
            list_semantic: semantic_at(encoded, HEADER_BYTES)?,
        }),
        AGGREGATE_REPLACE_FIELD if encoded.len() == REPLACE_FIELD_BYTES => {
            Ok(ManagedOperation::AggregateReplaceField {
                semantic: semantic_at(encoded, HEADER_BYTES)?,
                field: field_at(encoded, HEADER_BYTES + SEMANTIC_BYTES)?,
            })
        }
        AGGREGATE_APPEND_PAIR if encoded.len() == APPEND_PAIR_BYTES => {
            Ok(ManagedOperation::AggregateAppendPair {
                aggregate_semantic: semantic_at(encoded, HEADER_BYTES)?,
                list_semantic: semantic_at(encoded, HEADER_BYTES + SEMANTIC_BYTES)?,
                pair_semantic: semantic_at(encoded, HEADER_BYTES + SEMANTIC_BYTES * 2)?,
                field: field_at(encoded, HEADER_BYTES + SEMANTIC_BYTES * 3)?,
            })
        }
        AGGREGATE_APPEND_VALUE if encoded.len() == APPEND_VALUE_BYTES => {
            Ok(ManagedOperation::AggregateAppendValue {
                aggregate_semantic: semantic_at(encoded, HEADER_BYTES)?,
                list_semantic: semantic_at(encoded, HEADER_BYTES + SEMANTIC_BYTES)?,
                field: field_at(encoded, HEADER_BYTES + SEMANTIC_BYTES * 2)?,
            })
        }
        STRING_EQUAL if encoded.len() == HEADER_BYTES => Ok(ManagedOperation::StringEqual),
        STRING_APPEND if encoded.len() == HEADER_BYTES => Ok(ManagedOperation::StringAppend),
        STRING_CONCAT if encoded.len() == HEADER_BYTES => Ok(ManagedOperation::StringConcat),
        STRING_PREPEND_LITERAL if encoded.len() >= HEADER_BYTES + 4 => Ok(
            ManagedOperation::StringPrependLiteral(literal_at(encoded, HEADER_BYTES)?),
        ),
        STRING_PREPEND_PROJECTED_LITERAL if encoded.len() >= HEADER_BYTES + SEMANTIC_BYTES + 8 => {
            let semantic = semantic_at(encoded, HEADER_BYTES)?;
            let field = field_at(encoded, HEADER_BYTES + SEMANTIC_BYTES)?;
            let literal = literal_at(encoded, HEADER_BYTES + SEMANTIC_BYTES + 4)?;
            Ok(ManagedOperation::StringPrependProjectedLiteral {
                semantic,
                field,
                literal,
            })
        }
        STRING_LIST_JOIN if encoded.len() == HEADER_BYTES => Ok(ManagedOperation::StringListJoin),
        STRING_ESCAPE_HTML_TEXT if encoded.len() == HEADER_BYTES => {
            Ok(ManagedOperation::StringEscapeHtmlText)
        }
        STRING_ESCAPE_HTML_ATTRIBUTE if encoded.len() == HEADER_BYTES => {
            Ok(ManagedOperation::StringEscapeHtmlAttribute)
        }
        MEMORY_SHALLOW_SIZE if encoded.len() == HEADER_BYTES => {
            Ok(ManagedOperation::MemoryShallowSize)
        }
        MEMORY_RETAINED_SIZE if encoded.len() == HEADER_BYTES => {
            Ok(ManagedOperation::MemoryRetainedSize)
        }
        _ => Err(ManagedMemoryError::InvalidAggregateAbi),
    }
}
