use super::connection::Callbacks;
use super::hub::AdmissionCallbacks;
use super::Utf8Bytes;
use crate::channel_plan::WebSocketEndpointPlan;
use crate::source_descriptor::PairedTransition;
use std::sync::{Arc, Mutex};

pub(crate) type Events = Arc<Mutex<Vec<String>>>;

pub(crate) struct Source {
    events: Events,
    plan: WebSocketEndpointPlan<()>,
    fail_cancel: bool,
}

impl Source {
    pub(crate) fn new(events: &Events, fail_cancel: bool) -> Self {
        Self {
            events: Arc::clone(events),
            plan: WebSocketEndpointPlan::new(4, 1024).unwrap(),
            fail_cancel,
        }
    }
}

impl AdmissionCallbacks for Source {
    fn room_identity(&mut self, _: i64) -> Result<String, crate::ServiceError> {
        unreachable!("unpaired endpoint")
    }
    fn matched(
        &mut self,
        _: String,
        _: String,
        _: String,
    ) -> Result<(String, String), crate::ServiceError> {
        unreachable!("unpaired endpoint")
    }

    fn restored(
        &mut self,
        _: String,
        _: String,
        _: i64,
        _: String,
        _: String,
    ) -> Result<String, crate::ServiceError> {
        unreachable!("unpaired endpoint")
    }
}

impl Callbacks for Source {
    type Callback = ();

    fn plan(&self) -> &WebSocketEndpointPlan<()> {
        &self.plan
    }
    async fn identity(&mut self, _: String) -> Result<Option<(String, i64)>, crate::ServiceError> {
        unreachable!("unpaired endpoint")
    }
    fn waiting(&mut self) -> Result<String, crate::ServiceError> {
        unreachable!("unpaired endpoint")
    }
    fn peer_left(&mut self) -> Result<String, crate::ServiceError> {
        unreachable!("unpaired endpoint")
    }
    fn enqueue(&mut self, text: Utf8Bytes) -> Result<(), crate::ServiceError> {
        self.events.lock().unwrap().push(format!("text:{text}"));
        Ok(())
    }
    fn next_inbound(&mut self) -> Result<(bool, Option<String>), crate::ServiceError> {
        Ok((false, None))
    }
    fn next_paired(
        &mut self,
        _: Option<crate::source_descriptor::PairContext>,
    ) -> Result<Option<PairedTransition>, crate::ServiceError> {
        unreachable!("unpaired endpoint")
    }
    fn writable(&mut self) -> Result<(), crate::ServiceError> {
        self.events.lock().unwrap().push("writable".into());
        Ok(())
    }
    fn close(&mut self) -> Result<(), crate::ServiceError> {
        self.events.lock().unwrap().push("close".into());
        Ok(())
    }
    fn cancel(&mut self, reason: String) -> Result<(), crate::ServiceError> {
        self.events.lock().unwrap().push(format!("cancel:{reason}"));
        if self.fail_cancel {
            Err("cleanup failed".into())
        } else {
            Ok(())
        }
    }
}
