use super::Pool;

/// Builds a disconnected pool value without opening a database connection.
pub(crate) fn disconnected_pool(url: impl Into<String>) -> Pool {
    Pool { url: url.into() }
}
