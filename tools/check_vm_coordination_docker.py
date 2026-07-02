#!/usr/bin/env python3
"""Validate the Docker network simulation harness contract for VM coordination."""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
from dataclasses import dataclass
from typing import Iterable


REQUIRED_SCENARIO_NAMES = {
    "latency",
    "jitter",
    "packet_loss",
    "disconnect",
    "reconnect",
    "asymmetric_partition",
    "stale_epoch",
}


@dataclass(frozen=True)
class NetworkScenario:
    name: str
    description: str
    latency_ms: int = 0
    jitter_ms: int = 0
    drop_every: int = 0
    disconnect_first: bool = False
    reconnect_after_drop: bool = False
    asymmetric_partition: bool = False
    stale_epoch_delta: int = 0


@dataclass(frozen=True)
class VmPeer:
    node_id: str
    epoch: int


@dataclass(frozen=True)
class SimulatedEnvelope:
    trace_id: str
    from_node_id: str
    to_node_id: str
    epoch: int
    payload: str


@dataclass(frozen=True)
class SimulationResult:
    scenario: str
    delivered: int
    dropped: int
    rejected_stale_epoch: int
    reconnected: bool
    max_latency_ms: int


SCENARIOS = (
    NetworkScenario(
        "latency",
        "adds fixed one-way delay between VM peers",
        latency_ms=50,
    ),
    NetworkScenario(
        "jitter",
        "adds variable delay around the baseline latency",
        latency_ms=20,
        jitter_ms=15,
    ),
    NetworkScenario(
        "packet_loss",
        "drops a controlled percentage of peer traffic",
        drop_every=2,
    ),
    NetworkScenario(
        "disconnect",
        "cuts one peer off from the coordination network",
        disconnect_first=True,
    ),
    NetworkScenario(
        "reconnect",
        "restores a disconnected peer and validates epoch handling",
        disconnect_first=True,
        reconnect_after_drop=True,
    ),
    NetworkScenario(
        "asymmetric_partition",
        "allows one direction of traffic while blocking the opposite direction",
        asymmetric_partition=True,
    ),
    NetworkScenario(
        "stale_epoch",
        "rejects envelopes from a stale sender epoch",
        stale_epoch_delta=-1,
    ),
)


def fail(message: str) -> int:
    print(f"error[vm_coordination_docker]: {message}", file=sys.stderr)
    return 1


def skip(message: str) -> int:
    print(f"vm_coordination_docker_skipped: {message}")
    return 0


def docker_compose_command() -> list[str] | None:
    docker = shutil.which("docker")
    if docker is not None:
        result = subprocess.run(
            [docker, "compose", "version"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
        if result.returncode == 0:
            return [docker, "compose"]

    docker_compose = shutil.which("docker-compose")
    if docker_compose is not None:
        return [docker_compose]

    return None


def validate_required_scenarios(scenarios: Iterable[NetworkScenario]) -> int:
    names = {scenario.name for scenario in scenarios}
    missing = sorted(REQUIRED_SCENARIO_NAMES - names)
    if missing:
        return fail("missing required scenarios: " + ", ".join(missing))
    return 0


def simulate_scenario(scenario: NetworkScenario) -> SimulationResult:
    """Runs a deterministic two-peer VM coordination simulation."""

    peer_a = VmPeer("vm-a-node", 7)
    peer_b = VmPeer("vm-b-node", 7)
    delivered = 0
    dropped = 0
    rejected_stale_epoch = 0
    max_latency_ms = 0
    connected = not scenario.disconnect_first
    reconnected = False

    for index in range(1, 5):
        from_peer = peer_a if index % 2 == 1 else peer_b
        to_peer = peer_b if index % 2 == 1 else peer_a
        envelope = SimulatedEnvelope(
            trace_id=f"trace-{scenario.name}-{index}",
            from_node_id=from_peer.node_id,
            to_node_id=to_peer.node_id,
            epoch=from_peer.epoch + scenario.stale_epoch_delta,
            payload=f"message-{index}",
        )

        if scenario.reconnect_after_drop and index == 2:
            connected = True
            reconnected = True

        if not connected:
            dropped += 1
            continue

        if scenario.asymmetric_partition and envelope.from_node_id == peer_b.node_id:
            dropped += 1
            continue

        if scenario.drop_every and index % scenario.drop_every == 0:
            dropped += 1
            continue

        if envelope.epoch < to_peer.epoch:
            rejected_stale_epoch += 1
            continue

        delivered += 1
        observed_latency = scenario.latency_ms
        if scenario.jitter_ms:
            observed_latency += (index * 7) % scenario.jitter_ms
        max_latency_ms = max(max_latency_ms, observed_latency)

    return SimulationResult(
        scenario=scenario.name,
        delivered=delivered,
        dropped=dropped,
        rejected_stale_epoch=rejected_stale_epoch,
        reconnected=reconnected,
        max_latency_ms=max_latency_ms,
    )


def validate_simulation_results(results: Iterable[SimulationResult]) -> int:
    """Validates required delivery/drop/reconnect/stale-epoch outcomes."""

    by_name = {result.scenario: result for result in results}
    expectations = (
        ("latency", lambda result: result.delivered > 0 and result.max_latency_ms >= 50),
        ("jitter", lambda result: result.delivered > 0 and result.max_latency_ms > 20),
        ("packet_loss", lambda result: result.delivered > 0 and result.dropped > 0),
        ("disconnect", lambda result: result.delivered == 0 and result.dropped > 0),
        ("reconnect", lambda result: result.delivered > 0 and result.reconnected),
        (
            "asymmetric_partition",
            lambda result: result.delivered > 0 and result.dropped > 0,
        ),
        ("stale_epoch", lambda result: result.rejected_stale_epoch > 0),
    )
    for name, predicate in expectations:
        result = by_name.get(name)
        if result is None:
            return fail(f"missing simulation result for scenario `{name}`")
        if not predicate(result):
            return fail(f"scenario `{name}` did not satisfy its delivery contract: {result}")
    return 0


def print_simulation_results(results: Iterable[SimulationResult]) -> None:
    """Prints a compact deterministic simulation summary."""

    print("vm_coordination_simulation:")
    for result in results:
        print(
            "  "
            f"scenario={result.scenario} "
            f"delivered={result.delivered} "
            f"dropped={result.dropped} "
            f"stale_epoch={result.rejected_stale_epoch} "
            f"reconnected={str(result.reconnected).lower()} "
            f"max_latency_ms={result.max_latency_ms}"
        )


def main() -> int:
    scenario_status = validate_required_scenarios(SCENARIOS)
    if scenario_status != 0:
        return scenario_status

    simulation_results = tuple(simulate_scenario(scenario) for scenario in SCENARIOS)
    simulation_status = validate_simulation_results(simulation_results)
    if simulation_status != 0:
        return simulation_status
    print_simulation_results(simulation_results)

    docker = shutil.which("docker")
    if docker is None:
        return skip("docker binary not found")

    compose = docker_compose_command()
    if compose is None:
        return skip("docker compose is not available")

    if os.environ.get("TERLAN_VM_COORDINATION_DOCKER") != "1":
        names = ", ".join(scenario.name for scenario in SCENARIOS)
        return skip(
            "set TERLAN_VM_COORDINATION_DOCKER=1 to run Docker network scenarios "
            f"({names})"
        )

    docker_info = subprocess.run(
        [docker, "info", "--format", "{{.ServerVersion}}"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=False,
    )
    if docker_info.returncode != 0:
        return fail("docker daemon is unavailable: " + docker_info.stderr.strip())

    print("vm_coordination_docker_ready:")
    print(f"  docker={docker_info.stdout.strip()}")
    print(f"  compose={' '.join(compose)}")
    for scenario in SCENARIOS:
        print(f"  scenario={scenario.name}: {scenario.description}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
