//! Resource operations for VmHttpSessionRuntime.

use super::*;

impl VmHttpSessionRuntime {
    pub(crate) fn try_create_session(&mut self, identity: &str, ttl: u64) -> Result<bool, String> {
        self.sessions
            .try_create(
                identity,
                ttl,
                &mut Resources {
                    actors: &mut self.actors,
                    tables: &mut self.tables,
                },
            )
            .map_err(|error| error.to_string())
    }

    pub(crate) fn try_rotate_session(
        &mut self,
        identity: &str,
        replacement: &str,
        ttl: u64,
    ) -> Result<bool, String> {
        self.sessions
            .try_rotate(
                identity,
                replacement,
                ttl,
                &mut Resources {
                    actors: &mut self.actors,
                    tables: &mut self.tables,
                },
            )
            .map_err(|error| error.to_string())
    }
}
