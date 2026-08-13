#![forbid(unsafe_code)]

fn main() {
    terlan::runtime::vm::link_multicore_sanitizer_surface();
}

#[cfg(test)]
#[path = "multicore_tsan_harness_test.rs"]
mod multicore_tsan_harness_test;
