use super::{
    VmCodeInstructionOffsets, VmCodeServer, VmCodeServerEvent, VmModuleGenerationState,
    VmProcessSource, VmProcessState, VmProcessTable,
};

const MODULE: &str = "cpbugx";
const SOURCE: &str = "module cpbugx.\n\n\
pub before(): Int -> lethal() + 1.\n\n\
pub before2(): Int -> lethal2(2).\n\n\
pub before3(): Int -> lethal3(3).\n\n\
lethal(): Int -> 4711.\n\n\
lethal2(value: Int): Int -> value.\n\n\
lethal3(value: Int): Int -> value.\n";

fn publish_fixture(code_server: &mut VmCodeServer, source_name: &str) {
    let (module, artifact) = VmCodeServer::compile_source_artifact(source_name, SOURCE)
        .expect("compile false-dependency fixture");
    assert_eq!(module, MODULE);
    code_server.publish(module, artifact);
}

#[test]
fn returned_functions_do_not_leave_false_module_generation_dependencies() {
    let mut code_server = VmCodeServer::default();
    publish_fixture(&mut code_server, "cpbugx-v1.terl");
    let first_generation = code_server
        .active_generation(MODULE)
        .expect("first active generation");
    let mut processes = VmProcessTable::default();
    let pid = processes.spawn_root(VmProcessSource::new("app.Loop", "wait", 0));

    for (index, function) in ["before", "before2", "before3"].into_iter().enumerate() {
        let binding = code_server
            .enter_process_function(
                &mut processes,
                pid,
                MODULE,
                function,
                0,
                VmCodeInstructionOffsets::new(index, index + 1),
            )
            .expect("enter exported function");
        assert_eq!(binding.generation, first_generation);
        assert_eq!(
            processes
                .get(pid)
                .expect("live process")
                .current_location()
                .source
                .function,
            function
        );
        assert_eq!(
            code_server.snapshots_for_module(MODULE)[0].active_processes,
            1
        );

        let (returned, retirement) = code_server
            .return_process_function(&mut processes, pid)
            .expect("return exported function");
        assert_eq!(returned.source.function, function);
        assert_eq!(retirement, None);
        assert_eq!(
            processes
                .get(pid)
                .expect("live process")
                .current_location()
                .source
                .module,
            "app.Loop"
        );
        assert_eq!(
            code_server.snapshots_for_module(MODULE)[0].active_processes,
            0
        );
    }

    publish_fixture(&mut code_server, "cpbugx-v2.terl");
    let snapshots = code_server.snapshots_for_module(MODULE);
    assert_eq!(snapshots[0].generation, first_generation);
    assert_eq!(snapshots[0].state, VmModuleGenerationState::Retired);
    assert_eq!(snapshots[0].active_processes, 0);
    assert_eq!(
        processes.get(pid).expect("live process").state,
        VmProcessState::Runnable
    );
    assert_eq!(
        code_server
            .purge_retired_generations(MODULE)
            .expect("purge returned generation"),
        [VmCodeServerEvent::GenerationPurged {
            module: MODULE.to_string(),
            generation: first_generation,
        }]
    );
    code_server
        .unload_active_generation(MODULE)
        .expect("idle process must not block active generation unload");
}

#[test]
fn nested_calls_release_once_and_failed_entry_is_side_effect_free() {
    let mut code_server = VmCodeServer::default();
    publish_fixture(&mut code_server, "cpbugx-nested.terl");
    let mut processes = VmProcessTable::default();
    let pid = processes.spawn_root(VmProcessSource::new("app.Loop", "wait", 0));

    let outer = code_server
        .enter_process_function(
            &mut processes,
            pid,
            MODULE,
            "before",
            0,
            VmCodeInstructionOffsets::new(0, 5),
        )
        .expect("enter outer function");
    let inner = code_server
        .enter_process_function(
            &mut processes,
            pid,
            MODULE,
            "before2",
            0,
            VmCodeInstructionOffsets::new(0, 7),
        )
        .expect("enter inner function");
    assert_eq!(inner, outer);
    assert_eq!(
        code_server
            .return_process_function(&mut processes, pid)
            .expect("return inner function")
            .1,
        None
    );
    assert_eq!(
        code_server.snapshots_for_module(MODULE)[0].active_processes,
        1
    );
    code_server
        .return_process_function(&mut processes, pid)
        .expect("return outer function");
    assert_eq!(
        code_server.snapshots_for_module(MODULE)[0].active_processes,
        0
    );

    let stack_before = processes
        .snapshot(pid)
        .expect("process snapshot")
        .current_stacktrace;
    let error = code_server
        .enter_process_function(
            &mut processes,
            pid,
            MODULE,
            "missing",
            0,
            VmCodeInstructionOffsets::new(0, 9),
        )
        .expect_err("missing export must fail");
    assert!(error.contains("does not export `missing/0`"));
    assert_eq!(
        processes
            .snapshot(pid)
            .expect("process snapshot after rejection")
            .current_stacktrace,
        stack_before
    );
    assert_eq!(
        code_server.snapshots_for_module(MODULE)[0].active_processes,
        0
    );
}
