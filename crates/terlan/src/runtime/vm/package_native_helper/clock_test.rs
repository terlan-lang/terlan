use super::source_test_support::assert_source_checks;

#[test]
fn compiled_clocks_use_package_bindings_and_calendar_stays_source_owned() {
    assert_source_checks(
        "clock_source",
        r#"
module clock_source.
import std.time.{Clock, Date}.
pub wall(): Bool -> Clock.unix_time_ns() > 0.
pub monotonic(): Bool ->
    let first = Clock.monotonic_time_ns();
    let second = Clock.monotonic_time_ns();
    first >= 0 and second >= first.
pub date(): Bool ->
    let today = Date.today_utc();
    Date.year(today) >= 2026 and Date.month(today) >= 1 and Date.day(today) >= 1.
"#,
        &["wall", "monotonic", "date"],
    );
}

#[test]
fn obsolete_clock_capability_tags_are_rejected() {
    for tag in [36, 37] {
        assert_eq!(
            crate::runtime::native_image::control::tvm_fixed_capability_frame_words(tag),
            None
        );
    }
}
