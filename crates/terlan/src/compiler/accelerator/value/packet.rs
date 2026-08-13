//! Package-neutral tensor interchange metadata.

use serde::{Deserialize, Serialize};

use super::{
    AcceleratorAddressSpace, AcceleratorDeleter, AcceleratorDeviceId, AcceleratorResourceHandle,
    AcceleratorResourceId, AcceleratorResourceRole, AcceleratorScalarType, AcceleratorTensorLayout,
    AcceleratorValueError,
};

/// Current package-neutral tensor packet schema.
pub const ACCELERATOR_TENSOR_PACKET_SCHEMA: u64 = 1;

/// Ownership carried by a tensor packet at an inter-package boundary.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum AcceleratorPacketOwnership {
    /// Payload bytes are copied and carry no external lifetime.
    Copied,
    /// Packet transfers an owned opaque resource to the receiver.
    Transferred,
    /// Packet borrows an opaque resource for one non-zero lexical scope.
    Borrowed {
        /// Compiler-owned lexical scope identity.
        scope: u64,
    },
}

/// Complete checked input for constructing one tensor interchange packet.
pub struct AcceleratorTensorPacketInput {
    /// Checked scalar, shape, stride, offset, and byte metadata.
    pub layout: AcceleratorTensorLayout,
    /// Logical storage location.
    pub address_space: AcceleratorAddressSpace,
    /// Device associated with an imported or transferred resource.
    pub device: Option<AcceleratorDeviceId>,
    /// Optional stream resource ordering packet availability.
    pub stream: Option<AcceleratorResourceId>,
    /// Copy, transfer, or lexical borrow contract.
    pub ownership: AcceleratorPacketOwnership,
    /// Opaque allocation or imported tensor handle when payload is not copied.
    pub resource: Option<AcceleratorResourceHandle>,
    /// Exactly-once cleanup contract carried by transferred resources.
    pub deleter: AcceleratorDeleter,
    /// Number of payload or backing bytes made available by the sender.
    pub available_bytes: u64,
}

/// Versioned tensor metadata exchanged without exposing backend pointers.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorTensorPacket {
    /// Tensor packet schema version.
    pub schema: u64,
    /// Checked scalar, shape, stride, offset, and byte metadata.
    pub layout: AcceleratorTensorLayout,
    /// Logical storage location.
    pub address_space: AcceleratorAddressSpace,
    /// Device associated with an imported or transferred resource.
    pub device: Option<AcceleratorDeviceId>,
    /// Optional stream resource ordering packet availability.
    pub stream: Option<AcceleratorResourceId>,
    /// Copy, transfer, or lexical borrow contract.
    pub ownership: AcceleratorPacketOwnership,
    /// Opaque allocation or imported tensor handle when payload is not copied.
    pub resource: Option<AcceleratorResourceHandle>,
    /// Exactly-once cleanup contract carried by transferred resources.
    pub deleter: AcceleratorDeleter,
    /// Number of payload or backing bytes made available by the sender.
    pub available_bytes: u64,
}

impl AcceleratorTensorPacket {
    /// Constructs and validates one packet against the canonical schema.
    pub fn new(input: AcceleratorTensorPacketInput) -> Result<Self, AcceleratorValueError> {
        let AcceleratorTensorPacketInput {
            layout,
            address_space,
            device,
            stream,
            ownership,
            resource,
            deleter,
            available_bytes,
        } = input;
        let packet = Self {
            schema: ACCELERATOR_TENSOR_PACKET_SCHEMA,
            layout,
            address_space,
            device,
            stream,
            ownership,
            resource,
            deleter,
            available_bytes,
        };
        packet.validate()?;
        Ok(packet)
    }

    /// Validates schema, sizes, devices, ownership, borrow scope, and deleter metadata.
    pub fn validate(&self) -> Result<(), AcceleratorValueError> {
        if self.schema != ACCELERATOR_TENSOR_PACKET_SCHEMA {
            return Err(AcceleratorValueError::UnsupportedPacketSchema(self.schema));
        }
        self.layout.validate()?;
        self.address_space.validate()?;
        self.deleter.validate()?;
        let required_bytes = self
            .layout
            .byte_offset
            .checked_add(self.layout.storage_span_bytes)
            .ok_or(AcceleratorValueError::IntegerOverflow(
                "packet_required_bytes",
            ))?;
        if self.available_bytes < required_bytes {
            return Err(AcceleratorValueError::ByteCountMismatch {
                expected: required_bytes,
                actual: self.available_bytes,
            });
        }
        if let Some(space_device) = self.address_space.device() {
            if self.device.as_ref() != Some(space_device) {
                return Err(AcceleratorValueError::CrossDeviceAlias);
            }
        } else if self.device.is_some() {
            return Err(AcceleratorValueError::CrossDeviceAlias);
        }
        if self
            .resource
            .as_ref()
            .is_some_and(|resource| resource.address_space != self.address_space)
        {
            return Err(AcceleratorValueError::CrossDeviceAlias);
        }
        match (&self.ownership, &self.resource) {
            (AcceleratorPacketOwnership::Copied, None) => {
                if self.deleter != AcceleratorDeleter::None {
                    return Err(AcceleratorValueError::InvalidPacketOwnership);
                }
            }
            (
                AcceleratorPacketOwnership::Transferred,
                Some(AcceleratorResourceHandle {
                    role: AcceleratorResourceRole::Owned { .. },
                    ..
                }),
            ) => {}
            (
                AcceleratorPacketOwnership::Borrowed { scope },
                Some(AcceleratorResourceHandle {
                    role:
                        AcceleratorResourceRole::Borrowed {
                            scope: handle_scope,
                            ..
                        },
                    ..
                }),
            ) if *scope != 0
                && scope == handle_scope
                && self.deleter == AcceleratorDeleter::None => {}
            (AcceleratorPacketOwnership::Borrowed { .. }, _) => {
                return Err(AcceleratorValueError::EscapedBorrow)
            }
            _ => return Err(AcceleratorValueError::InvalidPacketOwnership),
        }
        Ok(())
    }

    /// Verifies that a package descriptor admits the packet scalar type.
    pub fn validate_supported_dtypes(
        &self,
        supported: &[String],
    ) -> Result<(), AcceleratorValueError> {
        let dtype = self.layout.dtype.identifier();
        if supported.iter().any(|candidate| candidate == dtype) {
            Ok(())
        } else {
            Err(AcceleratorValueError::UnsupportedScalarType(
                dtype.to_string(),
            ))
        }
    }

    /// Returns the packet's canonical scalar type.
    pub const fn dtype(&self) -> AcceleratorScalarType {
        self.layout.dtype
    }
}
