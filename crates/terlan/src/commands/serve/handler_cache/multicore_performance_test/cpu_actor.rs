//! CPU-bound generated actor scaling evidence.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::time::Instant;

use serde::Serialize;

use crate::runtime::vm::http_session::{VmHttpSessionRuntime, VmHttpSessionService};
use crate::runtime::vm::scheduler_topology::VmSchedulerTopology;
use crate::runtime::vm::ReplValue;

use super::super::AotHandlerGeneration;
use super::{timing_distribution, TimingDistribution, WARMUP_SAMPLE_COUNT};

/// Number of deterministic integer-mixing iterations per generated actor.
pub(super) const CPU_ITERATIONS_PER_ACTOR: usize = 200_000;
/// Independent batch samples retained for CPU scaling confidence.
pub(super) const CPU_SAMPLE_COUNT: usize = 31;
const CONFIDENCE_RESAMPLES: usize = 4_096;
const CONFIDENCE_SEED: u64 = 0x5445_524c_414e_4d43;

/// CPU-bound timing and throughput at one fixed scheduler width.
#[derive(Clone, Debug, Serialize)]
pub(super) struct CpuBoundWidthMeasurement {
    /// Requested fixed scheduler count.
    pub(super) requested_schedulers: usize,
    /// Number of independent measured batches.
    pub(super) samples: usize,
    /// Generated actor executions in each batch.
    pub(super) actors_per_sample: usize,
    /// Median generated actor executions per second.
    pub(super) median_actors_per_second: u128,
    /// Maximum overlapping scheduler-owner execution intervals.
    pub(super) maximum_simultaneously_active_schedulers: usize,
    /// Distinct fixed scheduler owner thread names.
    pub(super) distinct_scheduler_owner_threads: Vec<String>,
    /// Raw batch durations retained for confidence reconstruction.
    pub(super) sample_durations_ns: Vec<u128>,
    /// Timing distribution across measured batches.
    pub(super) timing: TimingDistribution,
}

/// Deterministic nonparametric confidence interval for two-scheduler speedup.
#[derive(Clone, Debug, Serialize)]
pub(super) struct CpuBoundSpeedupConfidence {
    /// Width-two median throughput divided by width-one median throughput.
    pub(super) median_speedup_ratio: f64,
    /// Confidence level represented by the interval.
    pub(super) confidence_level: f64,
    /// Lower percentile of deterministic bootstrap median ratios.
    pub(super) lower_bound: f64,
    /// Upper percentile of deterministic bootstrap median ratios.
    pub(super) upper_bound: f64,
    /// Number of deterministic bootstrap resamples.
    pub(super) resamples: usize,
    /// Fixed seed used to make the confidence artifact reproducible.
    pub(super) seed: u64,
    /// Auditable interval construction method.
    pub(super) method: &'static str,
}

/// CPU-bound generated actor evidence before release policy enforcement.
#[derive(Clone, Debug, Serialize)]
pub(super) struct CpuBoundActorEvidence {
    /// Stable generated export used by every width.
    pub(super) export: &'static str,
    /// Integer mixing iterations performed by every actor.
    pub(super) iterations_per_actor: usize,
    /// Cold executions completed and discarded at every scheduler width.
    pub(super) warmup_samples_per_width: usize,
    /// Width measurements in requested order.
    pub(super) widths: Vec<CpuBoundWidthMeasurement>,
    /// Width-one to width-two scaling and confidence evidence.
    pub(super) width_one_to_two: CpuBoundSpeedupConfidence,
}

/// Runs identical CPU-bound generated actors on each fixed scheduler owner.
pub(super) fn measure_cpu_bound_actor(
    image: &Path,
    samples: usize,
    widths: &[usize],
) -> Result<CpuBoundActorEvidence, String> {
    let width_measurements = widths
        .iter()
        .copied()
        .map(|width| measure_width(image, width, samples))
        .collect::<Result<Vec<_>, _>>()?;
    let width_one = required_width(&width_measurements, 1)?;
    let width_two = required_width(&width_measurements, 2)?;
    let width_one_to_two = speedup_confidence(width_one, width_two)?;
    Ok(CpuBoundActorEvidence {
        export: "app.MulticoreBenchmark.cpu_bound",
        iterations_per_actor: CPU_ITERATIONS_PER_ACTOR,
        warmup_samples_per_width: WARMUP_SAMPLE_COUNT,
        widths: width_measurements,
        width_one_to_two,
    })
}

impl CpuBoundActorEvidence {
    /// Returns the exact width measurement required by release policy.
    pub(super) fn width(&self, width: usize) -> Result<&CpuBoundWidthMeasurement, String> {
        required_width(&self.widths, width)
    }
}

/// Measures one CPU-bound actor per fixed scheduler owner.
fn measure_width(
    image: &Path,
    width: usize,
    samples: usize,
) -> Result<CpuBoundWidthMeasurement, String> {
    let sessions = VmHttpSessionService::new(VmHttpSessionRuntime::new(
        "terlc-multicore-cpu-benchmark",
        86_400,
    )?);
    let generation = AotHandlerGeneration::load_with_shard_count(image, sessions, width)?;
    let topology = VmSchedulerTopology::new(width)?;
    let mut durations = Vec::with_capacity(samples);
    let mut maximum_active = 0;
    let mut owner_threads = BTreeSet::new();
    let total_samples = WARMUP_SAMPLE_COUNT
        .checked_add(samples)
        .ok_or_else(|| "CPU-bound benchmark sample count overflow".to_string())?;
    for sample in 0..total_samples {
        let routes = topology
            .schedulers()
            .map(|scheduler| generation.route_new_actor_on(scheduler))
            .collect::<Result<Vec<_>, _>>()?;
        let barrier = Arc::new(Barrier::new(width));
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let started = Instant::now();
        let results = std::thread::scope(|scope| {
            routes
                .iter()
                .copied()
                .enumerate()
                .map(|(lane, route)| {
                    let owner = generation.shard(route.scheduler().index())?;
                    let barrier = Arc::clone(&barrier);
                    let active = Arc::clone(&active);
                    let maximum = Arc::clone(&maximum);
                    let seed = cpu_seed(sample, lane);
                    Ok(scope.spawn(move || {
                        owner
                            .probe_execution_with_args(
                                route,
                                "app.MulticoreBenchmark.cpu_bound".to_string(),
                                vec![ReplValue::Int(seed)],
                                barrier,
                                active,
                                maximum,
                            )
                            .map(|(value, thread)| (seed, value, thread))
                    }))
                })
                .collect::<Result<Vec<_>, String>>()
                .map(|joins| {
                    joins
                        .into_iter()
                        .map(|join| {
                            join.join().map_err(|_| {
                                "CPU-bound benchmark client thread panicked".to_string()
                            })?
                        })
                        .collect::<Result<Vec<_>, String>>()
                })
        });
        let elapsed = started.elapsed().as_nanos();
        if sample >= WARMUP_SAMPLE_COUNT {
            durations.push(elapsed);
        }
        maximum_active = maximum_active.max(maximum.load(Ordering::SeqCst));
        for route in &routes {
            generation.release_actor_route(route.scheduler().index());
        }
        for (seed, value, owner_thread) in results?? {
            let expected = cpu_result(seed);
            if value != ReplValue::Int(expected) {
                return Err(format!(
                    "CPU-bound actor returned {value:?}, expected {expected}"
                ));
            }
            owner_threads.insert(owner_thread);
        }
    }
    let timing = timing_distribution(&durations)?;
    let median_actors_per_second = (width as u128)
        .saturating_mul(1_000_000_000)
        .checked_div(timing.median_ns.max(1))
        .unwrap_or(0);
    Ok(CpuBoundWidthMeasurement {
        requested_schedulers: width,
        samples,
        actors_per_sample: width,
        median_actors_per_second,
        maximum_simultaneously_active_schedulers: maximum_active,
        distinct_scheduler_owner_threads: owner_threads.into_iter().collect(),
        sample_durations_ns: durations,
        timing,
    })
}

/// Returns a stable positive runtime seed for one sample and lane.
fn cpu_seed(sample: usize, lane: usize) -> i64 {
    17 + sample as i64 * 31 + lane as i64 * 7
}

/// Reproduces the generated actor result for correctness validation.
pub(super) fn cpu_result(mut value: i64) -> i64 {
    for _ in 0..CPU_ITERATIONS_PER_ACTOR {
        value = (value * 1_664_525 + 1_013_904_223) % 2_147_483_647;
    }
    value
}

/// Finds one unique required scheduler width.
fn required_width(
    widths: &[CpuBoundWidthMeasurement],
    requested: usize,
) -> Result<&CpuBoundWidthMeasurement, String> {
    widths
        .iter()
        .find(|measurement| measurement.requested_schedulers == requested)
        .ok_or_else(|| format!("CPU-bound benchmark omitted scheduler width {requested}"))
}

/// Builds a deterministic independent-sample bootstrap confidence interval.
pub(super) fn speedup_confidence(
    width_one: &CpuBoundWidthMeasurement,
    width_two: &CpuBoundWidthMeasurement,
) -> Result<CpuBoundSpeedupConfidence, String> {
    if width_one.sample_durations_ns.is_empty() || width_two.sample_durations_ns.is_empty() {
        return Err("CPU-bound confidence requires nonempty width samples".to_string());
    }
    let median_speedup_ratio = width_two.median_actors_per_second as f64
        / width_one.median_actors_per_second.max(1) as f64;
    let one_throughputs = throughput_samples(width_one);
    let two_throughputs = throughput_samples(width_two);
    let mut generator = DeterministicGenerator::new(CONFIDENCE_SEED);
    let mut ratios = Vec::with_capacity(CONFIDENCE_RESAMPLES);
    for _ in 0..CONFIDENCE_RESAMPLES {
        let one = bootstrap_median(&one_throughputs, &mut generator);
        let two = bootstrap_median(&two_throughputs, &mut generator);
        ratios.push(two / one.max(f64::MIN_POSITIVE));
    }
    ratios.sort_by(f64::total_cmp);
    Ok(CpuBoundSpeedupConfidence {
        median_speedup_ratio,
        confidence_level: 0.95,
        lower_bound: float_percentile(&ratios, 0.025),
        upper_bound: float_percentile(&ratios, 0.975),
        resamples: CONFIDENCE_RESAMPLES,
        seed: CONFIDENCE_SEED,
        method: "deterministic-independent-median-bootstrap-percentile",
    })
}

/// Converts raw batch durations into actor-throughput samples.
fn throughput_samples(measurement: &CpuBoundWidthMeasurement) -> Vec<f64> {
    measurement
        .sample_durations_ns
        .iter()
        .map(|duration| {
            measurement.actors_per_sample as f64 * 1_000_000_000.0 / (*duration).max(1) as f64
        })
        .collect()
}

/// Returns the median of one deterministic bootstrap resample.
fn bootstrap_median(samples: &[f64], generator: &mut DeterministicGenerator) -> f64 {
    let mut resample = (0..samples.len())
        .map(|_| samples[generator.index(samples.len())])
        .collect::<Vec<_>>();
    resample.sort_by(f64::total_cmp);
    let middle = resample.len() / 2;
    if resample.len() % 2 == 0 {
        (resample[middle - 1] + resample[middle]) / 2.0
    } else {
        resample[middle]
    }
}

/// Returns one bounded percentile from sorted finite floating-point samples.
fn float_percentile(sorted: &[f64], percentile: f64) -> f64 {
    let index = ((sorted.len() - 1) as f64 * percentile).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

/// Small reproducible generator used only for bootstrap index selection.
struct DeterministicGenerator {
    state: u64,
}

impl DeterministicGenerator {
    /// Creates a generator with one fixed nonzero state.
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Returns one index in `0..upper`.
    fn index(&mut self, upper: usize) -> usize {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        (self.state as usize) % upper
    }
}
