//! Checked tensor shape and layout metadata.

use serde::{Deserialize, Serialize};

use super::{AcceleratorScalarType, AcceleratorValueError};

/// Maximum tensor rank admitted at compiler and package boundaries.
pub const MAX_ACCELERATOR_TENSOR_RANK: usize = 32;

/// Logical tensor order declared independently of a backend implementation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AcceleratorTensorOrder {
    /// Last dimension varies fastest.
    RowMajor,
    /// First dimension varies fastest.
    ColumnMajor,
    /// Explicit non-negative element strides describe a non-contiguous or broadcast view.
    Strided,
}

/// Fully checked tensor dimensions, element strides, offset, and size.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorTensorLayout {
    /// Scalar representation used to calculate bytes and alignment.
    pub dtype: AcceleratorScalarType,
    /// Tensor dimensions in logical order.
    pub dimensions: Vec<u64>,
    /// Non-negative strides measured in elements.
    pub strides: Vec<u64>,
    /// Byte offset from the start of the backing allocation.
    pub byte_offset: u64,
    /// Declared contiguous or explicitly strided order.
    pub order: AcceleratorTensorOrder,
    /// Required byte alignment for the backing allocation and offset.
    pub alignment: u64,
    /// Checked product of all dimensions, with a scalar represented by rank zero.
    pub element_count: u64,
    /// Checked logical payload size in bytes.
    pub byte_size: u64,
    /// Checked backing span required by the shape and strides, excluding byte offset.
    pub storage_span_bytes: u64,
}

impl AcceleratorTensorLayout {
    /// Constructs a layout from signed package metadata and rejects invalid values.
    pub fn new(
        dtype: AcceleratorScalarType,
        dimensions: &[i64],
        strides: Option<&[i64]>,
        byte_offset: u64,
        order: AcceleratorTensorOrder,
        alignment: u64,
    ) -> Result<Self, AcceleratorValueError> {
        if dimensions.len() > MAX_ACCELERATOR_TENSOR_RANK {
            return Err(AcceleratorValueError::InvalidRank(dimensions.len()));
        }
        let dimensions = dimensions
            .iter()
            .map(|dimension| {
                u64::try_from(*dimension)
                    .map_err(|_| AcceleratorValueError::NegativeDimension(*dimension))
            })
            .collect::<Result<Vec<_>, _>>()?;
        validate_alignment(dtype, alignment, byte_offset)?;
        let canonical = contiguous_strides(&dimensions, order)?;
        let strides = match (order, strides) {
            (AcceleratorTensorOrder::Strided, Some(values)) => {
                checked_strides(values, dimensions.len(), true)?
            }
            (AcceleratorTensorOrder::Strided, None) => {
                return Err(AcceleratorValueError::StrideRankMismatch {
                    rank: dimensions.len(),
                    strides: 0,
                })
            }
            (_, Some(values)) => {
                let values = checked_strides(values, dimensions.len(), true)?;
                if values != canonical {
                    return Err(AcceleratorValueError::IncompatibleLayout);
                }
                values
            }
            (_, None) => canonical,
        };
        let element_count = dimensions.iter().try_fold(1u64, |count, dimension| {
            count
                .checked_mul(*dimension)
                .ok_or(AcceleratorValueError::IntegerOverflow("element_count"))
        })?;
        let byte_size = element_count
            .checked_mul(dtype.byte_width())
            .ok_or(AcceleratorValueError::IntegerOverflow("byte_size"))?;
        let storage_span_bytes = storage_span(&dimensions, &strides, dtype)?;
        byte_offset
            .checked_add(storage_span_bytes)
            .ok_or(AcceleratorValueError::IntegerOverflow("byte_offset"))?;
        Ok(Self {
            dtype,
            dimensions,
            strides,
            byte_offset,
            order,
            alignment,
            element_count,
            byte_size,
            storage_span_bytes,
        })
    }

    /// Returns the checked tensor rank.
    pub fn rank(&self) -> usize {
        self.dimensions.len()
    }

    /// Revalidates metadata decoded from an adapter or package boundary.
    pub fn validate(&self) -> Result<(), AcceleratorValueError> {
        let dimensions = self
            .dimensions
            .iter()
            .map(|dimension| {
                i64::try_from(*dimension)
                    .map_err(|_| AcceleratorValueError::IntegerOverflow("dimension"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let strides = self
            .strides
            .iter()
            .map(|stride| {
                i64::try_from(*stride).map_err(|_| AcceleratorValueError::IntegerOverflow("stride"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let rebuilt = Self::new(
            self.dtype,
            &dimensions,
            Some(&strides),
            self.byte_offset,
            self.order,
            self.alignment,
        )?;
        if &rebuilt == self {
            Ok(())
        } else {
            Err(AcceleratorValueError::IncompatibleLayout)
        }
    }

    /// Returns whether this layout has canonical row-major strides.
    pub fn is_row_major_contiguous(&self) -> bool {
        contiguous_strides(&self.dimensions, AcceleratorTensorOrder::RowMajor)
            .is_ok_and(|strides| strides == self.strides)
    }

    /// Returns whether a non-singleton dimension reuses one backing element.
    pub fn is_broadcast_view(&self) -> bool {
        self.dimensions
            .iter()
            .zip(&self.strides)
            .any(|(dimension, stride)| *dimension > 1 && *stride == 0)
    }
}

/// Validates explicit non-negative element strides under the selected order.
fn checked_strides(
    values: &[i64],
    rank: usize,
    allow_zero: bool,
) -> Result<Vec<u64>, AcceleratorValueError> {
    if values.len() != rank {
        return Err(AcceleratorValueError::StrideRankMismatch {
            rank,
            strides: values.len(),
        });
    }
    values
        .iter()
        .map(|stride| {
            if *stride < 0 || (*stride == 0 && !allow_zero) {
                Err(AcceleratorValueError::InvalidStride(*stride))
            } else {
                Ok(*stride as u64)
            }
        })
        .collect()
}

/// Computes canonical element strides for one contiguous order.
fn contiguous_strides(
    dimensions: &[u64],
    order: AcceleratorTensorOrder,
) -> Result<Vec<u64>, AcceleratorValueError> {
    let mut strides = vec![1; dimensions.len()];
    let mut running = 1u64;
    let indexes: Box<dyn Iterator<Item = usize>> = match order {
        AcceleratorTensorOrder::ColumnMajor => Box::new(0..dimensions.len()),
        AcceleratorTensorOrder::RowMajor | AcceleratorTensorOrder::Strided => {
            Box::new((0..dimensions.len()).rev())
        }
    };
    for index in indexes {
        strides[index] = running;
        running = running
            .checked_mul(dimensions[index])
            .ok_or(AcceleratorValueError::IntegerOverflow("strides"))?;
    }
    Ok(strides)
}

/// Computes the backing byte span touched by a non-negative-stride tensor.
fn storage_span(
    dimensions: &[u64],
    strides: &[u64],
    dtype: AcceleratorScalarType,
) -> Result<u64, AcceleratorValueError> {
    if dimensions.contains(&0) {
        return Ok(0);
    }
    let last_element =
        dimensions
            .iter()
            .zip(strides)
            .try_fold(0u64, |offset, (dimension, stride)| {
                let extent = dimension
                    .checked_sub(1)
                    .and_then(|value| value.checked_mul(*stride))
                    .ok_or(AcceleratorValueError::IntegerOverflow("storage_span"))?;
                offset
                    .checked_add(extent)
                    .ok_or(AcceleratorValueError::IntegerOverflow("storage_span"))
            })?;
    last_element
        .checked_add(1)
        .and_then(|elements| elements.checked_mul(dtype.byte_width()))
        .ok_or(AcceleratorValueError::IntegerOverflow("storage_span_bytes"))
}

/// Enforces natural power-of-two alignment and aligned offsets.
fn validate_alignment(
    dtype: AcceleratorScalarType,
    alignment: u64,
    byte_offset: u64,
) -> Result<(), AcceleratorValueError> {
    if !alignment.is_power_of_two()
        || alignment < dtype.alignment()
        || !alignment.is_multiple_of(dtype.alignment())
    {
        return Err(AcceleratorValueError::InvalidAlignment(alignment));
    }
    if !byte_offset.is_multiple_of(alignment) {
        return Err(AcceleratorValueError::MisalignedOffset {
            offset: byte_offset,
            alignment,
        });
    }
    Ok(())
}
