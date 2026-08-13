//! Shared actor lifecycle and publication control for fixed schedulers.

use std::sync::RwLock;

use super::actor_directory::{
    VmActorDirectory, VmActorLifecycle, VmActorMigrationStamp, VmActorMutatorToken,
    VmActorPublication, VmMailboxWake,
};
use super::process::VmProcessId;
use super::scheduler_topology::{VmFixedActorRoute, VmSchedulerId};

/// One acquired actor execution authority detached from structural locks.
#[derive(Debug)]
pub(crate) struct VmFixedActorLease {
    route: VmFixedActorRoute,
    token: VmActorMutatorToken,
}

/// Linear authority to transfer one unowned actor between explicit schedulers.
#[derive(Debug)]
pub(crate) struct VmFixedActorMigrationTicket {
    source: VmFixedActorRoute,
    destination: VmFixedActorRoute,
    stamp: VmActorMigrationStamp,
    source_lifecycle: VmActorLifecycle,
}

impl VmFixedActorMigrationTicket {
    /// Returns the route that released the actor for migration.
    pub(crate) const fn source(&self) -> VmFixedActorRoute {
        self.source
    }

    /// Returns the only destination authorized by this ticket.
    pub(crate) const fn destination(&self) -> VmFixedActorRoute {
        self.destination
    }

    /// Returns the actor generation protected by this migration ticket.
    pub(crate) fn actor_generation(&self) -> u64 {
        self.stamp.handle().actor_generation()
    }

    /// Returns the owner generation that released the actor for migration.
    pub(crate) fn owner_generation(&self) -> u64 {
        self.stamp.owner_generation()
    }

    /// Duplicates authority only so tests can prove stale-ticket rejection.
    #[cfg(all(test, not(feature = "multicore-tsan-harness")))]
    fn duplicate_for_test(&self) -> Self {
        Self {
            source: self.source,
            destination: self.destination,
            stamp: self.stamp,
            source_lifecycle: self.source_lifecycle,
        }
    }
}

impl VmFixedActorLease {
    /// Returns the immutable route owned by this execution lease.
    pub(crate) const fn route(&self) -> VmFixedActorRoute {
        self.route
    }

    /// Returns the actor generation protected by this execution lease.
    pub(crate) fn actor_generation(&self) -> u64 {
        self.token.handle().actor_generation()
    }

    /// Returns the exclusive owner generation protected by this lease.
    pub(crate) fn owner_generation(&self) -> u64 {
        self.token.owner_generation()
    }
}

/// Shard-global directory shared by all fixed scheduler threads.
#[derive(Debug)]
pub(crate) struct VmFixedSchedulerControl<P> {
    directory: RwLock<VmActorDirectory<VmFixedActorRoute, P>>,
}

impl<P> Default for VmFixedSchedulerControl<P> {
    /// Creates an empty control plane with no registered actors.
    fn default() -> Self {
        Self {
            directory: RwLock::new(VmActorDirectory::default()),
        }
    }
}

impl<P> VmFixedSchedulerControl<P> {
    /// Registers one actor route and publishes its initial runnable state.
    pub(crate) fn register(&self, route: VmFixedActorRoute) -> Result<(), String> {
        let pid = route_pid(route)?;
        let mut directory = self.write_directory("register")?;
        directory
            .insert(pid, route)
            .map_err(|error| control_error("register", error))?;
        directory
            .mark_queued(pid)
            .map_err(|error| control_error("queue registered actor", error))
    }

    /// Resolves the current scheduler route for one incoming actor identity.
    #[cfg(test)]
    pub(crate) fn resolve_route(
        &self,
        actor_id: std::num::NonZeroU64,
    ) -> Result<VmFixedActorRoute, String> {
        let pid = VmProcessId::from_native_owner(actor_id.get())
            .map_err(|error| format!("error[vm.fixed_scheduler.route]: {error}"))?;
        self.read_directory("resolve route")?
            .get(pid)
            .copied()
            .ok_or_else(|| {
                format!("error[vm.fixed_scheduler.route]: actor {actor_id} is not registered")
            })
    }

    /// Acquires an actor only on its deterministic home scheduler.
    ///
    /// The structural read lock is released before this method returns. Actor
    /// execution therefore never occurs while a shard-global lock is held.
    pub(crate) fn acquire(
        &self,
        route: VmFixedActorRoute,
        scheduler: VmSchedulerId,
    ) -> Result<VmFixedActorLease, String> {
        if route.scheduler() != scheduler {
            return Err(format!(
                "error[vm.fixed_scheduler.owner]: actor {} belongs to scheduler {}, not {}",
                route.actor_id(),
                route.scheduler().index(),
                scheduler.index()
            ));
        }
        let pid = route_pid(route)?;
        let directory = self.read_directory("acquire")?;
        validate_route(&directory, route)?;
        let token = directory
            .acquire_mutator(pid, scheduler.owner_word().get())
            .map_err(|error| control_error("acquire", error))?;
        directory
            .activate_mailbox(pid)
            .map_err(|error| control_error("activate mailbox", error))?;
        Ok(VmFixedActorLease { route, token })
    }

    /// Publishes one complete cross-thread payload through the MC-3 mailbox.
    #[cfg(any(test, feature = "multicore-tsan-harness"))]
    pub(crate) fn publish(
        &self,
        route: VmFixedActorRoute,
        payload: P,
    ) -> Result<VmMailboxWake, String> {
        self.publish_identified(route, payload)
            .map(|(_, wake)| wake)
    }

    /// Publishes one payload while retaining its actor-local sequence identity.
    pub(crate) fn publish_identified(
        &self,
        route: VmFixedActorRoute,
        payload: P,
    ) -> Result<(VmActorPublication, VmMailboxWake), String> {
        let directory = self.read_directory("publish")?;
        validate_route(&directory, route)?;
        directory
            .publish_fragment(route_pid(route)?, payload)
            .map_err(|error| control_error("publish", error))
    }

    /// Drains complete payloads under the receiver's exact execution lease.
    #[cfg(any(test, feature = "multicore-tsan-harness"))]
    pub(crate) fn drain(&self, lease: &VmFixedActorLease) -> Result<Vec<P>, String> {
        self.drain_identified(lease)
            .map(|fragments| fragments.into_iter().map(|(_, payload)| payload).collect())
    }

    /// Drains payloads with their exact actor-generation publication identities.
    pub(crate) fn drain_identified(
        &self,
        lease: &VmFixedActorLease,
    ) -> Result<Vec<(VmActorPublication, P)>, String> {
        let directory = self.read_directory("drain")?;
        validate_route(&directory, lease.route)?;
        directory
            .drain_payloads(&lease.token)
            .map_err(|error| control_error("drain", error))
    }

    /// Releases execution to a runnable, parked, or terminal boundary.
    pub(crate) fn release(
        &self,
        lease: VmFixedActorLease,
        next: VmActorLifecycle,
    ) -> Result<VmActorLifecycle, String> {
        let directory = self.read_directory("release")?;
        validate_route(&directory, lease.route)?;
        directory
            .release_mutator(lease.token, next)
            .map_err(|error| control_error("release", error))
    }

    /// Publishes one yielded actor back to its scheduler's runnable queue.
    pub(crate) fn requeue_yielded(&self, route: VmFixedActorRoute) -> Result<(), String> {
        let directory = self.read_directory("requeue yielded actor")?;
        validate_route(&directory, route)?;
        directory
            .mark_queued(route_pid(route)?)
            .map_err(|error| control_error("requeue yielded actor", error))
    }

    /// Begins one explicit migration after execution released or queued the actor.
    pub(crate) fn begin_migration(
        &self,
        source: VmFixedActorRoute,
        destination: VmSchedulerId,
    ) -> Result<VmFixedActorMigrationTicket, String> {
        let destination = source.migrated_to(destination)?;
        let pid = route_pid(source)?;
        let mut directory = self.write_directory("begin migration")?;
        validate_route(&directory, source)?;
        let source_lifecycle = directory
            .lifecycle(pid)
            .map_err(|error| control_error("inspect migration source", error))?;
        directory
            .begin_migration(pid)
            .map_err(|error| control_error("begin migration", error))?;
        let stamp = directory
            .migration_stamp(pid)
            .map_err(|error| control_error("stamp migration", error))?;
        Ok(VmFixedActorMigrationTicket {
            source,
            destination,
            stamp,
            source_lifecycle,
        })
    }

    /// Publishes the new route and makes its destination scheduler runnable.
    pub(crate) fn complete_migration(
        &self,
        ticket: VmFixedActorMigrationTicket,
    ) -> Result<VmFixedActorRoute, String> {
        let source = ticket.source();
        let destination = ticket.destination();
        let pid = route_pid(source)?;
        let mut directory = self.write_directory("complete migration")?;
        validate_migration_ticket(&directory, &ticket)?;
        directory
            .finish_migration_as(pid, ticket.source_lifecycle)
            .map_err(|error| control_error("finish migration", error))?;
        let route = directory.get_mut_unowned(pid).ok_or_else(|| {
            "error[vm.fixed_scheduler.migration]: migrating route disappeared".to_string()
        })?;
        *route = destination;
        Ok(destination)
    }

    /// Restores the source route when transfer publication cannot complete.
    #[cfg(test)]
    pub(crate) fn abort_migration(
        &self,
        ticket: VmFixedActorMigrationTicket,
    ) -> Result<VmFixedActorRoute, String> {
        let source = ticket.source();
        let pid = route_pid(source)?;
        let mut directory = self.write_directory("abort migration")?;
        validate_migration_ticket(&directory, &ticket)?;
        directory
            .finish_migration_as(pid, ticket.source_lifecycle)
            .map_err(|error| control_error("abort migration", error))?;
        Ok(source)
    }

    /// Retires and reclaims one actor after terminal execution was released.
    pub(crate) fn reclaim(&self, route: VmFixedActorRoute) -> Result<(), String> {
        let pid = route_pid(route)?;
        let mut directory = self.write_directory("reclaim")?;
        validate_route(&directory, route)?;
        directory
            .mark_retired(pid)
            .map_err(|error| control_error("retire", error))?;
        let reclaimed = directory
            .reclaim(pid)
            .map_err(|error| control_error("reclaim", error))?;
        if reclaimed != route {
            return Err("error[vm.fixed_scheduler.route]: reclaimed route changed".to_string());
        }
        Ok(())
    }

    /// Discards a registered actor that never reached scheduler execution.
    pub(crate) fn discard(&self, route: VmFixedActorRoute) -> Result<(), String> {
        let pid = route_pid(route)?;
        let mut directory = self.write_directory("discard")?;
        validate_route(&directory, route)?;
        directory
            .mark_exiting(pid)
            .map_err(|error| control_error("discard exiting", error))?;
        directory
            .mark_retired(pid)
            .map_err(|error| control_error("discard retire", error))?;
        directory
            .reclaim(pid)
            .map(|_| ())
            .map_err(|error| control_error("discard reclaim", error))
    }

    /// Retires every unowned actor during orderly scheduler shutdown.
    ///
    /// The preflight pass rejects an executing actor before any route is
    /// changed. Shutdown therefore never guesses which thread owns mutable
    /// actor state and never leaves a partially reclaimed directory because
    /// one route was still executing.
    pub(crate) fn shutdown(&self) -> Result<usize, String> {
        let mut directory = self.write_directory("shutdown")?;
        let routes = directory.values().copied().collect::<Vec<_>>();
        for route in &routes {
            let lifecycle = directory
                .lifecycle(route_pid(*route)?)
                .map_err(|error| control_error("shutdown preflight", error))?;
            if lifecycle == VmActorLifecycle::Executing {
                return Err(format!(
                    "error[vm.fixed_scheduler.shutdown]: actor {} is still executing",
                    route.actor_id()
                ));
            }
            if lifecycle == VmActorLifecycle::Reclaimed {
                return Err(format!(
                    "error[vm.fixed_scheduler.shutdown]: actor {} remained indexed after reclaim",
                    route.actor_id()
                ));
            }
        }
        for route in &routes {
            let pid = route_pid(*route)?;
            let lifecycle = directory
                .lifecycle(pid)
                .map_err(|error| control_error("shutdown lifecycle", error))?;
            if matches!(
                lifecycle,
                VmActorLifecycle::Queued
                    | VmActorLifecycle::Yielding
                    | VmActorLifecycle::Parked
                    | VmActorLifecycle::Migrating
            ) {
                directory
                    .mark_exiting(pid)
                    .map_err(|error| control_error("shutdown exiting", error))?;
            }
            if lifecycle != VmActorLifecycle::Retired {
                directory
                    .mark_retired(pid)
                    .map_err(|error| control_error("shutdown retire", error))?;
            }
            directory
                .reclaim(pid)
                .map_err(|error| control_error("shutdown reclaim", error))?;
        }
        Ok(routes.len())
    }

    /// Returns one actor lifecycle for diagnostics and tests.
    #[cfg(all(test, not(feature = "multicore-tsan-harness")))]
    pub(crate) fn lifecycle(&self, route: VmFixedActorRoute) -> Result<VmActorLifecycle, String> {
        let directory = self.read_directory("lifecycle")?;
        validate_route(&directory, route)?;
        directory
            .lifecycle(route_pid(route)?)
            .map_err(|error| control_error("lifecycle", error))
    }

    /// Returns the stable shard-global transition stream.
    #[cfg(test)]
    pub(crate) fn transition_events(
        &self,
    ) -> Result<Vec<super::actor_directory::VmActorTransitionEvent>, String> {
        Ok(self
            .read_directory("transition events")?
            .transition_events())
    }

    /// Acquires the structural directory for immutable bounded operations.
    fn read_directory(
        &self,
        operation: &str,
    ) -> Result<std::sync::RwLockReadGuard<'_, VmActorDirectory<VmFixedActorRoute, P>>, String>
    {
        self.directory.read().map_err(|_| {
            format!("error[vm.fixed_scheduler.poisoned]: {operation} read lock poisoned")
        })
    }

    /// Acquires the structural directory only for registration or reclamation.
    fn write_directory(
        &self,
        operation: &str,
    ) -> Result<std::sync::RwLockWriteGuard<'_, VmActorDirectory<VmFixedActorRoute, P>>, String>
    {
        self.directory.write().map_err(|_| {
            format!("error[vm.fixed_scheduler.poisoned]: {operation} write lock poisoned")
        })
    }
}

/// Converts a shard-global route identity to the directory's process key.
fn route_pid(route: VmFixedActorRoute) -> Result<VmProcessId, String> {
    VmProcessId::from_native_owner(route.actor_id().get())
        .map_err(|error| format!("error[vm.fixed_scheduler.route]: {error}"))
}

/// Verifies that a directory slot still contains the exact fixed route.
fn validate_route<P>(
    directory: &VmActorDirectory<VmFixedActorRoute, P>,
    route: VmFixedActorRoute,
) -> Result<(), String> {
    match directory.get(route_pid(route)?) {
        Some(current) if *current == route => Ok(()),
        Some(_) => Err(format!(
            "error[vm.fixed_scheduler.route]: actor {} route changed",
            route.actor_id()
        )),
        None => Err(format!(
            "error[vm.fixed_scheduler.route]: actor {} is not registered",
            route.actor_id()
        )),
    }
}

/// Rejects stale, cross-actor, or ABA migration authority before route mutation.
fn validate_migration_ticket<P>(
    directory: &VmActorDirectory<VmFixedActorRoute, P>,
    ticket: &VmFixedActorMigrationTicket,
) -> Result<(), String> {
    if ticket.source().home_scheduler() != ticket.destination().home_scheduler() {
        return Err(format!(
            "error[vm.fixed_scheduler.migration_stale]: actor {} home scheduler changed",
            ticket.source.actor_id()
        ));
    }
    let pid = route_pid(ticket.source)?;
    if directory.get(pid) != Some(&ticket.source) {
        return Err(format!(
            "error[vm.fixed_scheduler.migration_stale]: actor {} source route changed",
            ticket.source.actor_id()
        ));
    }
    let observed = directory.migration_stamp(pid).map_err(|_| {
        format!(
            "error[vm.fixed_scheduler.migration_stale]: actor {} is no longer migrating",
            ticket.source.actor_id()
        )
    })?;
    if observed.handle() != ticket.stamp.handle()
        || observed.owner_generation() != ticket.stamp.owner_generation()
    {
        return Err(format!(
            "error[vm.fixed_scheduler.migration_stale]: actor {} migration generation changed",
            ticket.source.actor_id()
        ));
    }
    Ok(())
}

/// Adds stable control-plane context to an actor-directory rejection.
fn control_error(operation: &str, error: super::actor_directory::VmActorDirectoryError) -> String {
    format!("error[vm.fixed_scheduler.{operation}]: {error:?}")
}

#[cfg(all(test, not(feature = "multicore-tsan-harness")))]
#[cfg(test)]
#[path = "fixed_scheduler_control_test.rs"]
#[cfg(test)]
mod fixed_scheduler_control_test;

#[cfg(any(test, feature = "multicore-tsan-harness"))]
#[path = "fixed_scheduler_control_stress_test.rs"]
mod fixed_scheduler_control_stress_test;

#[cfg(feature = "multicore-tsan-harness")]
pub(super) fn run_multicore_sanitizer_stress() {
    fixed_scheduler_control_stress_test::run_bounded_stress();
}

#[cfg(feature = "multicore-tsan-harness")]
pub(super) fn run_multicore_sanitizer_seed() {
    fixed_scheduler_control_stress_test::run_seed_from_environment();
}

#[cfg(test)]
#[path = "fixed_scheduler_control_mtx_beam_suite_parity_test.rs"]
#[cfg(test)]
mod fixed_scheduler_control_mtx_beam_suite_parity_test;
