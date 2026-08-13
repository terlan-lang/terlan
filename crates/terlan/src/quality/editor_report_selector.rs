/// One test selector and the evidence category it covers in an editor report.
#[derive(Debug, Clone, Copy)]
pub(super) struct EditorReportSelector {
    pub(super) fixture: &'static str,
    pub(super) category: &'static str,
    pub(super) evidence: &'static [&'static str],
}
