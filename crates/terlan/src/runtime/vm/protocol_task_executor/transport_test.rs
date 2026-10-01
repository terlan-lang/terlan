use super::super::{VmLazyBoundedQueue, TASK_WAKE_TOKEN};
use super::*;
use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;

struct Source(Rc<RefCell<Vec<(Token, Interest)>>>);

impl mio::event::Source for Source {
    fn register(&mut self, _: &mio::Registry, _: Token, _: Interest) -> io::Result<()> {
        panic!("the owner already registered this source")
    }

    fn reregister(
        &mut self,
        _: &mio::Registry,
        token: Token,
        interest: Interest,
    ) -> io::Result<()> {
        self.0.borrow_mut().push((token, interest));
        Ok(())
    }

    fn deregister(&mut self, _: &mio::Registry) -> io::Result<()> {
        panic!("adding write interest must not deregister the source")
    }
}

impl Write for Source {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::ErrorKind::WouldBlock.into())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn package_transport_arms_the_owning_registry_token_without_scheduling_itself() {
    let poll = mio::Poll::new().unwrap();
    let owner = Arc::new(VmProtocolOwnerWake {
        scheduler: VmSchedulerId::primary(),
        registry: poll.registry().try_clone().unwrap(),
        queue: Arc::new(VmLazyBoundedQueue::bounded(4)),
        poll_waker: Arc::new(mio::Waker::new(poll.registry(), TASK_WAKE_TOKEN).unwrap()),
    });
    let registrations = Rc::new(RefCell::new(Vec::new()));
    let token = Token(123);
    let interest = VmSocketInterest {
        owner: Arc::clone(&owner),
        token,
    };
    let mut stream = ReadyStream::new(Source(Rc::clone(&registrations)), interest);
    for _ in 0..3 {
        assert_eq!(
            stream.write(b"stalled").unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
    }
    assert_eq!(
        *registrations.borrow(),
        vec![(token, Interest::READABLE.add(Interest::WRITABLE))]
    );
    assert!(owner.queue.is_empty());
}
