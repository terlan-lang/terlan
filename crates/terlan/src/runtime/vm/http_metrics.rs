/// Snapshot of VM HTTP queue pressure exposed to runtime inspection.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct VmHttpQueueMetrics {
    pub(crate) current_depth: usize,
    pub(crate) max_depth: usize,
    pub(crate) enqueue_count: usize,
    pub(crate) dequeue_count: usize,
    pub(crate) enqueue_wait_count: usize,
    pub(crate) enqueue_wait_total_ns: u128,
    pub(crate) dequeue_wait_count: usize,
    pub(crate) dequeue_wait_total_ns: u128,
    pub(crate) parked_producers: usize,
    pub(crate) parked_consumers: usize,
    pub(crate) max_parked_producers: usize,
    pub(crate) max_parked_consumers: usize,
    pub(crate) producer_wakeup_count: usize,
    pub(crate) consumer_wakeup_count: usize,
}
