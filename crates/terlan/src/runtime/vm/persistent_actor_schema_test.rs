use super::{
    VmPersistentActorField, VmPersistentActorMigrationEdge, VmPersistentActorMigrationEffect,
    VmPersistentActorMigrationGraph, VmPersistentActorMigrationGuard,
    VmPersistentActorSchemaDescriptor, VmPersistentActorSchemaError, VmPersistentActorSchemaKey,
};

#[test]
fn vm_persistent_actor_schema_plans_deterministic_migration_chain() {
    let mut graph = VmPersistentActorMigrationGraph::new();
    let schema_v1 = schema("PlayerActor", 1);
    let schema_v2 = schema("PlayerActor", 2);
    let schema_v3 = schema("PlayerActor", 3);
    let mailbox_v1 = schema("PlayerMailbox", 1);
    let mailbox_v2 = schema("PlayerMailbox", 2);

    graph
        .register_schema(
            descriptor(schema_v1.clone(), 1)
                .field(required_field("name", "String"))
                .event("Joined")
                .mailbox(mailbox_v1.clone())
                .build(),
        )
        .expect("register v1");
    graph
        .register_schema(
            descriptor(schema_v2.clone(), 2)
                .field(required_field("display_name", "String"))
                .field(required_field("score", "Int"))
                .event("Joined")
                .mailbox(mailbox_v1)
                .build(),
        )
        .expect("register v2");
    graph
        .register_schema(
            descriptor(schema_v3.clone(), 3)
                .field(required_field("display_name", "String"))
                .field(required_field("score", "Int"))
                .field(optional_field("legacy_name", "String"))
                .event("PlayerJoined")
                .mailbox(mailbox_v2)
                .build(),
        )
        .expect("register v3");

    graph
        .add_edge(
            VmPersistentActorMigrationEdge::new(schema_v1.clone(), schema_v2.clone())
                .rename_field("name", "display_name")
                .default_field("score"),
        )
        .expect("v1 to v2 edge");
    graph
        .add_edge(
            VmPersistentActorMigrationEdge::new(schema_v2.clone(), schema_v3.clone())
                .tombstone_field("name")
                .rename_event_variant("Joined", "PlayerJoined")
                .migrate_mailbox_payload(),
        )
        .expect("v2 to v3 edge");

    let plan = graph.plan(&schema_v1, &schema_v3).expect("migration plan");

    assert_eq!(plan.len(), 2);
    assert_eq!(plan[0].from, schema_v1);
    assert_eq!(plan[0].to, schema_v2);
    assert_eq!(plan[1].to, schema_v3);
}

#[test]
fn vm_persistent_actor_schema_rejects_duplicate_missing_and_cyclic_migrations() {
    let schema_v1 = schema("MatchActor", 1);
    let schema_v2 = schema("MatchActor", 2);
    let schema_v3 = schema("MatchActor", 3);
    let mut graph = VmPersistentActorMigrationGraph::new();
    register_minimal(&mut graph, schema_v1.clone(), 1);
    register_minimal(&mut graph, schema_v2.clone(), 2);
    register_minimal(&mut graph, schema_v3.clone(), 3);

    assert_eq!(
        graph.register_schema(
            descriptor(schema_v1.clone(), 1)
                .field(required_field("state", "String"))
                .build()
        ),
        Err(VmPersistentActorSchemaError::DuplicateSchemaId(
            schema_v1.clone()
        ))
    );
    assert_eq!(
        graph.plan(&schema_v1, &schema_v3),
        Err(VmPersistentActorSchemaError::MissingMigrationEdge {
            from: schema_v1.clone(),
            to: schema_v3.clone()
        })
    );

    graph
        .add_edge(VmPersistentActorMigrationEdge::new(
            schema_v1.clone(),
            schema_v2.clone(),
        ))
        .expect("v1 to v2");
    assert_eq!(
        graph.add_edge(VmPersistentActorMigrationEdge::new(
            schema_v1.clone(),
            schema_v3.clone()
        )),
        Err(VmPersistentActorSchemaError::AmbiguousMigrationEdge {
            from: schema_v1.clone()
        })
    );
    graph
        .add_edge(VmPersistentActorMigrationEdge::new(
            schema_v2.clone(),
            schema_v1.clone(),
        ))
        .expect("v2 to v1");

    assert_eq!(
        graph.plan(&schema_v1, &schema_v3),
        Err(VmPersistentActorSchemaError::MigrationGraphCycle { schema: schema_v1 })
    );
}

#[test]
fn vm_persistent_actor_schema_rejects_unsafe_migration_guards_and_effects() {
    let schema_v1 = schema("TimerActor", 1);
    let schema_v2 = schema("TimerActor", 2);

    assert_eq!(
        graph_with(schema_v1.clone(), schema_v2.clone()).add_edge(
            VmPersistentActorMigrationEdge::new(schema_v1.clone(), schema_v2.clone())
                .with_guard(VmPersistentActorMigrationGuard::Nondeterministic)
        ),
        Err(
            VmPersistentActorSchemaError::NondeterministicMigrationGuard {
                from: schema_v1.clone(),
                to: schema_v2.clone()
            }
        )
    );
    assert_eq!(
        graph_with(schema_v1.clone(), schema_v2.clone()).add_edge(
            VmPersistentActorMigrationEdge::new(schema_v1.clone(), schema_v2.clone())
                .with_guard(VmPersistentActorMigrationGuard::WallClockDependent)
        ),
        Err(VmPersistentActorSchemaError::WallClockDependentMigration {
            from: schema_v1.clone(),
            to: schema_v2.clone()
        })
    );
    assert_eq!(
        graph_with(schema_v1.clone(), schema_v2.clone()).add_edge(
            VmPersistentActorMigrationEdge::new(schema_v1.clone(), schema_v2.clone())
                .with_effect(VmPersistentActorMigrationEffect::SideEffectful)
        ),
        Err(VmPersistentActorSchemaError::SideEffectfulMigration {
            from: schema_v1,
            to: schema_v2
        })
    );
}

#[test]
fn vm_persistent_actor_schema_rejects_lossy_event_mailbox_and_package_changes() {
    let schema_v1 = schema("ChatActor", 1);
    let schema_v2 = schema("ChatActor", 2);
    let mailbox_v1 = schema("ChatMailbox", 1);
    let mailbox_v2 = schema("ChatMailbox", 2);
    let mut graph = VmPersistentActorMigrationGraph::new();
    graph
        .register_schema(
            descriptor(schema_v1.clone(), 1)
                .field(required_field("room", "String"))
                .event("MessageSent")
                .mailbox(mailbox_v1)
                .build(),
        )
        .expect("register v1");
    graph
        .register_schema(
            descriptor(schema_v2.clone(), 2)
                .field(required_field("topic", "String"))
                .event("MessagePublished")
                .mailbox(mailbox_v2)
                .build(),
        )
        .expect("register v2");

    assert_eq!(
        graph.add_edge(VmPersistentActorMigrationEdge::new(
            schema_v1.clone(),
            schema_v2.clone()
        )),
        Err(VmPersistentActorSchemaError::RequiredFieldLost {
            field: "topic".to_string(),
            from: schema_v1.clone(),
            to: schema_v2.clone()
        })
    );
    assert_eq!(
        graph.add_edge(
            VmPersistentActorMigrationEdge::new(schema_v1.clone(), schema_v2.clone())
                .rename_field("room", "topic")
        ),
        Err(
            VmPersistentActorSchemaError::UnknownEventConstructorVariant {
                variant: "MessageSent".to_string(),
                schema: schema_v2.clone()
            }
        )
    );
    assert_eq!(
        graph.add_edge(
            VmPersistentActorMigrationEdge::new(schema_v1.clone(), schema_v2.clone())
                .rename_field("room", "topic")
                .rename_event_variant("MessageSent", "MessagePublished")
        ),
        Err(
            VmPersistentActorSchemaError::IncompatibleMailboxPayloadSchema {
                from: schema_v1,
                to: schema_v2
            }
        )
    );
    assert_eq!(
        VmPersistentActorSchemaDescriptor::new(schema("ChatActor", 3), 0),
        Err(VmPersistentActorSchemaError::StalePackageSchemaVersion {
            schema: schema("ChatActor", 3),
            expected_at_least: 1,
            actual: 0
        })
    );
}

#[test]
fn vm_persistent_actor_schema_rejects_out_of_order_event_migration_sequences() {
    let graph = VmPersistentActorMigrationGraph::new();

    assert_eq!(graph.validate_event_migration_sequence(&[1, 2, 3]), Ok(()));
    assert_eq!(
        graph.validate_event_migration_sequence(&[1, 3, 2]),
        Err(VmPersistentActorSchemaError::OutOfOrderEventMigration {
            expected_next: 2,
            actual: 3
        })
    );
}

fn schema(id: &str, version: u64) -> VmPersistentActorSchemaKey {
    VmPersistentActorSchemaKey::new(id, version).expect("schema key should be valid")
}

fn required_field(name: &str, type_name: &str) -> VmPersistentActorField {
    VmPersistentActorField::required(name, type_name).expect("field should be valid")
}

fn optional_field(name: &str, type_name: &str) -> VmPersistentActorField {
    VmPersistentActorField::optional(name, type_name).expect("field should be valid")
}

fn descriptor(key: VmPersistentActorSchemaKey, package_version: u64) -> DescriptorBuilder {
    DescriptorBuilder {
        descriptor: VmPersistentActorSchemaDescriptor::new(key, package_version)
            .expect("descriptor should be valid"),
    }
}

fn register_minimal(
    graph: &mut VmPersistentActorMigrationGraph,
    key: VmPersistentActorSchemaKey,
    package_version: u64,
) {
    graph
        .register_schema(
            descriptor(key, package_version)
                .field(required_field("state", "String"))
                .event("Changed")
                .build(),
        )
        .expect("schema should register");
}

fn graph_with(
    from: VmPersistentActorSchemaKey,
    to: VmPersistentActorSchemaKey,
) -> VmPersistentActorMigrationGraph {
    let mut graph = VmPersistentActorMigrationGraph::new();
    register_minimal(&mut graph, from, 1);
    register_minimal(&mut graph, to, 2);
    graph
}

struct DescriptorBuilder {
    descriptor: VmPersistentActorSchemaDescriptor,
}

impl DescriptorBuilder {
    fn field(mut self, field: VmPersistentActorField) -> Self {
        self.descriptor = self.descriptor.with_field(field).expect("field accepted");
        self
    }

    fn event(mut self, variant: &str) -> Self {
        self.descriptor = self
            .descriptor
            .with_event_variant(variant)
            .expect("event variant accepted");
        self
    }

    fn mailbox(mut self, schema: VmPersistentActorSchemaKey) -> Self {
        self.descriptor = self.descriptor.with_mailbox_payload_schema(schema);
        self
    }

    fn build(self) -> VmPersistentActorSchemaDescriptor {
        self.descriptor
    }
}
