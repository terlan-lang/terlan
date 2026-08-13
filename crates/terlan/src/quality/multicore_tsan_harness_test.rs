#[test]
fn bounded_seeded_multicore_memory_model_has_deadlock_watchdog() {
    terlan::runtime::vm::run_multicore_sanitizer_stress();
}

#[test]
#[ignore = "launched with an explicit seed by the bounded parent test"]
fn seeded_multicore_memory_model_child() {
    terlan::runtime::vm::run_multicore_sanitizer_seed();
}
