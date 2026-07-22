use super::VmSystemProfileCursor;

impl VmSystemProfileCursor {
    pub(crate) fn position(self) -> usize {
        self.transition_index
    }

    /// Restores a cursor position persisted by a diagnostics consumer. The
    /// position is validated against the transition stream during capture.
    pub(crate) fn from_position(transition_index: usize) -> Self {
        Self { transition_index }
    }
}
