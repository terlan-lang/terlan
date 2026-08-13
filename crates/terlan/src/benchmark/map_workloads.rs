use super::*;

/// Measures VM-owned scheduler primitives directly.
///
/// Inputs:
/// - No external input.
///
/// Output:
/// - Success when scheduling handles yield requeue, block/wake, and exit
///   cleanup with stable reduction accounting.
///
/// Transformation:
/// - Exercises the scheduler table directly instead of relying on actor
///   facade calls or host runtime scheduling behavior.
pub(super) fn measure_vm_scheduler_runtime_primitives() -> Result<(), String> {
    let mut processes = VmProcessTable::default();
    let pid = processes.spawn_root(VmProcessSource::new("bench.Scheduler", "main", 0));
    processes.with_process_control_mutator(pid, |process| {
        process.add_resource_handle("native:scheduler-benchmark");
    })?;

    let mut scheduler = VmScheduler::default();
    scheduler.enqueue_runnable(&processes, pid)?;
    let yielded = scheduler.run_next(&mut processes, |_process, _slice| {
        VmSchedulerDecision::Yield { reductions: 3 }
    })?;
    if yielded.pid != Some(pid)
        || yielded.reductions_charged != 3
        || yielded.outcome != VmSchedulerOutcome::Ran
    {
        return Err(format!("unexpected scheduler yield run: {yielded:?}"));
    }

    let blocked = scheduler.run_next(&mut processes, |_process, _slice| {
        VmSchedulerDecision::Block { reductions: 2 }
    })?;
    if blocked.pid != Some(pid)
        || blocked.reductions_charged != 2
        || blocked.outcome != VmSchedulerOutcome::Blocked
    {
        return Err(format!("unexpected scheduler block run: {blocked:?}"));
    }

    scheduler.wake_process(&mut processes, pid)?;
    let exited = scheduler.run_next(&mut processes, |_process, _slice| {
        VmSchedulerDecision::Exit {
            reductions: 1,
            reason: process::VmExitReason::Normal,
        }
    })?;
    match exited.outcome {
        VmSchedulerOutcome::Exited(cleanup) if cleanup == ["native:scheduler-benchmark"] => Ok(()),
        other => Err(format!("unexpected scheduler exit run: {other:?}")),
    }
}

/// Measures VM-owned NativeBoundary resource ownership primitives directly.
///
/// Inputs:
/// - No external input.
///
/// Output:
/// - Success when resource registration, ownership checks, transfer, release,
///   stale-handle diagnostics, and owner-exit cleanup behave correctly.
///
/// Transformation:
/// - Exercises the VM resource table directly instead of routing through a
///   native adapter or host runtime handle.
pub(super) fn measure_vm_resource_runtime_primitives() -> Result<(), String> {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(VmProcessSource::new("bench.Resource", "owner", 0));
    let recipient = processes.spawn_root(VmProcessSource::new("bench.Resource", "recipient", 0));
    let mut resources = VmResourceTable::default();

    let registered = resources.register(
        &mut processes,
        owner,
        VmResourceDescriptor::new("postgres.connection", "primary"),
        VmResourceTransferPolicy::Transferable,
    )?;
    let resource = match registered {
        VmResourceEvent::Registered {
            id,
            owner: event_owner,
        } if event_owner == owner => id,
        other => return Err(format!("unexpected resource registration event: {other:?}")),
    };

    resources.get_for_owner(owner, resource)?;
    if resources.get_for_owner(recipient, resource).is_ok() {
        return Err("resource was readable by non-owner".to_string());
    }

    let transferred = resources.transfer(&mut processes, resource, owner, recipient)?;
    if transferred
        != (VmResourceEvent::Transferred {
            id: resource,
            from: owner,
            to: recipient,
        })
    {
        return Err(format!(
            "unexpected resource transfer event: {transferred:?}"
        ));
    }
    resources.get_for_owner(recipient, resource)?;

    let released = resources.release(&mut processes, recipient, resource)?;
    if released
        != (VmResourceEvent::Released {
            id: resource,
            owner: recipient,
        })
    {
        return Err(format!("unexpected resource release event: {released:?}"));
    }
    if resources.get_for_owner(recipient, resource).is_ok() {
        return Err("released resource remained readable".to_string());
    }

    let cleanup_event = resources.register(
        &mut processes,
        recipient,
        VmResourceDescriptor::new("file.handle", "/tmp/report"),
        VmResourceTransferPolicy::OwnerOnly,
    )?;
    let cleanup_resource = match cleanup_event {
        VmResourceEvent::Registered { id, .. } => id,
        other => return Err(format!("unexpected cleanup registration event: {other:?}")),
    };
    processes.exit_process(recipient, process::VmExitReason::Normal)?;
    let cleanup = resources.cleanup_owner(recipient);
    if cleanup
        == [VmResourceEvent::CleanedUpOnExit {
            id: cleanup_resource,
            owner: recipient,
        }]
    {
        Ok(())
    } else {
        Err(format!("unexpected resource cleanup events: {cleanup:?}"))
    }
}

/// Measures VM cancellation and resource cleanup integration directly.
///
/// Inputs:
/// - No external input.
///
/// Output:
/// - Success when cancellation returns cleanup handles, resource cleanup removes
///   the owned handle, and later access reports the handle as stale.
///
/// Transformation:
/// - Exercises the scheduler/resource boundary without routing through source
///   syntax, host runtime cancellation, or NativeBoundary adapters.
pub(super) fn measure_vm_cancellation_resource_cleanup_primitives() -> Result<(), String> {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(VmProcessSource::new("bench.Resource", "owner", 0));
    let mut resources = VmResourceTable::default();
    let registered = resources.register(
        &mut processes,
        owner,
        VmResourceDescriptor::new("native.vector", "users"),
        VmResourceTransferPolicy::OwnerOnly,
    )?;
    let resource = match registered {
        VmResourceEvent::Registered {
            id,
            owner: event_owner,
        } if event_owner == owner => id,
        other => return Err(format!("unexpected resource registration event: {other:?}")),
    };

    let mut scheduler = VmScheduler::default();
    scheduler.enqueue_runnable(&processes, owner)?;
    scheduler.request_cancellation(&mut processes, owner)?;
    let cancelled = scheduler.run_next(&mut processes, |_process, _slice| {
        VmSchedulerDecision::Yield { reductions: 1 }
    })?;
    match cancelled.outcome {
        VmSchedulerOutcome::Cancelled(handles)
            if handles == [format!("resource:{}", resource.as_u64())] => {}
        other => return Err(format!("unexpected cancellation outcome: {other:?}")),
    }

    let cleanup = resources.cleanup_owner(owner);
    if cleanup
        != [VmResourceEvent::CleanedUpOnExit {
            id: resource,
            owner,
        }]
    {
        return Err(format!(
            "unexpected cancellation cleanup events: {cleanup:?}"
        ));
    }
    if resources.get_for_owner(owner, resource).is_ok() {
        return Err("cancelled process resource remained readable".to_string());
    }
    if !resources.snapshots().is_empty() {
        return Err("cancelled process resource remained in snapshot".to_string());
    }
    Ok(())
}

/// Measures VM-owned local table primitives directly.
///
/// Inputs:
/// - No external input.
///
/// Output:
/// - Success when the table path creates, mutates, reads, deletes, and
///   reports inspection state correctly.
///
/// Transformation:
/// - Exercises the VM primitive itself without claiming that source-level
///   collection syntax has been wired to this runtime path.
pub(super) fn measure_vm_table_runtime_primitives() -> Result<(), String> {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(VmProcessSource::new("bench.Table", "main", 0));
    let mut tables = VmTableStore::default();
    let event = tables.create(&processes, owner, "bench", VmTableAccess::OwnerOnly)?;
    let table = match event {
        table::VmTableEvent::Created { id, .. } => id,
        other => return Err(format!("unexpected table creation event: {other:?}")),
    };
    let key = VmPrimitiveValue::String("key".to_string());
    let value = VmPrimitiveValue::Int(42);
    tables.insert(&processes, owner, table, key.clone(), value.clone())?;
    let found = tables.lookup(&processes, owner, table, &key)?;
    if found != Some(value) {
        return Err(format!("unexpected table lookup result: {found:?}"));
    }
    let deleted = tables.delete(&processes, owner, table, &key)?;
    if deleted.is_none() {
        return Err("table delete did not report a deletion".to_string());
    }
    let snapshot = tables
        .snapshots()
        .into_iter()
        .find(|snapshot| snapshot.id == table)
        .ok_or_else(|| "table snapshot missing after delete".to_string())?;
    if snapshot.len == 0 {
        Ok(())
    } else {
        Err(format!(
            "table snapshot length after delete was {}",
            snapshot.len
        ))
    }
}

/// Measures the current Terlan VM map value workload.
///
/// Inputs:
/// - `size`: number of entries to insert, read, and update.
/// - `iterations`: timing samples to collect.
///
/// Output:
/// - Measurement summary for the active VM adaptive map representation.
///
/// Transformation:
/// - Uses the shared VM map helper so benchmark semantics stay aligned with
///   runtime `Map.put` behavior.
pub(super) fn measure_vm_map_workload(
    size: usize,
    iterations: usize,
) -> Result<Measurement, String> {
    let mut durations = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        run_vm_map_workload(size)?;
        durations.push(start.elapsed());
    }
    Ok(Measurement::from_durations(
        vm_map_measurement_name(size),
        &durations,
    ))
}

/// Executes one Terlan VM map insert/lookup/update workload.
///
/// Inputs:
/// - `size`: number of entries in the map.
///
/// Output:
/// - Success when all correctness checks pass.
///
/// Transformation:
/// - Builds a persistent-style map, reads every key, and updates every key
///   through the active VM adaptive map value.
pub(super) fn run_vm_map_workload(size: usize) -> Result<(), String> {
    let mut map = map_value::VmMapValue::from_entries(Vec::new());
    for index in 1..=size as i64 {
        map.insert_or_replace(VmPrimitiveValue::Int(index), VmPrimitiveValue::Int(index));
    }

    let mut sum = 0_i64;
    for index in 1..=size as i64 {
        let value = map
            .lookup(&VmPrimitiveValue::Int(index))
            .ok_or_else(|| format!("VM map lookup missed key {index}"))?;
        let VmPrimitiveValue::Int(number) = value else {
            return Err(format!("VM map lookup returned non-int value: {value:?}"));
        };
        sum += number;
    }

    let expected = size as i64 * (size as i64 + 1) / 2;
    if sum != expected {
        return Err(format!("VM map lookup sum {sum} != expected {expected}"));
    }

    let mut updated = map;
    for index in 1..=size as i64 {
        updated = updated.put_persistent_owned(
            VmPrimitiveValue::Int(index),
            VmPrimitiveValue::Int(index + 1),
        );
    }
    let last = updated
        .lookup(&VmPrimitiveValue::Int(size as i64))
        .ok_or_else(|| "VM map update lost last key".to_string())?;
    if last != &VmPrimitiveValue::Int(size as i64 + 1) {
        return Err(format!(
            "VM map update returned unexpected last value: {last:?}"
        ));
    }
    Ok(())
}

/// Measures a VM map workload where every key has the same hash.
///
/// Inputs:
/// - `iterations`: timing samples to collect.
///
/// Output:
/// - Measurement summary for the forced-collision A-CHAMP node path.
///
/// Transformation:
/// - Uses a benchmark-only key type with constant `Hash` so the VM map must
///   exercise collision-node lookup and update behavior.
pub(super) fn measure_vm_collision_heavy_map_workload(
    iterations: usize,
) -> Result<Measurement, String> {
    measure_repeated(
        "terlan_vm_map_collision_heavy_size_512",
        iterations,
        run_vm_collision_heavy_map_workload,
    )
}

pub(super) fn run_vm_collision_heavy_map_workload() -> Result<(), String> {
    let mut map = map_value::VmMapValue::from_entries(Vec::new());
    for index in 0..COLLISION_HEAVY_MAP_SIZE as i64 {
        map.insert_or_replace(CollidingBenchmarkKey(index), index);
    }
    for index in 0..COLLISION_HEAVY_MAP_SIZE as i64 {
        let value = map
            .lookup(&CollidingBenchmarkKey(index))
            .ok_or_else(|| format!("collision-heavy VM map missed key {index}"))?;
        if *value != index {
            return Err(format!(
                "collision-heavy VM map returned {value} for key {index}"
            ));
        }
    }
    let updated = map.put_persistent_owned(
        CollidingBenchmarkKey(COLLISION_HEAVY_MAP_SIZE as i64 - 1),
        9_999,
    );
    let value = updated
        .lookup(&CollidingBenchmarkKey(COLLISION_HEAVY_MAP_SIZE as i64 - 1))
        .ok_or_else(|| "collision-heavy VM map lost updated key".to_string())?;
    if *value == 9_999 {
        Ok(())
    } else {
        Err(format!(
            "collision-heavy VM map updated value was {value}, expected 9999"
        ))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct CollidingBenchmarkKey(i64);

impl std::hash::Hash for CollidingBenchmarkKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        1_i64.hash(state);
    }
}

/// Measures shared persistent updates for the VM map path.
pub(super) fn measure_vm_shared_persistent_map_workload(
    size: usize,
    iterations: usize,
) -> Result<Measurement, String> {
    let mut durations = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        run_vm_shared_persistent_map_workload(size)?;
        durations.push(start.elapsed());
    }
    Ok(Measurement::from_durations(
        vm_shared_persistent_map_measurement_name(size),
        &durations,
    ))
}

pub(super) fn run_vm_shared_persistent_map_workload(size: usize) -> Result<(), String> {
    let original = map_value::VmMapValue::from_entries(
        (1..=size as i64)
            .map(|index| (VmPrimitiveValue::Int(index), VmPrimitiveValue::Int(index)))
            .collect(),
    );
    let mut updated = original.clone();
    for index in 1..=size as i64 {
        updated =
            updated.put_persistent(VmPrimitiveValue::Int(index), VmPrimitiveValue::Int(-index));
    }
    let original_last = original
        .lookup(&VmPrimitiveValue::Int(size as i64))
        .ok_or_else(|| "shared persistent VM map lost original last key".to_string())?;
    let updated_last = updated
        .lookup(&VmPrimitiveValue::Int(size as i64))
        .ok_or_else(|| "shared persistent VM map lost updated last key".to_string())?;
    if original_last != &VmPrimitiveValue::Int(size as i64) {
        return Err(format!(
            "shared persistent VM map mutated original last value: {original_last:?}"
        ));
    }
    if updated_last != &VmPrimitiveValue::Int(-(size as i64)) {
        return Err(format!(
            "shared persistent VM map returned unexpected updated value: {updated_last:?}"
        ));
    }
    Ok(())
}

/// Measures iterator, equality, and rendering behavior for VM maps.
pub(super) fn measure_vm_iterator_equality_rendering_map_workload(
    size: usize,
    iterations: usize,
) -> Result<Measurement, String> {
    let mut durations = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        run_vm_iterator_equality_rendering_map_workload(size)?;
        durations.push(start.elapsed());
    }
    Ok(Measurement::from_durations(
        vm_iterator_equality_rendering_map_measurement_name(size),
        &durations,
    ))
}

pub(super) fn run_vm_iterator_equality_rendering_map_workload(size: usize) -> Result<(), String> {
    let map = map_value::VmMapValue::from_entries(
        (1..=size as i64)
            .map(|index| (VmPrimitiveValue::Int(index), VmPrimitiveValue::Int(index)))
            .collect(),
    );
    let entries = map.to_entries();
    let sum = entries
        .iter()
        .map(|(_, value)| match value {
            VmPrimitiveValue::Int(number) => Ok(*number),
            other => Err(format!("VM map iterator returned non-int value: {other:?}")),
        })
        .sum::<Result<i64, String>>()?;
    let expected = size as i64 * (size as i64 + 1) / 2;
    if sum != expected {
        return Err(format!("VM map iterator sum {sum} != expected {expected}"));
    }
    let same = map_value::VmMapValue::from_entries(entries.clone());
    if map != same {
        return Err("VM map equality failed for equivalent entry order".to_string());
    }
    let rendered = format!("{entries:?}");
    if rendered.contains("Int(1)") && rendered.contains(&format!("Int({})", size)) {
        Ok(())
    } else {
        Err("VM map rendering omitted expected boundary keys".to_string())
    }
}

/// Measures mixed insert/remove/update behavior for VM maps.
pub(super) fn measure_vm_mixed_map_workload(
    size: usize,
    iterations: usize,
) -> Result<Measurement, String> {
    let mut durations = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        run_vm_mixed_map_workload(size)?;
        durations.push(start.elapsed());
    }
    Ok(Measurement::from_durations(
        vm_mixed_map_measurement_name(size),
        &durations,
    ))
}

pub(super) fn run_vm_mixed_map_workload(size: usize) -> Result<(), String> {
    let mut map = map_value::VmMapValue::from_entries(
        (1..=size as i64)
            .map(|index| (VmPrimitiveValue::Int(index), VmPrimitiveValue::Int(index)))
            .collect(),
    );
    for index in 1..=size as i64 {
        if index % 3 == 0 {
            map.remove(&VmPrimitiveValue::Int(index));
        } else if index % 3 == 1 {
            map.insert_or_replace(
                VmPrimitiveValue::Int(index),
                VmPrimitiveValue::Int(index * 10),
            );
        } else {
            map.insert_or_replace(
                VmPrimitiveValue::Int(size as i64 + index),
                VmPrimitiveValue::Int(index),
            );
        }
    }
    if map.lookup(&VmPrimitiveValue::Int(3)).is_some() {
        return Err("VM mixed map retained removed key 3".to_string());
    }
    if map.lookup(&VmPrimitiveValue::Int(1)) != Some(&VmPrimitiveValue::Int(10)) {
        return Err("VM mixed map failed to update key 1".to_string());
    }
    if map
        .lookup(&VmPrimitiveValue::Int(size as i64 + 2))
        .is_none()
    {
        return Err("VM mixed map failed to insert derived key".to_string());
    }
    Ok(())
}

/// Measures OTP map workload timings for comparison with Terlan VM maps.
///
/// Inputs:
/// - `size`: number of entries to insert, read, and update.
/// - `iterations`: timing samples collected inside one Erlang VM process.
///
/// Output:
/// - Measurement summary using OTP-reported inner workload durations.
///
/// Transformation:
/// - Runs Erlang once per measurement and parses nanosecond durations printed
///   by the benchmark expression, so OTP startup is not included.
pub(super) fn measure_otp_map_workload(
    size: usize,
    iterations: usize,
) -> Result<Measurement, String> {
    let eval = otp_map_benchmark_eval(size, iterations);
    measure_otp_map_eval(otp_map_measurement_name(size), eval, iterations)
}

pub(super) fn measure_otp_shared_persistent_map_workload(
    size: usize,
    iterations: usize,
) -> Result<Measurement, String> {
    let eval = otp_shared_persistent_map_benchmark_eval(size, iterations);
    measure_otp_map_eval(
        otp_shared_persistent_map_measurement_name(size),
        eval,
        iterations,
    )
}

pub(super) fn measure_otp_iterator_equality_rendering_map_workload(
    size: usize,
    iterations: usize,
) -> Result<Measurement, String> {
    let eval = otp_iterator_equality_rendering_map_benchmark_eval(size, iterations);
    measure_otp_map_eval(
        otp_iterator_equality_rendering_map_measurement_name(size),
        eval,
        iterations,
    )
}

pub(super) fn measure_otp_mixed_map_workload(
    size: usize,
    iterations: usize,
) -> Result<Measurement, String> {
    let eval = otp_mixed_map_benchmark_eval(size, iterations);
    measure_otp_map_eval(otp_mixed_map_measurement_name(size), eval, iterations)
}

pub(super) fn measure_otp_map_eval(
    name: &'static str,
    eval: String,
    iterations: usize,
) -> Result<Measurement, String> {
    let output = Command::new("erl")
        .args(["-noshell", "-eval", &eval])
        .output()
        .map_err(|error| format!("failed to start OTP map benchmark with erl: {error}"))?;
    if !output.status.success() {
        return Err(format_command_failure("erl map benchmark", &output));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let durations = parse_erlang_duration_list(stdout.trim())?;
    if durations.len() != iterations {
        return Err(format!(
            "OTP map benchmark returned {} durations, expected {iterations}",
            durations.len()
        ));
    }
    Ok(Measurement::from_durations(name, &durations))
}

/// Builds the Erlang expression for one OTP map benchmark measurement.
///
/// Inputs:
/// - `size`: map cardinality.
/// - `iterations`: samples to collect.
///
/// Output:
/// - `erl -eval` expression that prints a list of nanosecond durations.
///
/// Transformation:
/// - Uses only OTP `maps` and `lists` operations that correspond to the VM map
///   workload: insert, lookup, and persistent update.
pub(super) fn otp_map_benchmark_eval(size: usize, iterations: usize) -> String {
    format!(
        "Size = {size}, Iterations = {iterations}, \
         Work = fun() -> \
         Keys = lists:seq(1, Size), \
         Start = erlang:monotonic_time(nanosecond), \
         M0 = lists:foldl(fun(I, Acc) -> maps:put(I, I, Acc) end, #{{}}, Keys), \
         Sum = lists:foldl(fun(I, Acc) -> Acc + maps:get(I, M0) end, 0, Keys), \
         M1 = lists:foldl(fun(I, Acc) -> maps:update(I, I + 1, Acc) end, M0, Keys), \
         Expected = Size * (Size + 1) div 2, \
         Last = maps:get(Size, M1), \
         End = erlang:monotonic_time(nanosecond), \
         case {{Sum =:= Expected, Last =:= Size + 1}} of \
         {{true, true}} -> End - Start; \
         _ -> halt(2) \
         end \
         end, \
         Durations = [Work() || _ <- lists:seq(1, Iterations)], \
         io:format(\"~p~n\", [Durations]), \
         halt(0)."
    )
}

pub(super) fn otp_shared_persistent_map_benchmark_eval(size: usize, iterations: usize) -> String {
    format!(
        "Size = {size}, Iterations = {iterations}, \
         Work = fun() -> \
         Keys = lists:seq(1, Size), \
         Start = erlang:monotonic_time(nanosecond), \
         M0 = lists:foldl(fun(I, Acc) -> maps:put(I, I, Acc) end, #{{}}, Keys), \
         M1 = lists:foldl(fun(I, Acc) -> maps:update(I, -I, Acc) end, M0, Keys), \
         OriginalLast = maps:get(Size, M0), \
         UpdatedLast = maps:get(Size, M1), \
         End = erlang:monotonic_time(nanosecond), \
         case {{OriginalLast =:= Size, UpdatedLast =:= -Size}} of \
         {{true, true}} -> End - Start; \
         _ -> halt(2) \
         end \
         end, \
         Durations = [Work() || _ <- lists:seq(1, Iterations)], \
         io:format(\"~p~n\", [Durations]), \
         halt(0)."
    )
}

pub(super) fn otp_iterator_equality_rendering_map_benchmark_eval(
    size: usize,
    iterations: usize,
) -> String {
    format!(
        "Size = {size}, Iterations = {iterations}, \
         Work = fun() -> \
         Keys = lists:seq(1, Size), \
         Start = erlang:monotonic_time(nanosecond), \
         M0 = lists:foldl(fun(I, Acc) -> maps:put(I, I, Acc) end, #{{}}, Keys), \
         List = maps:to_list(M0), \
         Sum = lists:foldl(fun({{_, V}}, Acc) -> Acc + V end, 0, List), \
         Same = M0 =:= maps:from_list(List), \
         Rendered = lists:flatten(io_lib:format(\"~p\", [M0])), \
         HasFirst = string:str(Rendered, \"1\") > 0, \
         HasLast = string:str(Rendered, integer_to_list(Size)) > 0, \
         Expected = Size * (Size + 1) div 2, \
         End = erlang:monotonic_time(nanosecond), \
         case {{Sum =:= Expected, Same, HasFirst, HasLast}} of \
         {{true, true, true, true}} -> End - Start; \
         _ -> halt(2) \
         end \
         end, \
         Durations = [Work() || _ <- lists:seq(1, Iterations)], \
         io:format(\"~p~n\", [Durations]), \
         halt(0)."
    )
}

pub(super) fn otp_mixed_map_benchmark_eval(size: usize, iterations: usize) -> String {
    format!(
        "Size = {size}, Iterations = {iterations}, \
         Work = fun() -> \
         Keys = lists:seq(1, Size), \
         Start = erlang:monotonic_time(nanosecond), \
         M0 = lists:foldl(fun(I, Acc) -> maps:put(I, I, Acc) end, #{{}}, Keys), \
         M1 = lists:foldl(fun(I, Acc) -> \
             case I rem 3 of \
             0 -> maps:remove(I, Acc); \
             1 -> maps:update(I, I * 10, Acc); \
             2 -> maps:put(Size + I, I, Acc) \
             end \
         end, M0, Keys), \
         Removed = not maps:is_key(3, M1), \
         Updated = maps:get(1, M1) =:= 10, \
         Inserted = maps:is_key(Size + 2, M1), \
         End = erlang:monotonic_time(nanosecond), \
         case {{Removed, Updated, Inserted}} of \
         {{true, true, true}} -> End - Start; \
         _ -> halt(2) \
         end \
         end, \
         Durations = [Work() || _ <- lists:seq(1, Iterations)], \
         io:format(\"~p~n\", [Durations]), \
         halt(0)."
    )
}

/// Parses an Erlang printed integer list as durations.
///
/// Inputs:
/// - `text`: output such as `[1,2,3]`.
///
/// Output:
/// - Duration values interpreted as nanoseconds.
///
/// Transformation:
/// - Keeps the parser intentionally strict so malformed OTP benchmark output
///   fails the gate instead of producing misleading timings.
pub(super) fn parse_erlang_duration_list(text: &str) -> Result<Vec<Duration>, String> {
    let trimmed = text.trim();
    let Some(body) = trimmed
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
    else {
        return Err(format!(
            "OTP map benchmark output was not a list: {trimmed}"
        ));
    };
    if body.trim().is_empty() {
        return Ok(Vec::new());
    }
    body.split(',')
        .map(|part| {
            let nanos = part
                .trim()
                .parse::<u64>()
                .map_err(|error| format!("invalid OTP duration `{part}`: {error}"))?;
            Ok(Duration::from_nanos(nanos))
        })
        .collect()
}

/// Returns the Terlan VM map benchmark row name for one size.
///
/// Inputs:
/// - `size`: map cardinality.
///
/// Output:
/// - Stable measurement name split from OTP rows.
///
/// Transformation:
/// - Keeps benchmark JSON readable and avoids dynamic names that cannot be
///   stored in the static measurement schema.
pub(super) fn vm_map_measurement_name(size: usize) -> &'static str {
    match size {
        16 => "terlan_vm_map_insert_lookup_update_size_16",
        32 => "terlan_vm_map_insert_lookup_update_size_32",
        33 => "terlan_vm_map_insert_lookup_update_size_33",
        127 => "terlan_vm_map_insert_lookup_update_size_127",
        128 => "terlan_vm_map_insert_lookup_update_size_128",
        129 => "terlan_vm_map_insert_lookup_update_size_129",
        5_000 => "terlan_vm_map_insert_lookup_update_size_5000",
        _ => "terlan_vm_map_insert_lookup_update_size_custom",
    }
}

/// Returns the OTP map benchmark row name for one size.
///
/// Inputs:
/// - `size`: map cardinality.
///
/// Output:
/// - Stable measurement name split from Terlan VM rows.
///
/// Transformation:
/// - Keeps OTP comparison rows explicit without tying the benchmark report to
///   a runtime fallback or product dependency.
pub(super) fn otp_map_measurement_name(size: usize) -> &'static str {
    match size {
        16 => "otp_map_insert_lookup_update_size_16",
        32 => "otp_map_insert_lookup_update_size_32",
        33 => "otp_map_insert_lookup_update_size_33",
        127 => "otp_map_insert_lookup_update_size_127",
        128 => "otp_map_insert_lookup_update_size_128",
        129 => "otp_map_insert_lookup_update_size_129",
        5_000 => "otp_map_insert_lookup_update_size_5000",
        _ => "otp_map_insert_lookup_update_size_custom",
    }
}

pub(super) fn vm_shared_persistent_map_measurement_name(size: usize) -> &'static str {
    match size {
        5_000 => "terlan_vm_map_shared_persistent_update_size_5000",
        _ => "terlan_vm_map_shared_persistent_update_size_custom",
    }
}

pub(super) fn otp_shared_persistent_map_measurement_name(size: usize) -> &'static str {
    match size {
        5_000 => "otp_map_shared_persistent_update_size_5000",
        _ => "otp_map_shared_persistent_update_size_custom",
    }
}

pub(super) fn vm_iterator_equality_rendering_map_measurement_name(size: usize) -> &'static str {
    match size {
        5_000 => "terlan_vm_map_iterator_equality_rendering_size_5000",
        _ => "terlan_vm_map_iterator_equality_rendering_size_custom",
    }
}

pub(super) fn otp_iterator_equality_rendering_map_measurement_name(size: usize) -> &'static str {
    match size {
        5_000 => "otp_map_iterator_equality_rendering_size_5000",
        _ => "otp_map_iterator_equality_rendering_size_custom",
    }
}

pub(super) fn vm_mixed_map_measurement_name(size: usize) -> &'static str {
    match size {
        5_000 => "terlan_vm_map_mixed_insert_remove_update_size_5000",
        _ => "terlan_vm_map_mixed_insert_remove_update_size_custom",
    }
}

pub(super) fn otp_mixed_map_measurement_name(size: usize) -> &'static str {
    match size {
        5_000 => "otp_map_mixed_insert_remove_update_size_5000",
        _ => "otp_map_mixed_insert_remove_update_size_custom",
    }
}

pub(super) const LARGE_MAP_REFERENCE_SIZE: usize = 5_000;
