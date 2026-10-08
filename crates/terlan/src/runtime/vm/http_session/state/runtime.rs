use super::resources::Resources;
use super::*;
#[cfg(test)]
use crate::runtime::vm::actor::VmActorReceive;

impl VmHttpSessionRuntime {
    /// Creates a local HTTP session runtime.
    pub(crate) fn new(node_id: impl Into<String>, ttl_ticks: u64) -> Result<Self, String> {
        Self::new_with_recovery_policy(
            node_id,
            ttl_ticks,
            VmHttpSessionRecoveryPolicy::CreateLocalReplacement,
        )
    }

    /// Creates a local HTTP session runtime with explicit recovery behavior.
    pub(crate) fn new_with_recovery_policy(
        node_id: impl Into<String>,
        ttl_ticks: u64,
        recovery_policy: VmHttpSessionRecoveryPolicy,
    ) -> Result<Self, String> {
        let node_id = node_id.into();
        if node_id.trim().is_empty() {
            return Err("HTTP session node id cannot be empty".to_string());
        }
        if ttl_ticks == 0 {
            return Err(terlan_http_native::session_registry::SessionError::ZeroTtl.to_string());
        }
        let sessions = SessionRegistry::new(recovery_policy);
        #[cfg(test)]
        let live_template_protocol =
            crate::runtime::vm::live_template_protocol::generate_vm_live_template_protocol_manifest(
            );
        #[cfg(test)]
        crate::runtime::vm::live_template_protocol::validate_vm_live_template_protocol_manifest(
            &live_template_protocol,
        )?;
        Ok(Self {
            actors: VmActorRuntime::default(),
            tables: VmTableStore::default(),
            sessions,
            node_id,
            ttl_ticks,
            #[cfg(test)]
            live_template_protocol,
        })
    }

    /// Acquires an exact identity selected by source, or creates a new actor.
    pub(crate) fn lookup_or_create(
        &mut self,
        identity: Option<&str>,
    ) -> Result<VmHttpSessionLookup, String> {
        let identity = identity.unwrap_or_default();
        match self.lookup_available(identity)? {
            Some(lookup) => Ok(lookup),
            None => self.create_session(identity),
        }
    }

    pub(crate) fn lookup_available(
        &mut self,
        identity: &str,
    ) -> Result<Option<VmHttpSessionLookup>, String> {
        let record = self
            .sessions
            .lookup(
                identity,
                &mut Resources {
                    actors: &mut self.actors,
                    tables: &mut self.tables,
                },
            )
            .map_err(|error| error.to_string())?;
        Ok(record.map(|record| self.lookup_for_record(record)))
    }

    pub(crate) fn create_session(
        &mut self,
        excluded_identity: &str,
    ) -> Result<VmHttpSessionLookup, String> {
        self.create_session_for(excluded_identity, self.ttl_ticks)
    }

    pub(crate) fn create_session_for(
        &mut self,
        excluded_identity: &str,
        ttl_ticks: u64,
    ) -> Result<VmHttpSessionLookup, String> {
        let record = self
            .sessions
            .create(
                excluded_identity,
                ttl_ticks,
                &mut Resources {
                    actors: &mut self.actors,
                    tables: &mut self.tables,
                },
            )
            .map_err(|error| error.to_string())?;
        Ok(self.lookup_for_record(record))
    }

    /// Looks up a session only after explicit stateful actor affinity is valid.
    #[cfg(test)]
    pub(crate) fn lookup_or_create_with_affinity_keys(
        &mut self,
        identity: Option<&str>,
        affinity_keys: &[VmHttpSessionAffinityKey],
    ) -> Result<VmHttpSessionLookup, String> {
        resolve_http_session_affinity_key(affinity_keys).map_err(|err| err.render())?;
        self.lookup_or_create(identity)
    }

    /// Writes one session value.
    pub(crate) fn write(
        &mut self,
        session: &VmHttpSession,
        key: impl Into<String>,
        value: ReplValue,
    ) -> Result<(), String> {
        let record = self.live_record(&session.id)?;
        self.tables
            .insert(
                self.actors.processes(),
                record.value.actor,
                record.value.table,
                ReplValue::String(key.into()),
                value,
            )
            .map(|_| ())
    }

    /// Reads one session value.
    pub(crate) fn read(
        &mut self,
        session: &VmHttpSession,
        key: &str,
    ) -> Result<Option<ReplValue>, String> {
        let record = self.live_record(&session.id)?;
        self.tables.lookup(
            self.actors.processes(),
            record.value.actor,
            record.value.table,
            &ReplValue::String(key.to_string()),
        )
    }

    /// Deletes one session value.
    pub(crate) fn delete(
        &mut self,
        session: &VmHttpSession,
        key: &str,
    ) -> Result<Option<ReplValue>, String> {
        let record = self.live_record(&session.id)?;
        let deleted = self.tables.delete(
            self.actors.processes(),
            record.value.actor,
            record.value.table,
            &ReplValue::String(key.to_string()),
        )?;
        Ok(deleted_session_value(deleted))
    }

    /// Returns the optimistic-concurrency version for this session actor state.
    #[cfg(test)]
    pub(crate) fn state_version(&mut self, session: &VmHttpSession) -> Result<u64, String> {
        self.live_record(&session.id)
            .map(|record| record.value.state_version)
    }

    /// Applies one state update only if the caller observed the current version.
    #[cfg(test)]
    pub(crate) fn apply_state_update(
        &mut self,
        session: &VmHttpSession,
        expected_version: u64,
        update: impl FnOnce(&mut Self, &VmHttpSession) -> Result<(), String>,
    ) -> Result<u64, String> {
        let record = self.live_record(&session.id)?;
        if record.value.state_version != expected_version {
            return Err(state_version_conflict_diagnostic(
                &session.id,
                expected_version,
                record.value.state_version,
            ));
        }

        update(self, session)?;
        self.live_record(&session.id)?;
        let record = self
            .sessions
            .value_mut(&session.id)
            .expect("live HTTP session should have mutable state version");
        record.state_version = record.state_version.saturating_add(1);
        Ok(record.state_version)
    }

    /// Captures durable state for a session actor before shutdown or migration.
    #[cfg(test)]
    pub(crate) fn persistence_snapshot(
        &mut self,
        session: &VmHttpSession,
    ) -> Result<VmHttpSessionPersistenceSnapshot, String> {
        let record = self.live_record(&session.id)?;
        let table_entries = self.tables.entries(
            self.actors.processes(),
            record.value.actor,
            record.value.table,
        )?;
        Ok(VmHttpSessionPersistenceSnapshot {
            session_id: record.id,
            expires_at_tick: record.expires_at_tick,
            state_version: record.value.state_version,
            table_entries,
            command_results: record.value.command_results,
        })
    }

    /// Replays a durable session snapshot into this runtime after restart.
    #[cfg(test)]
    pub(crate) fn replay_persistence_snapshot(
        &mut self,
        snapshot: VmHttpSessionPersistenceSnapshot,
    ) -> Result<VmHttpSessionLookup, String> {
        let session_id = normalize_persistence_session_id(&snapshot.session_id)?;
        self.sessions
            .validate_restore(session_id, snapshot.expires_at_tick)
            .map_err(|error| error.to_string())?;

        let actor = self
            .actors
            .spawn_root(VmProcessSource::new("std.http.Session", "actor", 0));
        let table = created_session_table_id(
            self.tables
                .create(
                    self.actors.processes(),
                    actor,
                    format!("http_session:{session_id}"),
                    VmTableAccess::OwnerOnly,
                )
                .expect("replayed HTTP session actor should be live for table creation"),
        );
        for entry in &snapshot.table_entries {
            self.tables
                .insert(
                    self.actors.processes(),
                    actor,
                    table,
                    entry.key.clone(),
                    entry.value.clone(),
                )
                .map(|_| ())?;
        }
        let record = VmHttpSessionRecord {
            id: session_id.to_string(),
            expires_at_tick: snapshot.expires_at_tick,
            value: VmHttpSessionResource {
                actor,
                table,
                state_version: snapshot.state_version,
                command_results: snapshot.command_results,
                live_template_subscribers: BTreeMap::new(),
            },
        };
        self.sessions
            .restore(record.clone())
            .map_err(|error| error.to_string())?;
        Ok(self.lookup_for_record(record))
    }

    /// Enqueues one VM message on the stateful session actor mailbox.
    #[cfg(test)]
    pub(crate) fn enqueue_actor_message(
        &mut self,
        session: &VmHttpSession,
        payload: ReplValue,
    ) -> Result<u64, String> {
        let record = self.live_record(&session.id)?;
        self.actors
            .send(record.value.actor, record.value.actor, payload)
    }

    /// Reports stateful actor mailbox pressure with a stable attribution string.
    #[cfg(test)]
    pub(crate) fn actor_mailbox_backpressure(
        &mut self,
        session: &VmHttpSession,
        threshold: usize,
    ) -> Result<VmHttpSessionMailboxBackpressure, String> {
        if threshold == 0 {
            return Err(
                "HTTP session actor mailbox backpressure threshold must be greater than 0"
                    .to_string(),
            );
        }
        let record = self.live_record(&session.id)?;
        let mailbox_len = self
            .actors
            .processes()
            .get(record.value.actor)
            .expect("live HTTP session actor should exist")
            .mailbox_len();
        let saturated = mailbox_len >= threshold;
        Ok(VmHttpSessionMailboxBackpressure {
            session_id: record.id.clone(),
            actor_pid: record.value.actor.as_u64(),
            mailbox_len,
            threshold,
            saturated,
            attribution: mailbox_backpressure_attribution(
                &record.id,
                mailbox_len,
                threshold,
                saturated,
            ),
        })
    }

    /// Migrates durable session state to another VM HTTP worker.
    #[cfg(test)]
    pub(crate) fn migrate_to_worker(
        &mut self,
        session: &VmHttpSession,
        destination: &mut VmHttpSessionRuntime,
    ) -> Result<VmHttpSessionWorkerMigration, String> {
        let source = self.lookup_existing(&session.id)?;
        if self.node_id == destination.node_id {
            return Err(format!(
                "HTTP session `{}` migration target must be a different worker",
                session.id
            ));
        }

        let snapshot = self.persistence_snapshot(session)?;
        let migrated = destination.replay_persistence_snapshot(snapshot)?;
        self.expire(session)?;
        Ok(VmHttpSessionWorkerMigration {
            session_id: session.id.clone(),
            source_route: source.route,
            destination_route: migrated.route.clone(),
            diagnostic: worker_migration_diagnostic(
                &session.id,
                &self.node_id,
                &destination.node_id,
                migrated.route.actor_pid,
            ),
        })
    }

    /// Reports whether a session can survive a VM source hot reload.
    #[cfg(test)]
    pub(crate) fn hot_reload_migration_compatibility_report(
        &mut self,
        session: &VmHttpSession,
        previous_generation: u64,
        active_generation: u64,
    ) -> Result<VmHttpSessionHotReloadMigrationReport, String> {
        if previous_generation == active_generation {
            return Err(format!(
                "HTTP session `{}` hot reload report requires distinct generations",
                session.id
            ));
        }
        let record = self.live_record(&session.id)?;
        let durable_table_entries = self.tables.entries(
            self.actors.processes(),
            record.value.actor,
            record.value.table,
        )?;
        Ok(VmHttpSessionHotReloadMigrationReport {
            session_id: record.id.clone(),
            previous_generation,
            active_generation,
            compatible: true,
            durable_table_entries: durable_table_entries.len(),
            durable_command_results: record.value.command_results.len(),
            transient_subscribers: record.value.live_template_subscribers.len(),
            diagnostic: hot_reload_migration_compatibility_diagnostic(
                &record.id,
                previous_generation,
                active_generation,
                durable_table_entries.len(),
                record.value.command_results.len(),
                record.value.live_template_subscribers.len(),
            ),
        })
    }

    /// Applies a stateful HTTP command once and replays duplicate command ids.
    #[cfg(test)]
    pub(crate) fn apply_idempotent_command(
        &mut self,
        session: &VmHttpSession,
        command_id: &str,
        command: impl FnOnce(&mut Self, &VmHttpSession) -> Result<ReplValue, String>,
    ) -> Result<VmHttpSessionCommandOutcome, String> {
        let command_id = normalize_command_id(command_id)?;
        let record = self.live_record(&session.id)?;
        if let Some(result) = record.value.command_results.get(command_id).cloned() {
            return Ok(VmHttpSessionCommandOutcome::Replayed(result));
        }

        let result = command(self, session)?;
        self.live_record(&session.id)?;
        self.sessions
            .value_mut(&session.id)
            .expect("live HTTP session should have a mutable session record")
            .command_results
            .insert(command_id.to_string(), result.clone());
        Ok(VmHttpSessionCommandOutcome::Applied(result))
    }

    /// Receives one queued session actor message for runtime integration tests.
    #[cfg(test)]
    pub(crate) fn receive_actor_message(
        &mut self,
        session: &VmHttpSession,
    ) -> Result<Option<ReplValue>, String> {
        self.receive_next_actor_payload(session)
    }

    #[cfg(test)]
    pub(crate) fn receive_next_actor_payload(
        &mut self,
        session: &VmHttpSession,
    ) -> Result<Option<ReplValue>, String> {
        let actor = self.live_record(&session.id)?.value.actor;
        match self.actors.receive_next_or_block(actor)? {
            VmActorReceive::Message(message) => Ok(Some(message.payload)),
            VmActorReceive::Blocked | VmActorReceive::Timeout => Ok(None),
        }
    }

    /// Registers a live-template subscriber owned by this session actor.
    #[cfg(test)]
    pub(crate) fn subscribe_live_template(
        &mut self,
        session: &VmHttpSession,
        subscriber_id: &str,
        transport: &str,
    ) -> Result<VmHttpSessionLiveTemplateSubscriber, String> {
        let subscriber = VmHttpSessionLiveTemplateSubscriber {
            id: normalize_live_template_subscriber_field(
                subscriber_id,
                "HTTP live-template subscriber id",
            )?
            .to_string(),
            transport: normalize_live_template_subscriber_field(
                transport,
                "HTTP live-template subscriber transport",
            )?
            .to_string(),
        };
        self.live_record(&session.id)?;
        self.sessions
            .value_mut(&session.id)
            .expect("live HTTP session should have mutable subscriber state")
            .live_template_subscribers
            .insert(subscriber.id.clone(), subscriber.clone());
        Ok(subscriber)
    }

    /// Registers a live-template subscriber only after capability admission passes.
    #[cfg(test)]
    pub(crate) fn subscribe_live_template_with_capability(
        &mut self,
        session: &VmHttpSession,
        subscriber_id: &str,
        transport: &str,
        required_capability: &str,
        granted_capabilities: &[&str],
    ) -> Result<VmHttpSessionLiveTemplateSubscriptionAuthorization, String> {
        let subscriber = VmHttpSessionLiveTemplateSubscriber {
            id: normalize_live_template_subscriber_field(
                subscriber_id,
                "HTTP live-template subscriber id",
            )?
            .to_string(),
            transport: normalize_live_template_subscriber_field(
                transport,
                "HTTP live-template subscriber transport",
            )?
            .to_string(),
        };
        let required_capability = normalize_live_template_subscriber_field(
            required_capability,
            "HTTP live-template subscriber capability",
        )?
        .to_string();
        let mut granted_capabilities = granted_capabilities
            .iter()
            .map(|capability| {
                normalize_live_template_subscriber_field(
                    capability,
                    "HTTP live-template granted capability",
                )
                .map(str::to_string)
            })
            .collect::<Result<Vec<_>, _>>()?;
        granted_capabilities.sort();
        granted_capabilities.dedup();
        if !granted_capabilities
            .iter()
            .any(|capability| capability == &required_capability)
        {
            return Err(live_template_subscriber_capability_diagnostic(
                &subscriber.id,
                &required_capability,
            ));
        }

        self.live_record(&session.id)?;
        self.sessions
            .value_mut(&session.id)
            .expect("live HTTP session should have mutable subscriber state")
            .live_template_subscribers
            .insert(subscriber.id.clone(), subscriber.clone());
        Ok(VmHttpSessionLiveTemplateSubscriptionAuthorization {
            diagnostic: live_template_subscriber_authorized_diagnostic(
                &subscriber.id,
                &required_capability,
            ),
            subscriber,
            required_capability,
            granted_capabilities,
        })
    }

    /// Removes a live-template subscriber from this session actor.
    #[cfg(test)]
    pub(crate) fn unsubscribe_live_template(
        &mut self,
        session: &VmHttpSession,
        subscriber_id: &str,
    ) -> Result<Option<VmHttpSessionLiveTemplateSubscriber>, String> {
        let subscriber_id = normalize_live_template_subscriber_field(
            subscriber_id,
            "HTTP live-template subscriber id",
        )?;
        self.live_record(&session.id)?;
        Ok(self
            .sessions
            .value_mut(&session.id)
            .expect("live HTTP session should have mutable subscriber state")
            .live_template_subscribers
            .remove(subscriber_id))
    }

    /// Lists live-template subscribers still attached to this session actor.
    #[cfg(test)]
    pub(crate) fn live_template_subscribers(
        &mut self,
        session: &VmHttpSession,
    ) -> Result<Vec<VmHttpSessionLiveTemplateSubscriber>, String> {
        let record = self.live_record(&session.id)?;
        Ok(record
            .value
            .live_template_subscribers
            .values()
            .cloned()
            .collect())
    }

    /// Binds a typed template to the VM actor state slot it renders.
    #[cfg(test)]
    pub(crate) fn bind_live_template_to_actor_state(
        &mut self,
        session: &VmHttpSession,
        template_id: &str,
        state_key: &str,
    ) -> Result<VmHttpSessionLiveTemplateActorBinding, String> {
        let template_id =
            normalize_live_template_subscriber_field(template_id, "HTTP live-template id")?
                .to_string();
        let state_key =
            normalize_live_template_subscriber_field(state_key, "HTTP live-template state key")?
                .to_string();
        let record = self.live_record(&session.id)?;
        let state_value = self.tables.lookup(
            self.actors.processes(),
            record.value.actor,
            record.value.table,
            &ReplValue::String(state_key.clone()),
        )?;

        Ok(VmHttpSessionLiveTemplateActorBinding {
            session_id: record.id.clone(),
            actor_pid: record.value.actor.as_u64(),
            table_id: record.value.table.as_u64(),
            template_id: template_id.clone(),
            state_key: state_key.clone(),
            state_value,
            state_version: record.value.state_version,
            live_template_subscriber_count: record.value.live_template_subscribers.len(),
            diagnostic: live_template_actor_binding_diagnostic(
                &record.id,
                &template_id,
                &state_key,
                record.value.actor.as_u64(),
            ),
        })
    }

    /// Captures a source-map-aware trace for one live-template subscription.
    #[cfg(test)]
    pub(crate) fn trace_live_template_subscription_with_source_map(
        &mut self,
        session: &VmHttpSession,
        subscriber_id: &str,
        template_id: &str,
        source_module: &str,
        source_line: u32,
        source_column: u32,
    ) -> Result<VmHttpSessionLiveTemplateSubscriptionTrace, String> {
        let subscriber_id = normalize_live_template_subscriber_field(
            subscriber_id,
            "HTTP live-template subscriber id",
        )?
        .to_string();
        let template_id =
            normalize_live_template_subscriber_field(template_id, "HTTP live-template id")?
                .to_string();
        let source_module = normalize_live_template_subscriber_field(
            source_module,
            "HTTP live-template source module",
        )?
        .to_string();
        validate_live_template_source_location(source_line, source_column)?;
        let record = self.live_record(&session.id)?;
        let subscriber = record
            .value
            .live_template_subscribers
            .get(&subscriber_id)
            .ok_or_else(|| {
                live_template_missing_subscriber_trace_diagnostic(&subscriber_id, &template_id)
            })?;

        Ok(VmHttpSessionLiveTemplateSubscriptionTrace {
            session_id: record.id.clone(),
            actor_pid: record.value.actor.as_u64(),
            subscriber_id: subscriber.id.clone(),
            transport: subscriber.transport.clone(),
            template_id: template_id.clone(),
            source_module: source_module.clone(),
            source_line,
            source_column,
            state_version: record.value.state_version,
            diagnostic: live_template_source_map_trace_diagnostic(
                &record.id,
                &subscriber.id,
                &template_id,
                &source_module,
                source_line,
                source_column,
            ),
        })
    }

    /// Applies one actor state update and fans a typed patch to all live subscribers.
    #[cfg(test)]
    pub(crate) fn fanout_live_template_state_update(
        &mut self,
        session: &VmHttpSession,
        expected_version: u64,
        patch_event: &str,
        source: &VmHttpSessionLiveTemplateSourceSpan,
        patch_payload: ReplValue,
        update: impl FnOnce(&mut Self, &VmHttpSession) -> Result<(), String>,
    ) -> Result<VmHttpSessionLiveTemplateStateFanout, String> {
        let patch_event = normalize_live_template_subscriber_field(
            patch_event,
            "HTTP live-template patch event",
        )?
        .to_string();
        validate_live_template_patch_payload(&patch_payload, source)?;
        let state_version = self.apply_state_update(session, expected_version, update)?;
        let state_version_value = i64::try_from(state_version)
            .map_err(|_| "HTTP live-template state version overflowed Int".to_string())?;
        let record = self.live_record(&session.id)?;
        let subscriber_events = record
            .value
            .live_template_subscribers
            .values()
            .map(|subscriber| VmHttpSessionLiveTemplateFanoutEvent {
                subscriber_id: subscriber.id.clone(),
                transport: subscriber.transport.clone(),
                event_id: live_template_state_patch_event_id(
                    &record.id,
                    state_version,
                    &subscriber.id,
                ),
                event_name: patch_event.clone(),
                payload: ReplValue::Tuple(vec![
                    ReplValue::Atom("live_template_state_update".to_string()),
                    ReplValue::String(patch_event.clone()),
                    ReplValue::Int(state_version_value),
                    patch_payload.clone(),
                ]),
            })
            .collect();
        Ok(VmHttpSessionLiveTemplateStateFanout {
            session_id: record.id,
            state_version,
            patch_event,
            subscriber_events,
        })
    }

    /// Rotates a session id while preserving actor and table state.
    pub(crate) fn rotate(
        &mut self,
        session: &VmHttpSession,
    ) -> Result<VmHttpSessionLookup, String> {
        self.rotate_for(session, self.ttl_ticks)
    }

    pub(crate) fn rotate_for(
        &mut self,
        session: &VmHttpSession,
        ttl_ticks: u64,
    ) -> Result<VmHttpSessionLookup, String> {
        let record = self
            .sessions
            .rotate(
                &session.id,
                ttl_ticks,
                &mut Resources {
                    actors: &mut self.actors,
                    tables: &mut self.tables,
                },
            )
            .map_err(|error| error.to_string())?;
        Ok(self.lookup_for_record(record))
    }

    /// Advances the package session clock deterministically in lifecycle tests.
    #[cfg(test)]
    pub(crate) fn advance_ticks(&mut self, ticks: u64) {
        self.sessions.advance_ticks(ticks);
    }

    #[cfg(test)]
    pub(crate) fn expire_due(&mut self) -> Result<Vec<String>, String> {
        self.sessions
            .expire_due(&mut Resources {
                actors: &mut self.actors,
                tables: &mut self.tables,
            })
            .map_err(|error| error.to_string())
    }

    pub(crate) fn expire_due_limit(
        &mut self,
        limit: usize,
    ) -> Result<Vec<String>, terlan_http_native::session_registry::SessionError> {
        self.sessions.expire_due_limit(
            &mut Resources {
                actors: &mut self.actors,
                tables: &mut self.tables,
            },
            limit,
        )
    }

    pub(crate) fn expire(&mut self, session: &VmHttpSession) -> Result<(), String> {
        self.sessions
            .expire(
                &session.id,
                &mut Resources {
                    actors: &mut self.actors,
                    tables: &mut self.tables,
                },
            )
            .map_err(|error| error.to_string())
    }

    /// Reports whether a managed session handle still names a live actor.
    pub(crate) fn is_live(&self, session: &VmHttpSession) -> bool {
        self.is_live_session(session.managed_id())
    }

    /// Returns runtime-inspection rows for live sessions.
    #[cfg(test)]
    pub(crate) fn snapshots(&self) -> Vec<VmHttpSessionSnapshot> {
        let table_lengths = self
            .tables
            .snapshots()
            .into_iter()
            .map(|snapshot| (snapshot.id, snapshot.len))
            .collect::<BTreeMap<_, _>>();
        self.sessions
            .entries()
            .values()
            .map(|record| VmHttpSessionSnapshot {
                session_id: record.id.clone(),
                actor_pid: record.value.actor.as_u64(),
                table_id: record.value.table.as_u64(),
                table_len: table_lengths.get(&record.value.table).copied().unwrap_or(0),
                live_template_subscriber_count: record.value.live_template_subscribers.len(),
                actor_mailbox_len: self
                    .actors
                    .processes()
                    .get(record.value.actor)
                    .map(|process| process.mailbox_len())
                    .unwrap_or(0),
                state_version: record.value.state_version,
                expires_at_tick: record.expires_at_tick,
                sticky_key: self.sticky_key(&record.id),
            })
            .collect()
    }

    #[cfg(test)]
    pub(in crate::runtime::vm::http_session) fn lookup_existing(
        &self,
        session_id: &str,
    ) -> Result<VmHttpSessionLookup, String> {
        let record = self
            .sessions
            .entries()
            .get(session_id)
            .cloned()
            .ok_or_else(|| {
                terlan_http_native::session_registry::SessionError::Stale(session_id.into())
                    .to_string()
            })?;
        Ok(self.lookup_for_record(record))
    }

    fn lookup_for_record(&self, record: VmHttpSessionRecord) -> VmHttpSessionLookup {
        VmHttpSessionLookup {
            session: VmHttpSession {
                id: record.id.clone(),
            },
            route: VmHttpSessionRoute {
                node_id: self.node_id.clone(),
                session_id: record.id.clone(),
                actor_pid: record.value.actor.as_u64(),
                sticky_key: self.sticky_key(&record.id),
            },
        }
    }

    #[inline(never)]
    pub(in crate::runtime::vm::http_session) fn live_record(
        &mut self,
        session_id: &str,
    ) -> Result<VmHttpSessionRecord, String> {
        self.sessions
            .live(
                session_id,
                &mut Resources {
                    actors: &mut self.actors,
                    tables: &mut self.tables,
                },
            )
            .map_err(|error| error.to_string())
    }

    fn is_live_session(&self, session_id: &str) -> bool {
        self.sessions.is_live(session_id, |resource| {
            resources::exit_reason(&self.actors, resource.actor).is_none()
        })
    }

    #[cfg(test)]
    pub(in crate::runtime::vm::http_session) fn session_actor_exit_reason(
        &self,
        actor: VmProcessId,
    ) -> Option<VmExitReason> {
        resources::exit_reason(&self.actors, actor)
    }

    fn sticky_key(&self, session_id: &str) -> String {
        format!("{}:{session_id}", self.node_id)
    }
}

/// Reads one string session value through the VM session runtime.
pub fn get(
    runtime: &mut VmHttpSessionRuntime,
    session: &VmHttpSession,
    key: &str,
) -> Result<Option<String>, terlan_runtime_abi::BoundaryError> {
    runtime
        .read(session, key)
        .map(|value| {
            value.map(|stored| match stored {
                ReplValue::String(value) => value,
                other => other.render(),
            })
        })
        .map_err(|error| {
            terlan_runtime_abi::BoundaryError::message(
                terlan_runtime_abi::ErrorDomain::VmRuntime,
                "read HTTP session value",
                error,
            )
        })
}
