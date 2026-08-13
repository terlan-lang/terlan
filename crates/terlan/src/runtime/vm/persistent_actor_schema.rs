use std::collections::{BTreeMap, BTreeSet};

/// VM-owned schema identity used before persistent actor replay can load state.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct VmPersistentActorSchemaKey {
    pub(crate) id: String,
    pub(crate) version: u64,
}

impl VmPersistentActorSchemaKey {
    pub(crate) fn new(id: impl Into<String>, version: u64) -> Result<Self, String> {
        let id = id.into();
        if id.is_empty() {
            return Err("error[vm_persistent_actor_schema]: schema id must be non-empty".into());
        }
        if version == 0 {
            return Err(
                "error[vm_persistent_actor_schema]: schema version must be non-zero".into(),
            );
        }
        Ok(Self { id, version })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmPersistentActorField {
    pub(crate) name: String,
    pub(crate) type_name: String,
    pub(crate) required: bool,
}

impl VmPersistentActorField {
    pub(crate) fn required(
        name: impl Into<String>,
        type_name: impl Into<String>,
    ) -> Result<Self, String> {
        Self::new(name, type_name, true)
    }

    #[cfg(test)]
    pub(crate) fn optional(
        name: impl Into<String>,
        type_name: impl Into<String>,
    ) -> Result<Self, String> {
        Self::new(name, type_name, false)
    }

    fn new(
        name: impl Into<String>,
        type_name: impl Into<String>,
        required: bool,
    ) -> Result<Self, String> {
        let name = name.into();
        let type_name = type_name.into();
        if name.is_empty() {
            return Err("error[vm_persistent_actor_schema]: field name must be non-empty".into());
        }
        if type_name.is_empty() {
            return Err("error[vm_persistent_actor_schema]: field type must be non-empty".into());
        }
        Ok(Self {
            name,
            type_name,
            required,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmPersistentActorSchemaDescriptor {
    pub(crate) key: VmPersistentActorSchemaKey,
    pub(crate) package_version: u64,
    fields: BTreeMap<String, VmPersistentActorField>,
    event_variants: BTreeSet<String>,
    mailbox_payload_schema: Option<VmPersistentActorSchemaKey>,
}

impl VmPersistentActorSchemaDescriptor {
    pub(crate) fn new(
        key: VmPersistentActorSchemaKey,
        package_version: u64,
    ) -> Result<Self, VmPersistentActorSchemaError> {
        if package_version == 0 {
            return Err(VmPersistentActorSchemaError::StalePackageSchemaVersion {
                schema: key,
                expected_at_least: 1,
                actual: 0,
            });
        }
        Ok(Self {
            key,
            package_version,
            fields: BTreeMap::new(),
            event_variants: BTreeSet::new(),
            mailbox_payload_schema: None,
        })
    }

    pub(crate) fn with_field(
        mut self,
        field: VmPersistentActorField,
    ) -> Result<Self, VmPersistentActorSchemaError> {
        if self.fields.insert(field.name.clone(), field).is_some() {
            return Err(VmPersistentActorSchemaError::DuplicateField { schema: self.key });
        }
        Ok(self)
    }

    pub(crate) fn with_event_variant(mut self, variant: impl Into<String>) -> Result<Self, String> {
        let variant = variant.into();
        if variant.is_empty() {
            return Err(
                "error[vm_persistent_actor_schema]: event variant must be non-empty".into(),
            );
        }
        self.event_variants.insert(variant);
        Ok(self)
    }

    #[cfg(test)]
    pub(crate) fn with_mailbox_payload_schema(
        mut self,
        schema: VmPersistentActorSchemaKey,
    ) -> Self {
        self.mailbox_payload_schema = Some(schema);
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmPersistentActorMigrationGuard {
    Deterministic,
    #[cfg(test)]
    Nondeterministic,
    #[cfg(test)]
    WallClockDependent,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmPersistentActorMigrationEffect {
    Pure,
    SideEffectful,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmPersistentActorMigrationEdge {
    pub(crate) from: VmPersistentActorSchemaKey,
    pub(crate) to: VmPersistentActorSchemaKey,
    pub(crate) field_renames: BTreeMap<String, String>,
    pub(crate) defaulted_fields: BTreeSet<String>,
    pub(crate) tombstoned_fields: BTreeSet<String>,
    pub(crate) event_variant_renames: BTreeMap<String, String>,
    pub(crate) migrates_mailbox_payload: bool,
    pub(crate) guard: VmPersistentActorMigrationGuard,
    pub(crate) effect: VmPersistentActorMigrationEffect,
}

impl VmPersistentActorMigrationEdge {
    pub(crate) fn new(from: VmPersistentActorSchemaKey, to: VmPersistentActorSchemaKey) -> Self {
        Self {
            from,
            to,
            field_renames: BTreeMap::new(),
            defaulted_fields: BTreeSet::new(),
            tombstoned_fields: BTreeSet::new(),
            event_variant_renames: BTreeMap::new(),
            migrates_mailbox_payload: false,
            guard: VmPersistentActorMigrationGuard::Deterministic,
            effect: VmPersistentActorMigrationEffect::Pure,
        }
    }

    #[cfg(test)]
    pub(crate) fn rename_field(mut self, from: impl Into<String>, to: impl Into<String>) -> Self {
        self.field_renames.insert(from.into(), to.into());
        self
    }

    #[cfg(test)]
    pub(crate) fn default_field(mut self, name: impl Into<String>) -> Self {
        self.defaulted_fields.insert(name.into());
        self
    }

    #[cfg(test)]
    pub(crate) fn tombstone_field(mut self, name: impl Into<String>) -> Self {
        self.tombstoned_fields.insert(name.into());
        self
    }

    #[cfg(test)]
    pub(crate) fn rename_event_variant(
        mut self,
        from: impl Into<String>,
        to: impl Into<String>,
    ) -> Self {
        self.event_variant_renames.insert(from.into(), to.into());
        self
    }

    #[cfg(test)]
    pub(crate) fn migrate_mailbox_payload(mut self) -> Self {
        self.migrates_mailbox_payload = true;
        self
    }

    #[cfg(test)]
    pub(crate) fn with_guard(mut self, guard: VmPersistentActorMigrationGuard) -> Self {
        self.guard = guard;
        self
    }

    #[cfg(test)]
    pub(crate) fn with_effect(mut self, effect: VmPersistentActorMigrationEffect) -> Self {
        self.effect = effect;
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmPersistentActorSchemaError {
    DuplicateSchemaId(VmPersistentActorSchemaKey),
    DuplicateField {
        schema: VmPersistentActorSchemaKey,
    },
    UnknownSchemaId(VmPersistentActorSchemaKey),
    MissingMigrationEdge {
        from: VmPersistentActorSchemaKey,
        to: VmPersistentActorSchemaKey,
    },
    MigrationGraphCycle {
        schema: VmPersistentActorSchemaKey,
    },
    AmbiguousMigrationEdge {
        from: VmPersistentActorSchemaKey,
    },
    #[cfg(test)]
    NondeterministicMigrationGuard {
        from: VmPersistentActorSchemaKey,
        to: VmPersistentActorSchemaKey,
    },
    SideEffectfulMigration {
        from: VmPersistentActorSchemaKey,
        to: VmPersistentActorSchemaKey,
    },
    #[cfg(test)]
    WallClockDependentMigration {
        from: VmPersistentActorSchemaKey,
        to: VmPersistentActorSchemaKey,
    },
    RequiredFieldLost {
        field: String,
        from: VmPersistentActorSchemaKey,
        to: VmPersistentActorSchemaKey,
    },
    UnknownEventConstructorVariant {
        variant: String,
        schema: VmPersistentActorSchemaKey,
    },
    IncompatibleMailboxPayloadSchema {
        from: VmPersistentActorSchemaKey,
        to: VmPersistentActorSchemaKey,
    },
    StalePackageSchemaVersion {
        schema: VmPersistentActorSchemaKey,
        expected_at_least: u64,
        actual: u64,
    },
    #[cfg(test)]
    OutOfOrderEventMigration {
        expected_next: u64,
        actual: u64,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct VmPersistentActorMigrationGraph {
    schemas: BTreeMap<VmPersistentActorSchemaKey, VmPersistentActorSchemaDescriptor>,
    edges: BTreeMap<VmPersistentActorSchemaKey, VmPersistentActorMigrationEdge>,
}

impl VmPersistentActorMigrationGraph {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn register_schema(
        &mut self,
        schema: VmPersistentActorSchemaDescriptor,
    ) -> Result<(), VmPersistentActorSchemaError> {
        if self.schemas.contains_key(&schema.key) {
            return Err(VmPersistentActorSchemaError::DuplicateSchemaId(schema.key));
        }
        self.schemas.insert(schema.key.clone(), schema);
        Ok(())
    }

    pub(crate) fn add_edge(
        &mut self,
        edge: VmPersistentActorMigrationEdge,
    ) -> Result<(), VmPersistentActorSchemaError> {
        let from = self
            .schemas
            .get(&edge.from)
            .ok_or_else(|| VmPersistentActorSchemaError::UnknownSchemaId(edge.from.clone()))?;
        let to = self
            .schemas
            .get(&edge.to)
            .ok_or_else(|| VmPersistentActorSchemaError::UnknownSchemaId(edge.to.clone()))?;

        if self.edges.contains_key(&edge.from) {
            return Err(VmPersistentActorSchemaError::AmbiguousMigrationEdge { from: edge.from });
        }
        match edge.guard {
            VmPersistentActorMigrationGuard::Deterministic => {}
            #[cfg(test)]
            VmPersistentActorMigrationGuard::Nondeterministic => {
                return Err(
                    VmPersistentActorSchemaError::NondeterministicMigrationGuard {
                        from: edge.from,
                        to: edge.to,
                    },
                );
            }
            #[cfg(test)]
            VmPersistentActorMigrationGuard::WallClockDependent => {
                return Err(VmPersistentActorSchemaError::WallClockDependentMigration {
                    from: edge.from,
                    to: edge.to,
                });
            }
        }
        if edge.effect == VmPersistentActorMigrationEffect::SideEffectful {
            return Err(VmPersistentActorSchemaError::SideEffectfulMigration {
                from: edge.from,
                to: edge.to,
            });
        }
        validate_field_coverage(from, to, &edge)?;
        validate_event_variants(from, to, &edge)?;
        validate_mailbox_payload(from, to, &edge)?;

        self.edges.insert(edge.from.clone(), edge);
        Ok(())
    }

    pub(crate) fn plan(
        &self,
        from: &VmPersistentActorSchemaKey,
        to: &VmPersistentActorSchemaKey,
    ) -> Result<Vec<VmPersistentActorMigrationEdge>, VmPersistentActorSchemaError> {
        if !self.schemas.contains_key(from) {
            return Err(VmPersistentActorSchemaError::UnknownSchemaId(from.clone()));
        }
        if !self.schemas.contains_key(to) {
            return Err(VmPersistentActorSchemaError::UnknownSchemaId(to.clone()));
        }
        let mut current = from.clone();
        let mut visited = BTreeSet::new();
        let mut plan = Vec::new();
        while &current != to {
            if !visited.insert(current.clone()) {
                return Err(VmPersistentActorSchemaError::MigrationGraphCycle { schema: current });
            }
            let Some(edge) = self.edges.get(&current) else {
                return Err(VmPersistentActorSchemaError::MissingMigrationEdge {
                    from: current,
                    to: to.clone(),
                });
            };
            current = edge.to.clone();
            plan.push(edge.clone());
        }
        Ok(plan)
    }

    #[cfg(test)]
    pub(crate) fn validate_event_migration_sequence(
        &self,
        event_schema_versions: &[u64],
    ) -> Result<(), VmPersistentActorSchemaError> {
        for (expected_next, actual) in (1..).zip(event_schema_versions.iter()) {
            if *actual != expected_next {
                return Err(VmPersistentActorSchemaError::OutOfOrderEventMigration {
                    expected_next,
                    actual: *actual,
                });
            }
        }
        Ok(())
    }
}

fn validate_field_coverage(
    from: &VmPersistentActorSchemaDescriptor,
    to: &VmPersistentActorSchemaDescriptor,
    edge: &VmPersistentActorMigrationEdge,
) -> Result<(), VmPersistentActorSchemaError> {
    for target in to.fields.values().filter(|field| field.required) {
        let unchanged = from
            .fields
            .get(&target.name)
            .is_some_and(|source| source.type_name == target.type_name);
        let renamed = edge.field_renames.iter().any(|(source, target_name)| {
            target_name == &target.name
                && from
                    .fields
                    .get(source)
                    .is_some_and(|field| field.type_name == target.type_name)
        });
        let defaulted = edge.defaulted_fields.contains(&target.name);
        if !(unchanged || renamed || defaulted) {
            return Err(VmPersistentActorSchemaError::RequiredFieldLost {
                field: target.name.clone(),
                from: from.key.clone(),
                to: to.key.clone(),
            });
        }
    }

    for source in from.fields.values().filter(|field| field.required) {
        let still_exists = to.fields.contains_key(&source.name);
        let renamed = edge.field_renames.contains_key(&source.name);
        let tombstoned = edge.tombstoned_fields.contains(&source.name);
        if !(still_exists || renamed || tombstoned) {
            return Err(VmPersistentActorSchemaError::RequiredFieldLost {
                field: source.name.clone(),
                from: from.key.clone(),
                to: to.key.clone(),
            });
        }
    }
    Ok(())
}

fn validate_event_variants(
    from: &VmPersistentActorSchemaDescriptor,
    to: &VmPersistentActorSchemaDescriptor,
    edge: &VmPersistentActorMigrationEdge,
) -> Result<(), VmPersistentActorSchemaError> {
    for (source, target) in &edge.event_variant_renames {
        if !from.event_variants.contains(source) {
            return Err(
                VmPersistentActorSchemaError::UnknownEventConstructorVariant {
                    variant: source.clone(),
                    schema: from.key.clone(),
                },
            );
        }
        if !to.event_variants.contains(target) {
            return Err(
                VmPersistentActorSchemaError::UnknownEventConstructorVariant {
                    variant: target.clone(),
                    schema: to.key.clone(),
                },
            );
        }
    }
    for variant in &from.event_variants {
        if !to.event_variants.contains(variant) && !edge.event_variant_renames.contains_key(variant)
        {
            return Err(
                VmPersistentActorSchemaError::UnknownEventConstructorVariant {
                    variant: variant.clone(),
                    schema: to.key.clone(),
                },
            );
        }
    }
    Ok(())
}

fn validate_mailbox_payload(
    from: &VmPersistentActorSchemaDescriptor,
    to: &VmPersistentActorSchemaDescriptor,
    edge: &VmPersistentActorMigrationEdge,
) -> Result<(), VmPersistentActorSchemaError> {
    if from.mailbox_payload_schema != to.mailbox_payload_schema && !edge.migrates_mailbox_payload {
        return Err(
            VmPersistentActorSchemaError::IncompatibleMailboxPayloadSchema {
                from: from.key.clone(),
                to: to.key.clone(),
            },
        );
    }
    Ok(())
}

#[cfg(test)]
#[path = "persistent_actor_schema_test.rs"]
#[cfg(test)]
mod persistent_actor_schema_test;
