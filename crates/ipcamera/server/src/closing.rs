//! Closing a session off the runtime's threads, where it would hold up every viewer there, and
//! waiting for all of them as the server ends, which would otherwise cut them short.

use std::{
    ops::Deref,
    sync::{Arc, Condvar, Mutex, PoisonError},
    thread,
    time::Instant,
};

use ipcamera_core::sync::LockExt;

#[derive(Default)]
pub(crate) struct Closing {
    open: Mutex<usize>,
    closed: Condvar,
}

impl Closing {
    pub(crate) fn hold<T: Send + 'static>(self: &Arc<Self>, value: T) -> OffThread<T> {
        *self.open.locked() += 1;
        OffThread {
            held: Some((value, Counted(self.clone()))),
        }
    }

    /// Blocks, so never on the runtime's threads; false when `until` came first.
    pub(crate) fn wait(&self, until: Instant) -> bool {
        let mut open = self.open.locked();
        while *open > 0 {
            let Some(left) = until.checked_duration_since(Instant::now()) else {
                return false;
            };
            // A thread that panicked while closing has still been counted off.
            (open, _) = self
                .closed
                .wait_timeout(open, left)
                .unwrap_or_else(PoisonError::into_inner);
        }
        true
    }
}

struct Counted(Arc<Closing>);

impl Drop for Counted {
    fn drop(&mut self) {
        *self.0.open.locked() -= 1;
        self.0.closed.notify_all();
    }
}

pub(crate) struct OffThread<T: Send + 'static> {
    /// Always `Some` until dropped.
    held: Option<(T, Counted)>,
}

impl<T: Send + 'static> Deref for OffThread<T> {
    type Target = T;

    fn deref(&self) -> &T {
        let (value, _) = self.held.as_ref().expect("it is held until it is dropped");
        value
    }
}

impl<T: Send + 'static> Drop for OffThread<T> {
    fn drop(&mut self) {
        let Some((value, counted)) = self.held.take() else {
            return;
        };
        // The value first, then its count, so a waiter never returns while it is still closing.
        let _ = thread::Builder::new()
            .name("ipcamera-close".into())
            .spawn(move || {
                drop(value);
                drop(counted);
            });
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::atomic::{AtomicBool, Ordering},
        time::Duration,
    };

    use super::*;

    struct Slow(Arc<AtomicBool>);

    impl Drop for Slow {
        fn drop(&mut self) {
            thread::sleep(Duration::from_millis(200));
            self.0.store(true, Ordering::SeqCst);
        }
    }

    fn soon() -> Instant {
        Instant::now() + Duration::from_secs(10)
    }

    #[test]
    fn waiting_returns_once_everything_dropped_has_closed() {
        let closing = Arc::new(Closing::default());
        assert!(closing.wait(soon()), "nothing is open");

        let closed = Arc::new(AtomicBool::new(false));
        let held = closing.hold(Slow(closed.clone()));
        let dropped = Instant::now();
        drop(held);
        assert!(
            dropped.elapsed() < Duration::from_millis(150),
            "the drop itself never waits"
        );

        assert!(closing.wait(soon()));
        assert!(
            closed.load(Ordering::SeqCst),
            "it closed before the wait ended"
        );
    }

    #[test]
    fn waiting_gives_up_on_what_is_still_open() {
        let closing = Arc::new(Closing::default());
        let held = closing.hold(Slow(Arc::default()));

        assert!(!closing.wait(Instant::now() + Duration::from_millis(50)));

        drop(held);
        assert!(closing.wait(soon()));
    }
}
