//! Fast owner-local protocol resources with cold multi-generation retention.

use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};

use mio::Token;

thread_local! {
    static OWNER_LOCAL_SCHEDULED: RefCell<VecDeque<Token>> =
        const { RefCell::new(VecDeque::new()) };
}

pub(super) fn push_owner_local_scheduled(token: Token) {
    OWNER_LOCAL_SCHEDULED.with(|scheduled| scheduled.borrow_mut().push_back(token));
}

pub(super) fn pop_owner_local_scheduled() -> Option<Token> {
    OWNER_LOCAL_SCHEDULED.with(|scheduled| scheduled.borrow_mut().pop_front())
}

pub(super) fn has_owner_local_scheduled() -> bool {
    OWNER_LOCAL_SCHEDULED.with(|scheduled| !scheduled.borrow().is_empty())
}

#[derive(Default)]
pub(super) struct VmProtocolLocalResources {
    pub(super) active: Option<VmProtocolLocalResource>,
    pub(super) inactive: BTreeMap<(TypeId, u64), Box<dyn Any>>,
}

pub(super) struct VmProtocolLocalResource {
    key: (TypeId, u64),
    value: Box<dyn Any>,
}

impl VmProtocolLocalResources {
    pub(super) fn with_resource<T: 'static, R>(
        &mut self,
        identity: u64,
        initialize: impl FnOnce() -> Result<T, String>,
        use_resource: impl FnOnce(&mut T) -> Result<R, String>,
    ) -> Result<R, String> {
        let key = (TypeId::of::<T>(), identity);
        if self.active.as_ref().is_none_or(|active| active.key != key) {
            let value = match self.inactive.remove(&key) {
                Some(value) => value,
                None => Box::new(initialize()?),
            };
            if let Some(previous) = self.active.replace(VmProtocolLocalResource { key, value }) {
                self.inactive.insert(previous.key, previous.value);
            }
        }
        let resource = self
            .active
            .as_mut()
            .and_then(|active| active.value.downcast_mut::<T>())
            .ok_or_else(|| {
                format!(
                    "error[vm.protocol_resource]: identity {identity} has a different resource type"
                )
            })?;
        use_resource(resource)
    }

    pub(super) fn retire(&mut self, identity: u64) {
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.key.1 == identity)
        {
            self.active = None;
        }
        self.inactive
            .retain(|(_, resource_identity), _| *resource_identity != identity);
    }
}
