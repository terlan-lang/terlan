//! Logging is source-owned policy over the console boundary.

use super::source_constructor_test::check_sources;

#[test]
fn logging_names_do_not_override_provider_bodies() {
    let caller = r#"
module logging_consumer.
import std.log.Log.
import std.log.Log.{debug as trace, info as record, warn as caution, error as failure}.
pub check(): Bool -> Log.debug("d") == "debug:d" and trace("d") == "debug:d"
    and Log.info("i") == "info:i" and record("i") == "info:i"
    and Log.warn("w") == "warn:w" and caution("w") == "warn:w"
    and Log.error("e") == "error:e" and failure("e") == "error:e".
"#;
    let provider = r#"
module std.log.Log.
pub debug(message: String): String -> "debug:" + message.
pub info(message: String): String -> "info:" + message.
pub warn(message: String): String -> "warn:" + message.
pub error(message: String): String -> "error:" + message.
"#;
    check_sources(&[caller, provider]);
    check_sources(&[
        &caller.replace("std.log.Log", "ordinary.Log"),
        &provider.replace("std.log.Log", "ordinary.Log"),
    ]);
}
