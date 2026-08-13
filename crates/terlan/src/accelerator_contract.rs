//! Backend-neutral pointer-free accelerator resource contract.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Typed rejection produced before an accelerator package operation is dispatched.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "detail", rename_all = "kebab-case")]
pub enum AcceleratorValueError {
    /// A package named a scalar type outside the canonical compiler model.
    UnsupportedScalarType(String),
    /// A tensor dimension was negative.
    NegativeDimension(i64),
    /// A tensor exceeded the compiler rank limit.
    InvalidRank(usize),
    /// Shape, stride, offset, or byte-size arithmetic overflowed.
    IntegerOverflow(&'static str),
    /// A strided layout omitted one stride per dimension.
    StrideRankMismatch { rank: usize, strides: usize },
    /// A stride was zero or negative.
    InvalidStride(i64),
    /// Explicit strides did not match the declared contiguous order.
    IncompatibleLayout,
    /// Alignment was zero, non-power-of-two, or smaller than the scalar alignment.
    InvalidAlignment(u64),
    /// The byte offset did not satisfy the declared alignment.
    MisalignedOffset { offset: u64, alignment: u64 },
    /// A backend, device, owner, or external address-space identity was malformed.
    InvalidIdentity(String),
    /// A resource handle no longer names the current resource generation.
    StaleHandle,
    /// A resource operation used a borrowed handle where ownership was required.
    OwnershipRequired,
    /// A resource cannot transfer or dispose while a borrow remains active.
    BorrowActive,
    /// A borrow was released by a different scope or principal.
    BorrowMismatch,
    /// A resource has already been disposed.
    AlreadyDisposed,
    /// A packet borrow did not carry a valid lexical scope.
    EscapedBorrow,
    /// Packet metadata aliases a resource owned by another device.
    CrossDeviceAlias,
    /// Packet bytes do not match the checked tensor layout.
    ByteCountMismatch { expected: u64, actual: u64 },
    /// Packet schema is not supported by this compiler.
    UnsupportedPacketSchema(u64),
    /// Packet ownership and resource handle role disagree.
    InvalidPacketOwnership,
}

impl fmt::Display for AcceleratorValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "accelerator value rejected: ")?;
        match self {
            Self::UnsupportedScalarType(value) => {
                write!(formatter, "unsupported scalar type `{value}`")
            }
            Self::NegativeDimension(value) => write!(formatter, "negative dimension `{value}`"),
            Self::InvalidRank(rank) => {
                write!(formatter, "rank `{rank}` exceeds the supported limit")
            }
            Self::IntegerOverflow(field) => {
                write!(formatter, "integer overflow while computing `{field}`")
            }
            Self::StrideRankMismatch { rank, strides } => {
                write!(formatter, "rank `{rank}` has `{strides}` strides")
            }
            Self::InvalidStride(stride) => write!(formatter, "invalid stride `{stride}`"),
            Self::IncompatibleLayout => write!(
                formatter,
                "strides do not match the declared contiguous order"
            ),
            Self::InvalidAlignment(value) => write!(formatter, "invalid alignment `{value}`"),
            Self::MisalignedOffset { offset, alignment } => write!(
                formatter,
                "byte offset `{offset}` is not aligned to `{alignment}`"
            ),
            Self::InvalidIdentity(value) => write!(formatter, "invalid stable identity `{value}`"),
            Self::StaleHandle => write!(formatter, "stale resource handle"),
            Self::OwnershipRequired => write!(formatter, "resource ownership is required"),
            Self::BorrowActive => write!(formatter, "resource has an active borrow"),
            Self::BorrowMismatch => write!(formatter, "borrow scope or principal does not match"),
            Self::AlreadyDisposed => write!(formatter, "resource was already disposed"),
            Self::EscapedBorrow => write!(formatter, "borrow escaped its lexical scope"),
            Self::CrossDeviceAlias => write!(formatter, "resource and packet devices differ"),
            Self::ByteCountMismatch { expected, actual } => write!(
                formatter,
                "expected `{expected}` bytes but received `{actual}`"
            ),
            Self::UnsupportedPacketSchema(schema) => {
                write!(formatter, "unsupported tensor packet schema `{schema}`")
            }
            Self::InvalidPacketOwnership => write!(
                formatter,
                "packet ownership does not match its resource handle"
            ),
        }
    }
}

impl std::error::Error for AcceleratorValueError {}

/// Validates a stable lowercase identity shared by accelerator contracts.
pub(crate) fn validate_value_identity(value: &str) -> Result<(), AcceleratorValueError> {
    let valid = !value.is_empty()
        && value == value.trim()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        });
    if valid {
        Ok(())
    } else {
        Err(AcceleratorValueError::InvalidIdentity(value.to_string()))
    }
}

/// Stable accelerator device identity without a backend pointer.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorDeviceId {
    /// Package-neutral backend identity.
    pub backend: String,
    /// Backend-assigned device ordinal.
    pub ordinal: u32,
}

impl AcceleratorDeviceId {
    /// Constructs and validates a device identity.
    pub fn new(backend: impl Into<String>, ordinal: u32) -> Result<Self, AcceleratorValueError> {
        let backend = backend.into();
        validate_value_identity(&backend)?;
        Ok(Self { backend, ordinal })
    }
}

/// Logical storage location exposed at compiler and package boundaries.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum AcceleratorAddressSpace {
    /// Ordinary process-owned host memory.
    Host,
    /// Page-locked host memory associated with a backend.
    PinnedHost { backend: String },
    /// Memory owned by one accelerator device.
    Device { device: AcceleratorDeviceId },
    /// Memory governed by another package or external runtime.
    External { provider: String, space: String },
}

impl AcceleratorAddressSpace {
    /// Validates all package-owned identities in the address-space descriptor.
    pub fn validate(&self) -> Result<(), AcceleratorValueError> {
        match self {
            Self::Host => Ok(()),
            Self::PinnedHost { backend } => validate_value_identity(backend),
            Self::Device { device } => validate_value_identity(&device.backend),
            Self::External { provider, space } => {
                validate_value_identity(provider)?;
                validate_value_identity(space)
            }
        }
    }

    /// Returns the device identity when the address space is device-local.
    pub fn device(&self) -> Option<&AcceleratorDeviceId> {
        match self {
            Self::Device { device } => Some(device),
            Self::Host | Self::PinnedHost { .. } | Self::External { .. } => None,
        }
    }
}

/// Resource kinds whose lifetimes are enforced linearly by generated adapters.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AcceleratorResourceClass {
    /// Backend device context.
    DeviceContext,
    /// Device or host allocation.
    Allocation,
    /// Ordered asynchronous execution stream.
    Stream,
    /// Completion event.
    Event,
    /// Loaded accelerator module.
    Module,
    /// Resolved kernel entrypoint.
    Kernel,
    /// Captured execution graph.
    Graph,
    /// Collective communication membership and native transport state.
    Communicator,
    /// Tensor resource imported from another owner.
    ImportedTensor,
}

/// Stable resource slot and generation used to reject stale handles.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorResourceId {
    /// Resource-table slot.
    pub slot: u64,
    /// Generation advanced by ownership transfer and disposal.
    pub generation: u64,
}

/// Stable package, actor, or runtime owner identity.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
#[serde(transparent)]
pub struct AcceleratorResourcePrincipal(String);

impl AcceleratorResourcePrincipal {
    /// Constructs a validated owner identity.
    pub fn new(value: impl Into<String>) -> Result<Self, AcceleratorValueError> {
        let value = value.into();
        validate_value_identity(&value)?;
        Ok(Self(value))
    }

    /// Returns the stable identity text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Access represented by a resource handle.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum AcceleratorResourceRole {
    /// Handle carries ownership for one principal.
    Owned {
        /// Current resource owner.
        principal: AcceleratorResourcePrincipal,
    },
    /// Handle is valid only for one principal and lexical scope.
    Borrowed {
        /// Principal allowed to use the borrow.
        principal: AcceleratorResourcePrincipal,
        /// Non-zero compiler-owned lexical scope identity.
        scope: u64,
    },
}

/// Pointer-free handle admitted into compiler-generated package calls.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorResourceHandle {
    /// Versioned resource-table identity.
    pub id: AcceleratorResourceId,
    /// Resource class checked by adapters.
    pub class: AcceleratorResourceClass,
    /// Pointer-free storage location used to reject cross-device aliases.
    pub address_space: AcceleratorAddressSpace,
    /// Handle ownership or borrow role.
    pub role: AcceleratorResourceRole,
}

impl AcceleratorResourceHandle {
    /// Validates pointer-free identity, address-space, and borrow metadata.
    pub fn validate(&self) -> Result<(), AcceleratorValueError> {
        if self.id.slot == 0 {
            return Err(AcceleratorValueError::InvalidIdentity(
                "resource slot 0".to_string(),
            ));
        }
        self.address_space.validate()?;
        match &self.role {
            AcceleratorResourceRole::Owned { principal } => {
                validate_value_identity(principal.as_str())
            }
            AcceleratorResourceRole::Borrowed { principal, scope } => {
                validate_value_identity(principal.as_str())?;
                if *scope == 0 {
                    return Err(AcceleratorValueError::EscapedBorrow);
                }
                Ok(())
            }
        }
    }
}

/// Package operation invoked exactly once when an owned resource is disposed.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum AcceleratorDeleter {
    /// Resource needs no package callback.
    None,
    /// Dispatch one package operation with the opaque resource ID.
    PackageOperation {
        /// Package owning the deleter implementation.
        package: String,
        /// Operation identifier declared by package metadata.
        operation: String,
    },
}

impl AcceleratorDeleter {
    /// Validates package and operation identities.
    pub fn validate(&self) -> Result<(), AcceleratorValueError> {
        match self {
            Self::None => Ok(()),
            Self::PackageOperation { package, operation } => {
                validate_value_identity(package)?;
                validate_value_identity(operation)
            }
        }
    }
}

/// Exactly-once deleter request returned by resource disposal.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorDeleterInvocation {
    /// Resource identity before disposal advanced its generation.
    pub resource: AcceleratorResourceId,
    /// Validated deleter contract.
    pub deleter: AcceleratorDeleter,
}

/// Canonical state machine for one opaque linear resource.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceleratorLinearResource {
    /// Current slot and generation.
    id: AcceleratorResourceId,
    /// Resource kind.
    class: AcceleratorResourceClass,
    /// Logical storage location.
    address_space: AcceleratorAddressSpace,
    /// Current owner while the resource remains live.
    owner: AcceleratorResourcePrincipal,
    /// Active borrow principal and scope.
    borrow: Option<(AcceleratorResourcePrincipal, u64)>,
    /// Deleter consumed by the first successful disposal.
    deleter: Option<AcceleratorDeleter>,
    /// Whether disposal has completed.
    disposed: bool,
}

impl AcceleratorLinearResource {
    /// Creates one live resource with generation zero.
    pub fn new(
        slot: u64,
        class: AcceleratorResourceClass,
        address_space: AcceleratorAddressSpace,
        owner: AcceleratorResourcePrincipal,
        deleter: AcceleratorDeleter,
    ) -> Result<(Self, AcceleratorResourceHandle), AcceleratorValueError> {
        address_space.validate()?;
        deleter.validate()?;
        let resource = Self {
            id: AcceleratorResourceId {
                slot,
                generation: 0,
            },
            class,
            address_space,
            owner: owner.clone(),
            borrow: None,
            deleter: Some(deleter),
            disposed: false,
        };
        let handle = resource.owned_handle(owner);
        Ok((resource, handle))
    }

    /// Returns the resource's pointer-free address-space identity.
    pub fn address_space(&self) -> &AcceleratorAddressSpace {
        &self.address_space
    }

    /// Creates one active lexical borrow and rejects nested or escaped borrows.
    pub fn borrow(
        &mut self,
        owner: &AcceleratorResourceHandle,
        borrower: AcceleratorResourcePrincipal,
        scope: u64,
    ) -> Result<AcceleratorResourceHandle, AcceleratorValueError> {
        self.require_owner(owner)?;
        if scope == 0 {
            return Err(AcceleratorValueError::EscapedBorrow);
        }
        if self.borrow.is_some() {
            return Err(AcceleratorValueError::BorrowActive);
        }
        self.borrow = Some((borrower.clone(), scope));
        Ok(AcceleratorResourceHandle {
            id: self.id,
            class: self.class,
            address_space: self.address_space.clone(),
            role: AcceleratorResourceRole::Borrowed {
                principal: borrower,
                scope,
            },
        })
    }

    /// Ends the matching lexical borrow.
    pub fn release_borrow(
        &mut self,
        borrowed: &AcceleratorResourceHandle,
    ) -> Result<(), AcceleratorValueError> {
        self.require_live_handle(borrowed)?;
        let AcceleratorResourceRole::Borrowed { principal, scope } = &borrowed.role else {
            return Err(AcceleratorValueError::BorrowMismatch);
        };
        if self.borrow.as_ref() != Some(&(principal.clone(), *scope)) {
            return Err(AcceleratorValueError::BorrowMismatch);
        }
        self.borrow = None;
        Ok(())
    }

    /// Transfers ownership, advances the generation, and invalidates every old handle.
    pub fn transfer(
        &mut self,
        owner: &AcceleratorResourceHandle,
        recipient: AcceleratorResourcePrincipal,
    ) -> Result<AcceleratorResourceHandle, AcceleratorValueError> {
        self.require_owner(owner)?;
        if self.borrow.is_some() {
            return Err(AcceleratorValueError::BorrowActive);
        }
        let next_generation =
            self.id
                .generation
                .checked_add(1)
                .ok_or(AcceleratorValueError::IntegerOverflow(
                    "resource_generation",
                ))?;
        self.id.generation = next_generation;
        self.owner = recipient.clone();
        Ok(self.owned_handle(recipient))
    }

    /// Disposes an owned resource and returns at most one deleter invocation.
    pub fn dispose(
        &mut self,
        owner: &AcceleratorResourceHandle,
    ) -> Result<Option<AcceleratorDeleterInvocation>, AcceleratorValueError> {
        self.require_owner(owner)?;
        if self.borrow.is_some() {
            return Err(AcceleratorValueError::BorrowActive);
        }
        let next_generation =
            self.id
                .generation
                .checked_add(1)
                .ok_or(AcceleratorValueError::IntegerOverflow(
                    "resource_generation",
                ))?;
        let prior = self.id;
        let deleter = self
            .deleter
            .take()
            .ok_or(AcceleratorValueError::AlreadyDisposed)?;
        self.disposed = true;
        self.id.generation = next_generation;
        match deleter {
            AcceleratorDeleter::None => Ok(None),
            deleter => Ok(Some(AcceleratorDeleterInvocation {
                resource: prior,
                deleter,
            })),
        }
    }

    /// Validates a live handle against slot, generation, and class.
    pub fn validate_handle(
        &self,
        handle: &AcceleratorResourceHandle,
    ) -> Result<(), AcceleratorValueError> {
        self.require_live_handle(handle)
    }

    /// Produces an owned handle for the current generation.
    fn owned_handle(&self, principal: AcceleratorResourcePrincipal) -> AcceleratorResourceHandle {
        AcceleratorResourceHandle {
            id: self.id,
            class: self.class,
            address_space: self.address_space.clone(),
            role: AcceleratorResourceRole::Owned { principal },
        }
    }

    /// Enforces current ownership after validating the handle generation.
    fn require_owner(
        &self,
        handle: &AcceleratorResourceHandle,
    ) -> Result<(), AcceleratorValueError> {
        self.require_live_handle(handle)?;
        match &handle.role {
            AcceleratorResourceRole::Owned { principal } if principal == &self.owner => Ok(()),
            AcceleratorResourceRole::Owned { .. } | AcceleratorResourceRole::Borrowed { .. } => {
                Err(AcceleratorValueError::OwnershipRequired)
            }
        }
    }

    /// Rejects disposed resources and handles from another generation or class.
    fn require_live_handle(
        &self,
        handle: &AcceleratorResourceHandle,
    ) -> Result<(), AcceleratorValueError> {
        if self.disposed {
            return Err(AcceleratorValueError::AlreadyDisposed);
        }
        if handle.id != self.id
            || handle.class != self.class
            || handle.address_space != self.address_space
        {
            return Err(AcceleratorValueError::StaleHandle);
        }
        Ok(())
    }
}
