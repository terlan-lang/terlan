//! Bounded shard-wide policy for scheduler work stealing.

use super::scheduler::VmSchedulerClass;
use super::scheduler_topology::VmSchedulerId;

const CLASS_COUNT: usize = 3;
const SERVICE_CYCLE: [VmSchedulerClass; 6] = [
    VmSchedulerClass::Priority,
    VmSchedulerClass::Priority,
    VmSchedulerClass::Normal,
    VmSchedulerClass::Priority,
    VmSchedulerClass::Normal,
    VmSchedulerClass::Background,
];

/// Bounds every work-stealing policy dimension used by one execution shard.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmWorkStealingConfig {
    local_service_budget: u32,
    steal_batch_size: usize,
    locality_threshold: usize,
    minimum_backoff_polls: u32,
    maximum_backoff_polls: u32,
    starvation_bounds: [u64; CLASS_COUNT],
}

impl VmWorkStealingConfig {
    /// Creates a validated policy with explicit finite bounds.
    pub(crate) fn new(
        local_service_budget: u32,
        steal_batch_size: usize,
        locality_threshold: usize,
        minimum_backoff_polls: u32,
        maximum_backoff_polls: u32,
        starvation_bounds: [u64; CLASS_COUNT],
    ) -> Result<Self, String> {
        if local_service_budget == 0 {
            return Err("error[vm.work_stealing.config]: local service budget is zero".to_string());
        }
        if steal_batch_size == 0 {
            return Err("error[vm.work_stealing.config]: steal batch size is zero".to_string());
        }
        if minimum_backoff_polls == 0 || maximum_backoff_polls < minimum_backoff_polls {
            return Err("error[vm.work_stealing.config]: invalid failed-steal backoff".to_string());
        }
        if starvation_bounds.contains(&0) {
            return Err("error[vm.work_stealing.config]: starvation bound is zero".to_string());
        }
        Ok(Self {
            local_service_budget,
            steal_batch_size,
            locality_threshold,
            minimum_backoff_polls,
            maximum_backoff_polls,
            starvation_bounds,
        })
    }
}

impl Default for VmWorkStealingConfig {
    /// Returns the bounded production defaults for one scheduler shard.
    fn default() -> Self {
        Self::new(8, 4, 1, 1, 64, [6, 12, 24])
            .expect("default work-stealing configuration is valid")
    }
}

/// Immutable queue evidence published by one scheduler owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmSchedulerWorkSnapshot {
    scheduler: VmSchedulerId,
    runnable: [usize; CLASS_COUNT],
    oldest_wait_ticks: [u64; CLASS_COUNT],
}

impl VmSchedulerWorkSnapshot {
    /// Creates one complete scheduler snapshot in priority/normal/background order.
    pub(crate) const fn new(
        scheduler: VmSchedulerId,
        runnable: [usize; CLASS_COUNT],
        oldest_wait_ticks: [u64; CLASS_COUNT],
    ) -> Self {
        Self {
            scheduler,
            runnable,
            oldest_wait_ticks,
        }
    }

    /// Returns the scheduler that published this evidence.
    pub(crate) const fn scheduler(self) -> VmSchedulerId {
        self.scheduler
    }

    /// Returns all runnable entries reported by this scheduler.
    pub(crate) fn runnable_total(self) -> usize {
        self.runnable.iter().copied().sum()
    }

    /// Returns runnable entries for one scheduling class.
    pub(crate) const fn runnable_in(self, class: VmSchedulerClass) -> usize {
        self.runnable[class_index(class)]
    }
}

/// Bounded transfer request selected by the shard-wide policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmStealPlan {
    thief: VmSchedulerId,
    victim: VmSchedulerId,
    class: VmSchedulerClass,
    maximum_actors: usize,
}

impl VmStealPlan {
    /// Returns the idle or fairness-assisting destination scheduler.
    pub(crate) const fn thief(self) -> VmSchedulerId {
        self.thief
    }

    /// Returns the source scheduler selected by deterministic rotation.
    pub(crate) const fn victim(self) -> VmSchedulerId {
        self.victim
    }

    /// Returns the queue class from which candidates may be claimed.
    pub(crate) const fn class(self) -> VmSchedulerClass {
        self.class
    }

    /// Returns the hard transfer batch bound.
    pub(crate) const fn maximum_actors(self) -> usize {
        self.maximum_actors
    }
}

/// One scheduler action selected without mutating actor state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmWorkDirective {
    /// Continue weighted service from the scheduler's local queues.
    ServeLocal(VmSchedulerClass),
    /// Ask one victim owner to publish up to a bounded actor batch.
    Steal(VmStealPlan),
    /// Delay another failed steal attempt for a bounded number of polls.
    Backoff(u32),
    /// Sleep after proving the shard has no currently runnable work.
    Sleep,
}

/// Stateful deterministic work-stealing policy for one scheduler pool.
#[derive(Debug)]
pub(crate) struct VmWorkStealingPolicy {
    config: VmWorkStealingConfig,
    width: usize,
    victim_cursor: Vec<usize>,
    service_cursor: Vec<usize>,
    local_services: Vec<u32>,
    failed_steals: Vec<u32>,
    backoff_remaining: Vec<u32>,
}

impl VmWorkStealingPolicy {
    /// Creates bounded per-scheduler policy state for one fixed topology.
    pub(crate) fn new(width: usize, config: VmWorkStealingConfig) -> Result<Self, String> {
        if width == 0 {
            return Err("error[vm.work_stealing.width]: scheduler width is zero".to_string());
        }
        Ok(Self {
            config,
            width,
            victim_cursor: vec![0; width],
            service_cursor: vec![0; width],
            local_services: vec![0; width],
            failed_steals: vec![0; width],
            backoff_remaining: vec![0; width],
        })
    }

    /// Selects one bounded scheduler action from a complete shard snapshot.
    pub(crate) fn decide(
        &mut self,
        thief: VmSchedulerId,
        snapshots: &[VmSchedulerWorkSnapshot],
    ) -> Result<VmWorkDirective, String> {
        let thief_index = self.validate_snapshots(thief, snapshots)?;
        let local = snapshots[thief_index];
        if self.backoff_remaining[thief_index] > 0 {
            self.backoff_remaining[thief_index] -= 1;
            return Ok(VmWorkDirective::Backoff(
                self.backoff_remaining[thief_index],
            ));
        }

        let remote_starved = snapshots.iter().enumerate().any(|(index, snapshot)| {
            index != thief_index && self.starved_class(*snapshot).is_some()
        });
        if local.runnable_total() > 0
            && self.local_services[thief_index] < self.config.local_service_budget
            && !remote_starved
        {
            self.local_services[thief_index] += 1;
            return Ok(VmWorkDirective::ServeLocal(
                self.next_service_class(thief_index, local),
            ));
        }

        if let Some(plan) = self.select_steal(thief_index, snapshots) {
            self.local_services[thief_index] = 0;
            return Ok(VmWorkDirective::Steal(plan));
        }
        if local.runnable_total() > 0 {
            self.local_services[thief_index] = 1;
            return Ok(VmWorkDirective::ServeLocal(
                self.next_service_class(thief_index, local),
            ));
        }
        Ok(VmWorkDirective::Sleep)
    }

    /// Records whether a selected steal published at least one actor transfer.
    pub(crate) fn record_steal_result(
        &mut self,
        thief: VmSchedulerId,
        transferred: usize,
    ) -> Result<(), String> {
        let index = self.scheduler_index(thief)?;
        if transferred > self.config.steal_batch_size {
            return Err(format!(
                "error[vm.work_stealing.batch]: transferred {transferred}, maximum {}",
                self.config.steal_batch_size
            ));
        }
        if transferred > 0 {
            self.failed_steals[index] = 0;
            self.backoff_remaining[index] = 0;
            return Ok(());
        }
        self.failed_steals[index] = self.failed_steals[index].saturating_add(1);
        let shift = self.failed_steals[index].saturating_sub(1).min(31);
        let multiplier = 1_u32.checked_shl(shift).unwrap_or(u32::MAX);
        self.backoff_remaining[index] = self
            .config
            .minimum_backoff_polls
            .saturating_mul(multiplier)
            .min(self.config.maximum_backoff_polls);
        Ok(())
    }

    /// Selects a victim using starvation urgency, load, and rotated tie-breaking.
    fn select_steal(
        &mut self,
        thief_index: usize,
        snapshots: &[VmSchedulerWorkSnapshot],
    ) -> Option<VmStealPlan> {
        let local_load = snapshots[thief_index].runnable_total();
        let start = self.victim_cursor[thief_index] % self.width;
        let mut selected: Option<(usize, bool, u64, usize, usize)> = None;
        for distance in 0..self.width {
            let index = (start + distance) % self.width;
            let snapshot = snapshots[index];
            if index == thief_index || snapshot.runnable_total() == 0 {
                continue;
            }
            let starved = self.starved_class(snapshot);
            let overloaded = snapshot.runnable_total()
                > local_load.saturating_add(self.config.locality_threshold);
            if local_load > 0 && starved.is_none() && !overloaded {
                continue;
            }
            let urgency = starved
                .map(|class| self.class_urgency(snapshot, class))
                .unwrap_or(0);
            let score = (
                index,
                starved.is_some(),
                urgency,
                snapshot.runnable_total(),
                usize::MAX - distance,
            );
            if selected.as_ref().is_none_or(|current| {
                (score.1, score.2, score.3, score.4) > (current.1, current.2, current.3, current.4)
            }) {
                selected = Some(score);
            }
        }
        let (victim_index, _, _, _, _) = selected?;
        self.victim_cursor[thief_index] = (victim_index + 1) % self.width;
        let victim = snapshots[victim_index];
        let class = self
            .starved_class(victim)
            .unwrap_or_else(|| highest_weighted_nonempty(victim));
        let maximum_actors = victim
            .runnable_in(class)
            .min(self.config.steal_batch_size)
            .max(1);
        Some(VmStealPlan {
            thief: snapshots[thief_index].scheduler(),
            victim: victim.scheduler(),
            class,
            maximum_actors,
        })
    }

    /// Returns the most overdue class, preferring lower classes on urgency ties.
    fn starved_class(&self, snapshot: VmSchedulerWorkSnapshot) -> Option<VmSchedulerClass> {
        classes()
            .into_iter()
            .filter(|class| snapshot.runnable_in(*class) > 0)
            .filter(|class| {
                snapshot.oldest_wait_ticks[class_index(*class)]
                    >= self.config.starvation_bounds[class_index(*class)]
            })
            .max_by_key(|class| (self.class_urgency(snapshot, *class), class_index(*class)))
    }

    /// Computes comparable fixed-point wait urgency without floating point.
    fn class_urgency(&self, snapshot: VmSchedulerWorkSnapshot, class: VmSchedulerClass) -> u64 {
        snapshot.oldest_wait_ticks[class_index(class)].saturating_mul(1_024)
            / self.config.starvation_bounds[class_index(class)]
    }

    /// Selects the next nonempty local class through the canonical 3:2:1 cycle.
    fn next_service_class(
        &mut self,
        scheduler_index: usize,
        snapshot: VmSchedulerWorkSnapshot,
    ) -> VmSchedulerClass {
        for _ in 0..SERVICE_CYCLE.len() {
            let cursor = self.service_cursor[scheduler_index];
            let class = SERVICE_CYCLE[cursor];
            self.service_cursor[scheduler_index] = (cursor + 1) % SERVICE_CYCLE.len();
            if snapshot.runnable_in(class) > 0 {
                return class;
            }
        }
        highest_weighted_nonempty(snapshot)
    }

    /// Validates one complete, ordered, duplicate-free scheduler snapshot set.
    fn validate_snapshots(
        &self,
        thief: VmSchedulerId,
        snapshots: &[VmSchedulerWorkSnapshot],
    ) -> Result<usize, String> {
        let thief_index = self.scheduler_index(thief)?;
        if snapshots.len() != self.width {
            return Err(format!(
                "error[vm.work_stealing.snapshot]: expected {} schedulers, found {}",
                self.width,
                snapshots.len()
            ));
        }
        for (index, snapshot) in snapshots.iter().enumerate() {
            if snapshot.scheduler().index() != index {
                return Err(format!(
                    "error[vm.work_stealing.snapshot]: scheduler {} occupies slot {index}",
                    snapshot.scheduler().index()
                ));
            }
        }
        Ok(thief_index)
    }

    /// Converts a stable scheduler identity to bounded policy storage.
    fn scheduler_index(&self, scheduler: VmSchedulerId) -> Result<usize, String> {
        let index = scheduler.index();
        if index >= self.width {
            return Err(format!(
                "error[vm.work_stealing.scheduler]: scheduler {index} is outside width {}",
                self.width
            ));
        }
        Ok(index)
    }
}

/// Returns scheduling classes in canonical priority order.
const fn classes() -> [VmSchedulerClass; CLASS_COUNT] {
    [
        VmSchedulerClass::Priority,
        VmSchedulerClass::Normal,
        VmSchedulerClass::Background,
    ]
}

/// Maps one scheduling class to snapshot array order.
const fn class_index(class: VmSchedulerClass) -> usize {
    match class {
        VmSchedulerClass::Priority => 0,
        VmSchedulerClass::Normal => 1,
        VmSchedulerClass::Background => 2,
    }
}

/// Selects the highest weighted class known to contain runnable work.
fn highest_weighted_nonempty(snapshot: VmSchedulerWorkSnapshot) -> VmSchedulerClass {
    classes()
        .into_iter()
        .find(|class| snapshot.runnable_in(*class) > 0)
        .expect("caller selected a nonempty scheduler snapshot")
}

#[cfg(test)]
#[path = "work_stealing_test.rs"]
mod work_stealing_test;
