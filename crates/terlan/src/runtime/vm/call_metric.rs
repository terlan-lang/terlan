/// Recording mode shared by exact-function count and time instrumentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmCallMetricMode {
    Active,
    Paused,
}
