//! Typed template rendering operations for generated native code.

use super::super::{
    ActorHeap, ManagedAggregate, ManagedFieldType, ManagedFieldValue, ManagedLayoutRegistry,
    ManagedList, ManagedMemoryError, ManagedString, SemanticTypeId,
};

const MAGIC: &[u8; 4] = b"TVMT";
const VERSION: u16 = 1;
const HEADER_BYTES: usize = 10;
const TEXT_CONTEXT: u8 = 0;
const ATTRIBUTE_CONTEXT: u8 = 1;

/// Closed checked value families accepted by template rendering operations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagedTemplateValueKind {
    /// Managed UTF-8 string.
    String,
    /// Native signed integer word.
    Int,
    /// Native IEEE-754 bit pattern.
    Float,
    /// Native zero-or-one boolean word.
    Bool,
    /// Managed persistent list containing managed UTF-8 strings.
    StringList,
    /// Managed `Option[String]` constructor.
    OptionalString,
    /// Managed `Option[Int]` constructor.
    OptionalInt,
    /// Managed `Option[Float]` constructor.
    OptionalFloat,
    /// Managed `Option[Bool]` constructor.
    OptionalBool,
    /// Managed `Option[List[String]]` constructor.
    OptionalStringList,
}

impl ManagedTemplateValueKind {
    /// Returns the stable operation-ABI discriminant.
    fn tag(self) -> u8 {
        match self {
            Self::String => 1,
            Self::Int => 2,
            Self::Float => 3,
            Self::Bool => 4,
            Self::StringList => 5,
            Self::OptionalString => 6,
            Self::OptionalInt => 7,
            Self::OptionalFloat => 8,
            Self::OptionalBool => 9,
            Self::OptionalStringList => 10,
        }
    }

    /// Decodes one stable operation-ABI discriminant.
    fn from_tag(tag: u8) -> Option<Self> {
        Some(match tag {
            1 => Self::String,
            2 => Self::Int,
            3 => Self::Float,
            4 => Self::Bool,
            5 => Self::StringList,
            6 => Self::OptionalString,
            7 => Self::OptionalInt,
            8 => Self::OptionalFloat,
            9 => Self::OptionalBool,
            10 => Self::OptionalStringList,
            _ => return None,
        })
    }

    /// Returns the non-optional family wrapped by an optional kind.
    fn optional_inner(self) -> Option<Self> {
        Some(match self {
            Self::OptionalString => Self::String,
            Self::OptionalInt => Self::Int,
            Self::OptionalFloat => Self::Float,
            Self::OptionalBool => Self::Bool,
            Self::OptionalStringList => Self::StringList,
            _ => return None,
        })
    }
}

/// Encodes one checked text or whole-attribute rendering operation.
///
/// Inputs:
/// - `kind`: exact checked value representation.
/// - `attribute`: absent for escaped text or present for a whole HTML
///   attribute governed by maintained attribute semantics.
///
/// Output:
/// - Immutable bounded operation bytes embedded in a native image.
///
/// Transformation:
/// - Encodes context, value family, and UTF-8 attribute identity without
///   storing template source or runtime parser state.
pub fn encode_template_render_operation(
    kind: ManagedTemplateValueKind,
    attribute: Option<&str>,
) -> Result<Vec<u8>, ManagedMemoryError> {
    let attribute = attribute.unwrap_or("").as_bytes();
    let length =
        u16::try_from(attribute.len()).map_err(|_| ManagedMemoryError::InvalidAggregateAbi)?;
    let mut encoded = Vec::with_capacity(HEADER_BYTES + attribute.len());
    encoded.extend_from_slice(MAGIC);
    encoded.extend_from_slice(&VERSION.to_le_bytes());
    encoded.push(if attribute.is_empty() {
        TEXT_CONTEXT
    } else {
        ATTRIBUTE_CONTEXT
    });
    encoded.push(kind.tag());
    encoded.extend_from_slice(&length.to_le_bytes());
    encoded.extend_from_slice(attribute);
    Ok(encoded)
}

/// Reports whether bytes identify the typed template operation family.
pub(super) fn is_template_operation(encoded: &[u8]) -> bool {
    encoded.starts_with(MAGIC)
}

/// Executes one typed rendering operation against the current actor heap.
pub(super) fn execute_template_operation(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    encoded: &[u8],
    words: &[i64],
) -> Result<u64, ManagedMemoryError> {
    let [word] = words else {
        return Err(ManagedMemoryError::InvalidAggregateArity);
    };
    let operation = decode_operation(encoded)?;
    let value = render_value(heap, layouts, operation.kind, *word)?;
    let rendered = match operation.attribute {
        None => render_text(value)?,
        Some(attribute) => render_attribute(&attribute, value)?,
    };
    heap.allocate_string(&rendered)
        .map(|value| value.erase().encoded_abi_word())
}

/// Decoded template rendering operation.
struct TemplateRenderOperation {
    kind: ManagedTemplateValueKind,
    attribute: Option<String>,
}

/// Decodes and validates one exact rendering operation payload.
fn decode_operation(encoded: &[u8]) -> Result<TemplateRenderOperation, ManagedMemoryError> {
    if encoded.len() < HEADER_BYTES
        || encoded.get(..4) != Some(MAGIC)
        || encoded.get(4..6) != Some(&VERSION.to_le_bytes())
    {
        return Err(ManagedMemoryError::InvalidAggregateAbi);
    }
    let context = encoded[6];
    let kind = ManagedTemplateValueKind::from_tag(encoded[7])
        .ok_or(ManagedMemoryError::InvalidAggregateAbi)?;
    let length = encoded
        .get(8..10)
        .and_then(|bytes| <[u8; 2]>::try_from(bytes).ok())
        .map(u16::from_le_bytes)
        .map(usize::from)
        .ok_or(ManagedMemoryError::InvalidAggregateAbi)?;
    if encoded.len() != HEADER_BYTES + length {
        return Err(ManagedMemoryError::InvalidAggregateAbi);
    }
    let attribute = std::str::from_utf8(&encoded[HEADER_BYTES..])
        .map_err(|_| ManagedMemoryError::InvalidUtf8)?;
    match (context, attribute.is_empty()) {
        (TEXT_CONTEXT, true) => Ok(TemplateRenderOperation {
            kind,
            attribute: None,
        }),
        (ATTRIBUTE_CONTEXT, false) => Ok(TemplateRenderOperation {
            kind,
            attribute: Some(attribute.to_string()),
        }),
        _ => Err(ManagedMemoryError::InvalidAggregateAbi),
    }
}

/// Runtime value normalized for maintained HTML rendering semantics.
enum TemplateRenderValue {
    Scalar(String),
    Boolean(bool),
    Tokens(Vec<String>),
    Missing,
}

/// Decodes one checked native or managed value family.
fn render_value(
    heap: &ActorHeap,
    layouts: &ManagedLayoutRegistry,
    kind: ManagedTemplateValueKind,
    word: i64,
) -> Result<TemplateRenderValue, ManagedMemoryError> {
    if let Some(inner) = kind.optional_inner() {
        return render_optional_value(heap, layouts, inner, word);
    }
    match kind {
        ManagedTemplateValueKind::String => {
            let value = super::reference_word(word)?.cast::<ManagedString>();
            Ok(TemplateRenderValue::Scalar(
                heap.read_string(value)?.to_string(),
            ))
        }
        ManagedTemplateValueKind::Int => Ok(TemplateRenderValue::Scalar(word.to_string())),
        ManagedTemplateValueKind::Float => Ok(TemplateRenderValue::Scalar(
            f64::from_bits(u64::from_ne_bytes(word.to_ne_bytes())).to_string(),
        )),
        ManagedTemplateValueKind::Bool => match word {
            0 => Ok(TemplateRenderValue::Boolean(false)),
            1 => Ok(TemplateRenderValue::Boolean(true)),
            _ => Err(ManagedMemoryError::InvalidManagedScalar),
        },
        ManagedTemplateValueKind::StringList => Ok(TemplateRenderValue::Tokens(read_string_list(
            heap, layouts, word,
        )?)),
        ManagedTemplateValueKind::OptionalString
        | ManagedTemplateValueKind::OptionalInt
        | ManagedTemplateValueKind::OptionalFloat
        | ManagedTemplateValueKind::OptionalBool
        | ManagedTemplateValueKind::OptionalStringList => unreachable!("optional handled above"),
    }
}

/// Opens one admitted option and renders its active payload or omission.
fn render_optional_value(
    heap: &ActorHeap,
    layouts: &ManagedLayoutRegistry,
    inner: ManagedTemplateValueKind,
    word: i64,
) -> Result<TemplateRenderValue, ManagedMemoryError> {
    let reference = super::reference_word(word)?;
    let semantic = heap.descriptor(reference)?.semantic_id();
    let layout = layouts
        .layout_for_reference(heap, semantic, reference)
        .map_err(|_| ManagedMemoryError::ManagedTypeMismatch)?;
    match layout.variant_name() {
        Some("None") if layout.fields().is_empty() => Ok(TemplateRenderValue::Missing),
        Some("Some") if layout.fields().len() == 1 => {
            let field = heap
                .read_aggregate(reference.cast::<ManagedAggregate>(), layout)?
                .field(0)?;
            let word = i64::from_ne_bytes(super::field_word(field).to_ne_bytes());
            render_value(heap, layouts, inner, word)
        }
        _ => Err(ManagedMemoryError::ManagedTypeMismatch),
    }
}

/// Reads one checked managed `List[String]` into owned token values.
fn read_string_list(
    heap: &ActorHeap,
    layouts: &ManagedLayoutRegistry,
    word: i64,
) -> Result<Vec<String>, ManagedMemoryError> {
    let list = super::reference_word(word)?.cast::<ManagedList>();
    let semantic = heap.descriptor(list)?.semantic_id();
    let descriptor = layouts
        .collection(semantic)
        .and_then(|collection| collection.list_descriptor())
        .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
    let expected = ManagedFieldType::Reference(SemanticTypeId::from_canonical("std.core.String")?);
    if descriptor.element_type() != expected {
        return Err(ManagedMemoryError::ManagedTypeMismatch);
    }
    heap.list_elements(descriptor, list)?
        .into_iter()
        .map(|element| match element {
            ManagedFieldValue::Reference(reference) => heap
                .read_string(reference.cast::<ManagedString>())
                .map(str::to_string),
            _ => Err(ManagedMemoryError::InvalidAggregateField),
        })
        .collect()
}

/// Escapes one scalar value for HTML text context.
fn render_text(value: TemplateRenderValue) -> Result<String, ManagedMemoryError> {
    match value {
        TemplateRenderValue::Scalar(value) => Ok(crate::terlan_html::escape_html_text(&value)),
        TemplateRenderValue::Boolean(value) => {
            Ok(crate::terlan_html::escape_html_text(&value.to_string()))
        }
        TemplateRenderValue::Tokens(_) | TemplateRenderValue::Missing => {
            Err(ManagedMemoryError::InvalidManagedOperation)
        }
    }
}

/// Applies maintained whole-attribute rendering and omission semantics.
fn render_attribute(
    attribute: &str,
    value: TemplateRenderValue,
) -> Result<String, ManagedMemoryError> {
    let kind = crate::terlan_html::template_attribute_slot_kind(attribute);
    let value = match value {
        TemplateRenderValue::Scalar(value) => {
            crate::terlan_html::TemplateAttributeValue::Scalar(value)
        }
        TemplateRenderValue::Boolean(value)
            if kind == crate::terlan_html::TemplateAttributeSlotKind::Boolean =>
        {
            crate::terlan_html::TemplateAttributeValue::Boolean(value)
        }
        TemplateRenderValue::Boolean(value) => {
            crate::terlan_html::TemplateAttributeValue::Scalar(value.to_string())
        }
        TemplateRenderValue::Tokens(value) => {
            crate::terlan_html::TemplateAttributeValue::Tokens(value)
        }
        TemplateRenderValue::Missing => crate::terlan_html::TemplateAttributeValue::Missing,
    };
    crate::terlan_html::render_template_attribute(attribute, value)
        .map(|value| value.map(|value| format!(" {value}")).unwrap_or_default())
        .map_err(|_| ManagedMemoryError::InvalidManagedOperation)
}
