//! Deterministic schema exported to accelerator package adapter generators.

use serde::Serialize;

use super::{AcceleratorScalarType, AcceleratorValueError, ACCELERATOR_TENSOR_PACKET_SCHEMA};

/// Stable compiler value-contract report schema.
pub const ACCELERATOR_VALUE_CONTRACT_SCHEMA: &str = "terlan.accelerator-value-contract.v1";

/// Serializable scalar type entry consumed by adapter generators.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorScalarSchema {
    /// Stable descriptor spelling.
    pub id: &'static str,
    /// Canonical storage width in bytes.
    pub byte_width: u64,
    /// Minimum natural alignment in bytes.
    pub alignment: u64,
}

/// One legal linear-resource transition emitted for generated adapters.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorOwnershipTransition {
    /// Starting ownership state.
    pub from: &'static str,
    /// Requested transition.
    pub operation: &'static str,
    /// Resulting ownership state.
    pub to: &'static str,
    /// Handles invalidated by the transition.
    pub invalidates: &'static str,
}

/// One generated codec or declaration surface backed by canonical Rust types.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorGeneratedAdapter {
    /// Generated surface identity.
    pub id: &'static str,
    /// Generator mechanism rather than a handwritten package copy.
    pub generator: &'static str,
    /// Canonical compiler types covered by the adapter.
    pub types: Vec<&'static str>,
}

/// Complete compiler-owned accelerator value schema used by quality reports and generators.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorValueContract {
    /// Stable report schema.
    pub schema: &'static str,
    /// Current tensor packet schema.
    pub tensor_packet_schema: u64,
    /// Canonical scalar types.
    pub scalar_types: Vec<AcceleratorScalarSchema>,
    /// Supported tensor orders.
    pub tensor_orders: Vec<&'static str>,
    /// Pointer-free address-space forms.
    pub address_spaces: Vec<&'static str>,
    /// Linear resource classes.
    pub resource_classes: Vec<&'static str>,
    /// Legal ownership state transitions.
    pub ownership_transitions: Vec<AcceleratorOwnershipTransition>,
    /// Generated package-facing declaration and codec surfaces.
    pub generated_adapters: Vec<AcceleratorGeneratedAdapter>,
    /// Typed rejection classes enforced before package dispatch.
    pub rejection_evidence: Vec<&'static str>,
}

impl AcceleratorValueContract {
    /// Returns the deterministic canonical contract for the current compiler.
    pub fn canonical() -> Self {
        let dtypes = [
            AcceleratorScalarType::Bool,
            AcceleratorScalarType::U8,
            AcceleratorScalarType::I8,
            AcceleratorScalarType::U16,
            AcceleratorScalarType::I16,
            AcceleratorScalarType::U32,
            AcceleratorScalarType::I32,
            AcceleratorScalarType::U64,
            AcceleratorScalarType::I64,
            AcceleratorScalarType::F16,
            AcceleratorScalarType::Bf16,
            AcceleratorScalarType::F32,
            AcceleratorScalarType::F64,
        ];
        Self {
            schema: ACCELERATOR_VALUE_CONTRACT_SCHEMA,
            tensor_packet_schema: ACCELERATOR_TENSOR_PACKET_SCHEMA,
            scalar_types: dtypes
                .into_iter()
                .map(|dtype| AcceleratorScalarSchema {
                    id: dtype.identifier(),
                    byte_width: dtype.byte_width(),
                    alignment: dtype.alignment(),
                })
                .collect(),
            tensor_orders: vec!["row-major", "column-major", "strided"],
            address_spaces: vec!["host", "pinned-host", "device", "external"],
            resource_classes: vec![
                "device-context",
                "allocation",
                "stream",
                "event",
                "module",
                "kernel",
                "graph",
                "imported-tensor",
            ],
            ownership_transitions: vec![
                AcceleratorOwnershipTransition {
                    from: "owned",
                    operation: "borrow",
                    to: "owned-with-active-borrow",
                    invalidates: "none",
                },
                AcceleratorOwnershipTransition {
                    from: "owned-with-active-borrow",
                    operation: "release-borrow",
                    to: "owned",
                    invalidates: "borrowed-handle",
                },
                AcceleratorOwnershipTransition {
                    from: "owned",
                    operation: "transfer",
                    to: "owned-by-recipient",
                    invalidates: "prior-generation",
                },
                AcceleratorOwnershipTransition {
                    from: "owned",
                    operation: "dispose",
                    to: "disposed",
                    invalidates: "all-handles",
                },
            ],
            generated_adapters: vec![
                AcceleratorGeneratedAdapter {
                    id: "rust-serde-codec-v1",
                    generator: "serde-derive-from-canonical-types",
                    types: vec![
                        "AcceleratorScalarType",
                        "AcceleratorTensorLayout",
                        "AcceleratorAddressSpace",
                        "AcceleratorResourceHandle",
                        "AcceleratorTensorPacket",
                    ],
                },
                AcceleratorGeneratedAdapter {
                    id: "terlan-package-declarations-v1",
                    generator: "compiler-canonical-value-schema",
                    types: vec!["DType", "TensorLayout", "TensorPacket", "ResourceHandle"],
                },
            ],
            rejection_evidence: vec![
                "unsupported-scalar-type",
                "negative-dimension",
                "invalid-rank",
                "integer-overflow",
                "invalid-stride",
                "incompatible-layout",
                "invalid-alignment",
                "stale-handle",
                "escaped-borrow",
                "double-transfer",
                "double-disposal",
                "cross-device-alias",
                "byte-count-mismatch",
                "unsupported-packet-schema",
            ],
        }
    }

    /// Renders package declarations from the canonical schema.
    pub fn render_terlan_declarations(&self, module: &str) -> String {
        self.render_terlan_declarations_for(
            module,
            &self
                .scalar_types
                .iter()
                .map(|dtype| dtype.id.to_string())
                .collect::<Vec<_>>(),
        )
        .expect("canonical scalar schema must render")
    }

    /// Renders package declarations filtered by descriptor-admitted scalar types.
    pub fn render_terlan_declarations_for(
        &self,
        module: &str,
        supported_dtypes: &[String],
    ) -> Result<String, AcceleratorValueError> {
        let dtypes = self
            .scalar_types
            .iter()
            .filter(|dtype| {
                supported_dtypes
                    .iter()
                    .any(|supported| supported == dtype.id)
            })
            .map(|dtype| format!("Atom[\"{}\"]", dtype.id))
            .collect::<Vec<_>>()
            .join(" |\n    ");
        for dtype in supported_dtypes {
            AcceleratorScalarType::try_from(dtype.as_str())?;
        }
        Ok(format!(
            "/** Compiler-generated accelerator value declarations. */\nmodule {module}.\n\n/** Canonical scalar types admitted by accelerator packages. */\npub type DType =\n    {dtypes}.\n\n/** Canonical tensor layout exchanged across package boundaries. */\npub struct TensorLayout {{\n    dtype: DType,\n    dimensions: List[Int],\n    strides: List[Int],\n    byte_offset: Int,\n    alignment: Int,\n    byte_size: Int\n}}.\n\n/** Opaque linear resource handle owned by compiler-generated adapters. */\npub opaque type ResourceHandle.\n\n/** Versioned tensor exchange packet with no native address fields. */\npub struct TensorPacket {{\n    schema: Int,\n    layout: TensorLayout,\n    resource: Option[ResourceHandle]\n}}.\n"
        ))
    }

    /// Renders a dependency-free Rust scalar codec filtered by package metadata.
    pub fn render_rust_scalar_codec(
        &self,
        supported_dtypes: &[String],
    ) -> Result<String, AcceleratorValueError> {
        let dtypes = supported_dtypes
            .iter()
            .map(|dtype| AcceleratorScalarType::try_from(dtype.as_str()))
            .collect::<Result<Vec<_>, _>>()?;
        let variants = dtypes
            .iter()
            .map(|dtype| {
                format!(
                    "    /// Canonical `{}` scalar.\n    {},",
                    dtype.identifier(),
                    rust_variant(*dtype)
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let parse_arms = dtypes
            .iter()
            .map(|dtype| {
                format!(
                    "            \"{}\" => Some(Self::{}),",
                    dtype.identifier(),
                    rust_variant(*dtype)
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let name_arms = dtypes
            .iter()
            .map(|dtype| {
                format!(
                    "            Self::{} => \"{}\",",
                    rust_variant(*dtype),
                    dtype.identifier()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let width_arms = dtypes
            .iter()
            .map(|dtype| {
                format!(
                    "            Self::{} => {},",
                    rust_variant(*dtype),
                    dtype.byte_width()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let dlpack_arms = dtypes
            .iter()
            .map(|dtype| {
                let (code, bits) = dlpack(*dtype);
                format!(
                    "            Self::{} => ({code}, {bits}),",
                    rust_variant(*dtype)
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let dlpack_parse_arms = dtypes
            .iter()
            .map(|dtype| {
                let (code, bits) = dlpack(*dtype);
                format!(
                    "            ({code}, {bits}) => Some(Self::{}),",
                    rust_variant(*dtype)
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(format!(
            "//! Compiler-generated scalar, shape, packet, and resource codecs. Do not edit by hand.\n\n/// Scalar storage types admitted by the package accelerator descriptor.\n#[derive(Clone, Copy, Debug, Eq, PartialEq)]\npub enum DType {{\n{variants}\n}}\n\nimpl DType {{\n    /// Parses one stable descriptor spelling.\n    pub fn parse(value: &str) -> Option<Self> {{\n        match value {{\n{parse_arms}\n            _ => None,\n        }}\n    }}\n\n    /// Parses one standard DLPack scalar code and bit width.\n    pub const fn from_dlpack(code: u8, bits: u8) -> Option<Self> {{\n        match (code, bits) {{\n{dlpack_parse_arms}\n            _ => None,\n        }}\n    }}\n\n    /// Returns the stable descriptor spelling.\n    pub const fn name(self) -> &'static str {{\n        match self {{\n{name_arms}\n        }}\n    }}\n\n    /// Returns the canonical storage width in bytes.\n    pub const fn byte_width(self) -> usize {{\n        match self {{\n{width_arms}\n        }}\n    }}\n\n    /// Returns the DLPack scalar code and bit width.\n    pub const fn dlpack(self) -> (u8, u8) {{\n        match self {{\n{dlpack_arms}\n        }}\n    }}\n}}\n{RUST_SHAPE_CODEC}{RUST_PACKET_CODEC}{RUST_RESOURCE_CODEC}"
        ))
    }
}

#[path = "schema/resource_codec.rs"]
mod resource_codec;
use resource_codec::RUST_RESOURCE_CODEC;

/// Dependency-free checked shape implementation emitted into native package adapters.
const RUST_SHAPE_CODEC: &str = r#"
/// Stable shape rejection translated by the package adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShapeError {
    /// Rank exceeds the canonical compiler limit.
    Rank,
    /// A dimension is negative or cannot fit the host index width.
    Dimension,
    /// Dimension multiplication overflowed.
    ElementOverflow,
    /// The supplied payload length differs from the shape product.
    LengthMismatch,
    /// Element count multiplied by scalar width overflowed.
    ByteOverflow,
    /// Canonical row-major stride construction overflowed.
    StrideOverflow,
}

/// Validated contiguous row-major multidimensional shape.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Shape {
    /// Dimensions in logical order.
    dimensions: Vec<usize>,
    /// Checked flattened element count.
    elements: usize,
    /// Checked contiguous row-major element strides.
    strides: Vec<usize>,
}

impl Shape {
    /// Checks rank, dimensions, element count, byte size, and row-major strides.
    pub fn checked(
        dimensions: &[i64],
        expected_elements: usize,
        element_width: usize,
    ) -> Result<Self, ShapeError> {
        if dimensions.len() > 32 {
            return Err(ShapeError::Rank);
        }
        let dimensions = dimensions
            .iter()
            .map(|dimension| usize::try_from(*dimension).map_err(|_| ShapeError::Dimension))
            .collect::<Result<Vec<_>, _>>()?;
        let elements = dimensions.iter().try_fold(1usize, |count, dimension| {
            count
                .checked_mul(*dimension)
                .ok_or(ShapeError::ElementOverflow)
        })?;
        if elements != expected_elements {
            return Err(ShapeError::LengthMismatch);
        }
        elements
            .checked_mul(element_width)
            .ok_or(ShapeError::ByteOverflow)?;
        let mut strides = vec![1usize; dimensions.len()];
        let mut stride = 1usize;
        for (index, dimension) in dimensions.iter().enumerate().rev() {
            strides[index] = stride;
            stride = stride
                .checked_mul(*dimension)
                .ok_or(ShapeError::StrideOverflow)?;
        }
        Ok(Self {
            dimensions,
            elements,
            strides,
        })
    }

    /// Returns dimensions in logical order.
    pub fn dimensions(&self) -> &[usize] {
        &self.dimensions
    }

    /// Returns the checked flattened element count.
    pub const fn elements(&self) -> usize {
        self.elements
    }

    /// Returns the checked rank.
    pub fn rank(&self) -> usize {
        self.dimensions.len()
    }

    /// Returns canonical row-major element strides.
    pub fn strides(&self) -> &[usize] {
        &self.strides
    }
}
"#;

/// Dependency-free copied-host tensor packet codec emitted into native adapters.
const RUST_PACKET_CODEC: &str = r#"
/// Maximum copied tensor payload admitted by the canonical packet codec.
pub const MAX_TENSOR_PACKET_BYTES: usize = 16_777_216;

/// Stable packet rejection translated by the package adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TensorPacketError {
    /// Header magic, schema, endian, lane, or device metadata is invalid.
    InvalidHeader,
    /// DLPack scalar metadata is not admitted by the package descriptor.
    UnsupportedDType,
    /// Metadata size arithmetic overflowed.
    MetadataOverflow,
    /// Shape metadata was rejected by the canonical shape codec.
    Shape(ShapeError),
    /// Packet payload size differs from the checked backing storage span.
    ByteCount,
    /// Explicit strides are negative, have the wrong rank, or overflow their storage span.
    Layout,
    /// Copied payload exceeds the canonical packet limit.
    TooLarge,
}

/// Decoded copied-host tensor metadata with validated layout and data offset.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TensorPacketMetadata {
    /// Canonical scalar type.
    pub dtype: DType,
    /// Checked logical tensor shape.
    pub shape: Shape,
    /// Non-negative element strides, including zero-stride broadcast dimensions.
    pub strides: Vec<usize>,
    /// First payload byte in the packet.
    pub data_offset: usize,
}

impl TensorPacketMetadata {
    /// Encodes a canonical header, dimensions, and strides for one copied payload.
    pub fn encode_prefix(
        dtype: DType,
        shape: &Shape,
        payload_bytes: usize,
    ) -> Result<Vec<u8>, TensorPacketError> {
        Self::encode_view_prefix(dtype, shape, shape.strides(), payload_bytes)
    }

    /// Encodes dimensions and explicit non-negative element strides for one copied payload.
    pub fn encode_view_prefix(
        dtype: DType,
        shape: &Shape,
        strides: &[usize],
        payload_bytes: usize,
    ) -> Result<Vec<u8>, TensorPacketError> {
        if payload_bytes > MAX_TENSOR_PACKET_BYTES {
            return Err(TensorPacketError::TooLarge);
        }
        let expected = storage_span_bytes(shape.dimensions(), strides, dtype.byte_width())?;
        if payload_bytes != expected {
            return Err(TensorPacketError::ByteCount);
        }
        let rank = shape.rank();
        if strides.len() != rank {
            return Err(TensorPacketError::Layout);
        }
        let metadata_bytes = 16usize
            .checked_add(
                rank.checked_mul(16)
                    .ok_or(TensorPacketError::MetadataOverflow)?,
            )
            .ok_or(TensorPacketError::MetadataOverflow)?;
        let mut packet = vec![0u8; metadata_bytes + payload_bytes];
        let (dtype_code, dtype_bits) = dtype.dlpack();
        packet[..4].copy_from_slice(b"TNXP");
        packet[4] = 1;
        packet[5] = native_endian_code();
        packet[6] = dtype_code;
        packet[7] = dtype_bits;
        packet[8..10].copy_from_slice(&1u16.to_le_bytes());
        packet[10] = 1;
        packet[11] = 0;
        packet[12..16].copy_from_slice(&(rank as u32).to_le_bytes());
        for (index, dimension) in shape.dimensions().iter().enumerate() {
            let value =
                i64::try_from(*dimension).map_err(|_| TensorPacketError::MetadataOverflow)?;
            let offset = 16 + index * 8;
            packet[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
        }
        for (index, stride) in strides.iter().enumerate() {
            let value = i64::try_from(*stride).map_err(|_| TensorPacketError::MetadataOverflow)?;
            let offset = 16 + rank * 8 + index * 8;
            packet[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
        }
        Ok(packet)
    }

    /// Decodes and validates one copied-host tensor packet before package dispatch.
    pub fn decode(bytes: &[u8]) -> Result<Self, TensorPacketError> {
        if bytes.len() < 16
            || &bytes[..4] != b"TNXP"
            || bytes[4] != 1
            || bytes[5] != native_endian_code()
            || bytes[8..10] != 1u16.to_le_bytes()
            || bytes[10] != 1
            || bytes[11] != 0
        {
            return Err(TensorPacketError::InvalidHeader);
        }
        let dtype =
            DType::from_dlpack(bytes[6], bytes[7]).ok_or(TensorPacketError::UnsupportedDType)?;
        let rank = u32::from_le_bytes(
            bytes[12..16]
                .try_into()
                .map_err(|_| TensorPacketError::InvalidHeader)?,
        ) as usize;
        if rank > 32 {
            return Err(TensorPacketError::Shape(ShapeError::Rank));
        }
        let metadata_bytes = 16usize
            .checked_add(
                rank.checked_mul(16)
                    .ok_or(TensorPacketError::MetadataOverflow)?,
            )
            .ok_or(TensorPacketError::MetadataOverflow)?;
        if bytes.len() < metadata_bytes {
            return Err(TensorPacketError::InvalidHeader);
        }
        let dimensions = (0..rank)
            .map(|index| {
                let offset = 16 + index * 8;
                bytes[offset..offset + 8]
                    .try_into()
                    .map(i64::from_le_bytes)
                    .map_err(|_| TensorPacketError::InvalidHeader)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let strides = (0..rank)
            .map(|index| {
                let offset = 16 + rank * 8 + index * 8;
                let value = bytes[offset..offset + 8]
                    .try_into()
                    .map(i64::from_le_bytes)
                    .map_err(|_| TensorPacketError::InvalidHeader)?;
                usize::try_from(value).map_err(|_| TensorPacketError::Layout)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let payload_bytes = bytes.len() - metadata_bytes;
        if payload_bytes > MAX_TENSOR_PACKET_BYTES {
            return Err(TensorPacketError::TooLarge);
        }
        if payload_bytes % dtype.byte_width() != 0 {
            return Err(TensorPacketError::ByteCount);
        }
        let logical_elements = dimensions.iter().try_fold(1usize, |count, dimension| {
            let dimension = usize::try_from(*dimension)
                .map_err(|_| TensorPacketError::Shape(ShapeError::Dimension))?;
            count
                .checked_mul(dimension)
                .ok_or(TensorPacketError::Shape(ShapeError::ElementOverflow))
        })?;
        let shape = Shape::checked(&dimensions, logical_elements, dtype.byte_width())
            .map_err(TensorPacketError::Shape)?;
        let expected = storage_span_bytes(shape.dimensions(), &strides, dtype.byte_width())?;
        if payload_bytes != expected {
            return Err(TensorPacketError::ByteCount);
        }
        Ok(Self {
            dtype,
            shape,
            strides,
            data_offset: metadata_bytes,
        })
    }

    /// Returns whether this packet reuses a backing element along a non-singleton dimension.
    pub fn is_broadcast_view(&self) -> bool {
        self.shape
            .dimensions()
            .iter()
            .zip(&self.strides)
            .any(|(dimension, stride)| *dimension > 1 && *stride == 0)
    }
}

/// Computes the backing byte span touched by a non-negative strided tensor.
fn storage_span_bytes(
    dimensions: &[usize],
    strides: &[usize],
    element_width: usize,
) -> Result<usize, TensorPacketError> {
    if dimensions.len() != strides.len() {
        return Err(TensorPacketError::Layout);
    }
    if dimensions.contains(&0) {
        return Ok(0);
    }
    let last_element = dimensions.iter().zip(strides).try_fold(
        0usize,
        |offset, (dimension, stride)| {
            dimension
                .checked_sub(1)
                .and_then(|extent| extent.checked_mul(*stride))
                .and_then(|extent| offset.checked_add(extent))
                .ok_or(TensorPacketError::MetadataOverflow)
        },
    )?;
    last_element
        .checked_add(1)
        .and_then(|elements| elements.checked_mul(element_width))
        .ok_or(TensorPacketError::MetadataOverflow)
}

/// Returns the native-endian marker retained by copied packet schema one.
const fn native_endian_code() -> u8 {
    if cfg!(target_endian = "little") {
        1
    } else {
        2
    }
}
"#;

/// Returns the generated Rust variant spelling for a canonical scalar.
fn rust_variant(dtype: AcceleratorScalarType) -> &'static str {
    match dtype {
        AcceleratorScalarType::Bool => "Bool",
        AcceleratorScalarType::U8 => "U8",
        AcceleratorScalarType::I8 => "I8",
        AcceleratorScalarType::U16 => "U16",
        AcceleratorScalarType::I16 => "I16",
        AcceleratorScalarType::U32 => "U32",
        AcceleratorScalarType::I32 => "I32",
        AcceleratorScalarType::U64 => "U64",
        AcceleratorScalarType::I64 => "I64",
        AcceleratorScalarType::F16 => "F16",
        AcceleratorScalarType::Bf16 => "Bf16",
        AcceleratorScalarType::F32 => "F32",
        AcceleratorScalarType::F64 => "F64",
    }
}

/// Returns the standard DLPack code and width for one canonical scalar.
fn dlpack(dtype: AcceleratorScalarType) -> (u8, u8) {
    match dtype {
        AcceleratorScalarType::I8 => (0, 8),
        AcceleratorScalarType::I16 => (0, 16),
        AcceleratorScalarType::I32 => (0, 32),
        AcceleratorScalarType::I64 => (0, 64),
        AcceleratorScalarType::U8 => (1, 8),
        AcceleratorScalarType::U16 => (1, 16),
        AcceleratorScalarType::U32 => (1, 32),
        AcceleratorScalarType::U64 => (1, 64),
        AcceleratorScalarType::F16 => (2, 16),
        AcceleratorScalarType::F32 => (2, 32),
        AcceleratorScalarType::F64 => (2, 64),
        AcceleratorScalarType::Bf16 => (4, 16),
        AcceleratorScalarType::Bool => (6, 8),
    }
}
