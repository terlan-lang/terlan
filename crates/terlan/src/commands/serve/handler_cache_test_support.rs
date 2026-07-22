#![cfg(test)]

use super::HANDLER_CACHE;

/// Clears the process-wide native handler cache between focused tests.
pub(in crate::commands::serve) fn clear_vm_handler_module_cache_for_test() {
    if let Some(cache) = HANDLER_CACHE.get() {
        cache.lock().expect("AOT handler cache lock").clear();
    }
}
