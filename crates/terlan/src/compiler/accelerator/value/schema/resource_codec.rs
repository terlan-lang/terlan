//! Generated native resource-handle codec source.

/// Dependency-free linear resource handle emitted into native package adapters.
pub(super) const RUST_RESOURCE_CODEC: &str = r#"
/// Access carried by one generation-qualified native resource handle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceAccess {
    /// The principal owns the resource and may transfer or dispose it.
    Owned,
    /// The principal may use the resource only within one lexical scope.
    Borrowed {
        /// Nonzero compiler-owned lexical scope identity.
        scope: u64,
    },
}

/// Stable resource-handle rejection shared by generated package adapters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceHandleError {
    /// Owner or native type identity is empty or contains protocol separators.
    InvalidIdentity,
    /// Resource slots and generations are one-based.
    InvalidGeneration,
    /// Borrow scopes are one-based.
    InvalidBorrowScope,
    /// Only an owned handle may create a borrow or transfer ownership.
    OwnershipRequired,
    /// Advancing the resource generation overflowed.
    GenerationOverflow,
}

/// Pointer-free native resource identity carried across a package boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceHandle {
    /// Package-worker principal that owns or borrows the resource.
    pub owner: String,
    /// Package-worker resource-table slot.
    pub id: u64,
    /// Slot generation used to reject stale handles after reuse or transfer.
    pub generation: u64,
    /// Fully qualified Terlan opaque resource type.
    pub type_name: String,
    /// Ownership or lexical borrow represented by this handle.
    pub access: ResourceAccess,
}

impl ResourceHandle {
    /// Constructs and validates one owned resource handle.
    pub fn owned(
        owner: impl Into<String>,
        id: u64,
        generation: u64,
        type_name: impl Into<String>,
    ) -> Result<Self, ResourceHandleError> {
        Self::checked(owner, id, generation, type_name, ResourceAccess::Owned)
    }

    /// Constructs and validates one resource handle with explicit access.
    pub fn checked(
        owner: impl Into<String>,
        id: u64,
        generation: u64,
        type_name: impl Into<String>,
        access: ResourceAccess,
    ) -> Result<Self, ResourceHandleError> {
        let owner = owner.into();
        let type_name = type_name.into();
        if !valid_resource_identity(&owner) || !valid_resource_identity(&type_name) {
            return Err(ResourceHandleError::InvalidIdentity);
        }
        if id == 0 || generation == 0 {
            return Err(ResourceHandleError::InvalidGeneration);
        }
        if matches!(access, ResourceAccess::Borrowed { scope: 0 }) {
            return Err(ResourceHandleError::InvalidBorrowScope);
        }
        Ok(Self {
            owner,
            id,
            generation,
            type_name,
            access,
        })
    }

    /// Creates a lexical borrow without changing the owned generation.
    pub fn borrow(
        &self,
        borrower: impl Into<String>,
        scope: u64,
    ) -> Result<Self, ResourceHandleError> {
        if self.access != ResourceAccess::Owned {
            return Err(ResourceHandleError::OwnershipRequired);
        }
        Self::checked(
            borrower,
            self.id,
            self.generation,
            self.type_name.clone(),
            ResourceAccess::Borrowed { scope },
        )
    }

    /// Transfers ownership and advances the generation, invalidating prior handles.
    pub fn transfer(&self, recipient: impl Into<String>) -> Result<Self, ResourceHandleError> {
        if self.access != ResourceAccess::Owned {
            return Err(ResourceHandleError::OwnershipRequired);
        }
        let generation = self
            .generation
            .checked_add(1)
            .ok_or(ResourceHandleError::GenerationOverflow)?;
        Self::owned(recipient, self.id, generation, self.type_name.clone())
    }

    /// Returns whether this handle names the same resource generation and type.
    pub fn aliases(&self, other: &Self) -> bool {
        self.id == other.id
            && self.generation == other.generation
            && self.type_name == other.type_name
    }
}

/// Accepts decoded protocol identities while excluding separators and whitespace.
fn valid_resource_identity(value: &str) -> bool {
    !value.is_empty()
        && value == value.trim()
        && !value
            .bytes()
            .any(|byte| byte == b':' || byte.is_ascii_whitespace())
}
"#;
