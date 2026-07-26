//! Streaming fingerprints for native compiler cache entries.

use std::fmt::{self, Write};

use sha2::{Digest, Sha256};

use super::{NativeModule, NATIVE_ABI_VERSION};

impl NativeModule {
    /// Hashes deterministic cache input without materializing backend IR text.
    ///
    /// Large generated continuation graphs can render to many gigabytes. The
    /// cache only needs their digest, so stream the canonical debug encoding
    /// directly into SHA-256 instead of retaining a second complete graph as
    /// one `String`.
    pub(crate) fn fingerprint_sha256(&self) -> String {
        let mut writer = Sha256Writer(Sha256::new());
        writer
            .write_str(NATIVE_ABI_VERSION)
            .expect("SHA-256 writes cannot fail");
        writer.write_char('\0').expect("SHA-256 writes cannot fail");
        write!(writer, "{self:?}").expect("SHA-256 writes cannot fail");
        let digest = writer.0.finalize();
        let mut encoded = String::with_capacity(digest.len().saturating_mul(2));
        for byte in digest {
            write!(encoded, "{byte:02x}").expect("writing to a String cannot fail");
        }
        encoded
    }
}

struct Sha256Writer(Sha256);

impl Write for Sha256Writer {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.0.update(value.as_bytes());
        Ok(())
    }
}
