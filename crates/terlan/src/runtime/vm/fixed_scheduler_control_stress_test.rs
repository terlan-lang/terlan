use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::num::NonZeroU64;
use std::path::PathBuf;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;

use super::*;
use crate::runtime::vm::scheduler_topology::{VmFixedActorRoute, VmSchedulerTopology};

const CHILD_SEED_ENV: &str = "TERLAN_VM_MULTICORE_STRESS_CHILD_SEED";
const FORCE_HANG_ENV: &str = "TERLAN_VM_MULTICORE_STRESS_FORCE_HANG";
const REPORT_PATH_ENV: &str = "TERLAN_VM_MULTICORE_STRESS_OUTPUT";
const CHILD_TEST_NAME: &str = "runtime::vm::fixed_scheduler_control::fixed_scheduler_control_stress_test::seeded_multicore_memory_model_child";
const ACTORS: usize = 8;
const PRODUCERS: usize = 6;
const PUBLICATIONS_PER_PRODUCER: usize = 32;
const ROUNDS: usize = 6;
const WATCHDOG_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(5);
const STRESS_SEEDS: [u64; 8] = [
    0x0000_0000_0000_0001,
    0x0000_0000_0000_002a,
    0x0123_4567_89ab_cdef,
    0x1357_9bdf_2468_ace0,
    0x5555_aaaa_5555_aaaa,
    0x8000_0000_0000_0001,
    0xdead_beef_cafe_babe,
    0xffff_ffff_ffff_fffe,
];

/// Stable evidence emitted by the portable multicore stress gate.
#[derive(Debug, Serialize)]
struct MulticoreStressReport {
    /// Versioned report contract.
    schema: &'static str,
    /// Final gate decision.
    decision: &'static str,
    /// Rust target family that executed the stress.
    platform: String,
    /// Seeds completed without timeout or invariant failure.
    seeds: Vec<String>,
    /// Number of actors exercised by every seed.
    actors_per_seed: usize,
    /// Number of concurrent producers exercised by every seed.
    producers_per_seed: usize,
    /// Number of complete publication and migration rounds.
    rounds_per_seed: usize,
    /// Number of publications emitted by each producer in every round.
    publications_per_producer: usize,
    /// Maximum wall-clock duration admitted for one child seed.
    watchdog_timeout_millis: u128,
}

/// Result of one watchdog-bounded child execution.
#[derive(Debug)]
enum ChildOutcome {
    /// The child exited before its deadline.
    Exited(ExitStatus),
    /// The child exceeded its deadline and was killed.
    TimedOut,
}

/// Small deterministic generator used only to vary stress interleavings.
#[derive(Clone, Copy, Debug)]
struct StressGenerator {
    state: u64,
}

impl StressGenerator {
    /// Creates a generator whose all-zero seed cannot become a fixed point.
    fn new(seed: u64) -> Self {
        Self { state: seed.max(1) }
    }

    /// Returns the next xorshift64 value.
    fn next(&mut self) -> u64 {
        let mut value = self.state;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.state = value;
        value
    }

    /// Introduces a deterministic bounded scheduling perturbation.
    fn perturb(&mut self) {
        match self.next() & 3 {
            0 => thread::yield_now(),
            1 => thread::sleep(Duration::from_micros(1)),
            _ => {}
        }
    }
}

/// Returns one actor route from a stable one-based actor index.
fn actor_route(topology: &VmSchedulerTopology, actor: usize) -> VmFixedActorRoute {
    topology.route(NonZeroU64::new(actor as u64 + 1).expect("nonzero actor identity"))
}

/// Encodes one publication into a globally unique stress payload.
fn payload(round: usize, producer: usize, item: usize, actor: usize) -> u64 {
    ((round as u64) << 48) | ((producer as u64) << 40) | ((item as u64) << 24) | actor as u64
}

/// Runs one concurrent publication round and returns its expected payload set.
fn publish_round(
    control: &Arc<VmFixedSchedulerControl<u64>>,
    routes: &[VmFixedActorRoute],
    seed: u64,
    round: usize,
) -> BTreeSet<u64> {
    let barrier = Arc::new(Barrier::new(PRODUCERS + 1));
    let workers = (0..PRODUCERS)
        .map(|producer| {
            let control = Arc::clone(control);
            let barrier = Arc::clone(&barrier);
            let routes = routes.to_vec();
            thread::spawn(move || {
                let mut generator =
                    StressGenerator::new(seed ^ ((round as u64 + 1) << 32) ^ (producer as u64 + 1));
                let mut published = Vec::with_capacity(PUBLICATIONS_PER_PRODUCER);
                barrier.wait();
                for item in 0..PUBLICATIONS_PER_PRODUCER {
                    generator.perturb();
                    let actor = (item + producer + generator.next() as usize) % routes.len();
                    let value = payload(round, producer, item, actor);
                    control
                        .publish(routes[actor], value)
                        .expect("concurrent publication must succeed");
                    published.push(value);
                }
                published
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    workers
        .into_iter()
        .flat_map(|worker| worker.join().expect("publication worker must not panic"))
        .collect()
}

/// Proves only one scheduler can hold an actor mutator lease at a time.
fn contend_for_owned_actor(control: &Arc<VmFixedSchedulerControl<u64>>, route: VmFixedActorRoute) {
    let lease = control
        .acquire(route, route.scheduler())
        .expect("owner must acquire queued actor");
    let barrier = Arc::new(Barrier::new(4));
    let contenders = (0..3)
        .map(|_| {
            let control = Arc::clone(control);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                control.acquire(route, route.scheduler()).is_err()
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    for contender in contenders {
        assert!(
            contender
                .join()
                .expect("ownership contender must not panic"),
            "duplicate actor ownership was admitted"
        );
    }
    control
        .release(lease, VmActorLifecycle::Yielding)
        .expect("owner must release actor");
    control
        .requeue_yielded(route)
        .expect("yielded actor must return to its scheduler queue");
}

/// Drains every actor and proves complete, unique publication delivery.
fn drain_round(
    control: &Arc<VmFixedSchedulerControl<u64>>,
    routes: &[VmFixedActorRoute],
    expected: BTreeSet<u64>,
) {
    let mut observed = BTreeSet::new();
    for route in routes {
        let lease = control
            .acquire(*route, route.scheduler())
            .expect("woken actor must be queued");
        for payload in control.drain(&lease).expect("mailbox drain must succeed") {
            assert!(observed.insert(payload), "duplicate payload {payload}");
        }
        control
            .release(lease, VmActorLifecycle::Parked)
            .expect("drained actor must park");
    }
    assert_eq!(observed, expected, "mailbox publication set changed");
}

/// Migrates selected parked actors while preserving their immutable home.
fn migrate_routes(
    control: &Arc<VmFixedSchedulerControl<u64>>,
    topology: &VmSchedulerTopology,
    routes: &mut [VmFixedActorRoute],
    generator: &mut StressGenerator,
) {
    for route in routes {
        generator.perturb();
        if generator.next() & 1 == 0 {
            continue;
        }
        let destination_index = (route.scheduler().index() + 1) % topology.width();
        let destination = topology
            .schedulers()
            .nth(destination_index)
            .expect("destination scheduler must exist");
        let home = route.home_scheduler();
        let ticket = control
            .begin_migration(*route, destination)
            .expect("parked actor migration must begin");
        *route = control
            .complete_migration(ticket)
            .expect("actor migration must complete");
        assert_eq!(route.home_scheduler(), home);
        assert_eq!(route.scheduler(), destination);
    }
}

/// Executes one deterministic multicore stress seed to terminal reclamation.
fn run_seed(seed: u64) {
    let topology = VmSchedulerTopology::new(4).expect("stress topology");
    let control = Arc::new(VmFixedSchedulerControl::<u64>::default());
    let mut routes = (0..ACTORS)
        .map(|actor| actor_route(&topology, actor))
        .collect::<Vec<_>>();
    for route in &routes {
        control.register(*route).expect("register stress actor");
        let lease = control
            .acquire(*route, route.scheduler())
            .expect("initial actor acquisition");
        control
            .release(lease, VmActorLifecycle::Parked)
            .expect("initial actor park");
    }

    let mut generator = StressGenerator::new(seed);
    for round in 0..ROUNDS {
        let expected = publish_round(&control, &routes, seed, round);
        contend_for_owned_actor(&control, routes[round % routes.len()]);
        drain_round(&control, &routes, expected);
        migrate_routes(&control, &topology, &mut routes, &mut generator);
    }

    for route in routes {
        assert_eq!(
            control.publish(route, u64::MAX).expect("terminal wake"),
            VmMailboxWake::Enqueue
        );
        let lease = control
            .acquire(route, route.scheduler())
            .expect("terminal actor acquisition");
        assert_eq!(
            control.drain(&lease).expect("terminal mailbox drain"),
            vec![u64::MAX]
        );
        control
            .release(lease, VmActorLifecycle::Exiting)
            .expect("terminal actor release");
        control.reclaim(route).expect("terminal actor reclaim");
    }
    assert_eq!(control.shutdown().expect("empty final shutdown"), 0);
}

/// Waits for one child without permitting a deadlock to hang the gate.
fn wait_with_watchdog(
    child: &mut std::process::Child,
    timeout: Duration,
) -> std::io::Result<ChildOutcome> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(ChildOutcome::Exited(status));
        }
        if Instant::now() >= deadline {
            if let Err(error) = child.kill() {
                if child.try_wait()?.is_none() {
                    return Err(error);
                }
            }
            child.wait()?;
            return Ok(ChildOutcome::TimedOut);
        }
        thread::sleep(POLL_INTERVAL);
    }
}

/// Executes one seed in an isolated test process under the deadlock watchdog.
fn run_seed_child(seed: u64) -> Result<(), String> {
    let mut child = Command::new(env::current_exe().map_err(|error| error.to_string())?)
        .args([
            "--ignored",
            "--exact",
            CHILD_TEST_NAME,
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_SEED_ENV, seed.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| format!("could not spawn stress seed {seed:#018x}: {error}"))?;
    match wait_with_watchdog(&mut child, WATCHDOG_TIMEOUT).map_err(|error| error.to_string())? {
        ChildOutcome::Exited(status) if status.success() => Ok(()),
        ChildOutcome::Exited(status) => {
            Err(format!("stress seed {seed:#018x} exited with {status}"))
        }
        ChildOutcome::TimedOut => Err(format!(
            "stress seed {seed:#018x} exceeded {} ms",
            WATCHDOG_TIMEOUT.as_millis()
        )),
    }
}

/// Returns the portable stress report path selected by the quality gate.
fn report_path() -> PathBuf {
    env::var_os(REPORT_PATH_ENV).map_or_else(
        || {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/quality/vm-multicore-memory-model.json")
        },
        PathBuf::from,
    )
}

/// Writes stable evidence after every configured seed has completed.
fn write_report() {
    let path = report_path();
    fs::create_dir_all(path.parent().expect("stress report parent"))
        .expect("create stress report directory");
    let report = MulticoreStressReport {
        schema: "terlan.vm-multicore-memory-model.v1",
        decision: "pass",
        platform: format!("{}-{}", env::consts::OS, env::consts::ARCH),
        seeds: STRESS_SEEDS
            .into_iter()
            .map(|seed| format!("{seed:#018x}"))
            .collect(),
        actors_per_seed: ACTORS,
        producers_per_seed: PRODUCERS,
        rounds_per_seed: ROUNDS,
        publications_per_producer: PUBLICATIONS_PER_PRODUCER,
        watchdog_timeout_millis: WATCHDOG_TIMEOUT.as_millis(),
    };
    let bytes = serde_json::to_vec_pretty(&report).expect("serialize stress report");
    fs::write(path, [bytes, vec![b'\n']].concat()).expect("write stress report");
}

#[test]
#[ignore = "launched with an explicit seed by the bounded parent test"]
/// Executes one isolated stress seed on behalf of the watchdog parent.
fn seeded_multicore_memory_model_child() {
    if env::var_os(FORCE_HANG_ENV).is_some() {
        thread::sleep(Duration::from_secs(60));
        return;
    }
    let seed = env::var(CHILD_SEED_ENV)
        .expect("watchdog child requires an explicit stress seed")
        .parse::<u64>()
        .expect("stress seed must be an unsigned integer");
    run_seed(seed);
}

#[test]
/// Runs every recorded seed without allowing one deadlock to hang the suite.
fn bounded_seeded_multicore_memory_model_has_deadlock_watchdog() {
    for seed in STRESS_SEEDS {
        run_seed_child(seed).unwrap_or_else(|error| panic!("{error}"));
    }
    write_report();
}

#[test]
/// Proves the watchdog kills and reaps an intentionally stuck child process.
fn deadlock_watchdog_terminates_stuck_child() {
    let mut child = Command::new(env::current_exe().expect("current test executable"))
        .args([
            "--ignored",
            "--exact",
            CHILD_TEST_NAME,
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_SEED_ENV, "1")
        .env(FORCE_HANG_ENV, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn intentionally stuck child");
    assert!(matches!(
        wait_with_watchdog(&mut child, Duration::from_millis(50))
            .expect("watch intentionally stuck child"),
        ChildOutcome::TimedOut
    ));
    assert!(
        child.try_wait().expect("inspect reaped child").is_some(),
        "watchdog left its killed child unreaped"
    );
}
