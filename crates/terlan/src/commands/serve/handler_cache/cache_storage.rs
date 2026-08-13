use super::*;

/// Invalidates admitted source generations after the watcher observes change.
#[cfg(test)]
pub(in super::super) fn invalidate_vm_handler_cache() {
    if let Some(cache) = HANDLER_CACHE.get() {
        if let Ok(mut cache) = cache.write() {
            cache.clear();
        }
    }
    advance_cache_epoch();
}

pub(super) fn cache() -> Result<&'static RwLock<HashMap<PathBuf, HandlerCacheEntry>>, String> {
    Ok(HANDLER_CACHE.get_or_init(|| RwLock::new(HashMap::new())))
}
