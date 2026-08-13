//! Canonical values shared by accelerator packages and compiler adapters.

#[path = "value/dtype.rs"]
mod dtype;
#[path = "value/layout.rs"]
mod layout;
#[path = "value/packet.rs"]
mod packet;
#[path = "value/resource.rs"]
mod resource;
#[path = "value/schema.rs"]
mod schema;

pub use crate::accelerator_contract::AcceleratorValueError;
pub use dtype::AcceleratorScalarType;
pub use layout::{AcceleratorTensorLayout, AcceleratorTensorOrder, MAX_ACCELERATOR_TENSOR_RANK};
pub use packet::{
    AcceleratorPacketOwnership, AcceleratorTensorPacket, AcceleratorTensorPacketInput,
    ACCELERATOR_TENSOR_PACKET_SCHEMA,
};
pub use resource::{
    AcceleratorAddressSpace, AcceleratorDeleter, AcceleratorDeleterInvocation, AcceleratorDeviceId,
    AcceleratorLinearResource, AcceleratorResourceClass, AcceleratorResourceHandle,
    AcceleratorResourceId, AcceleratorResourcePrincipal, AcceleratorResourceRole,
};
pub use schema::{
    AcceleratorGeneratedAdapter, AcceleratorOwnershipTransition, AcceleratorScalarSchema,
    AcceleratorValueContract,
};

#[cfg(test)]
#[path = "value_test.rs"]
mod value_test;
