//! Taking a lock that a panicking thread left poisoned: what the suite's locks guard stays
//! usable, and refusing it would freeze the app.

use std::sync::{Mutex, MutexGuard, PoisonError};

/// Locking that carries on past a panic in another thread.
pub trait LockExt<T> {
    /// Takes the lock, even if a thread panicked holding it.
    fn locked(&self) -> MutexGuard<'_, T>;

    /// The value, without locking, for whoever has the `Mutex` to itself; even if poisoned.
    fn locked_mut(&mut self) -> &mut T;
}

impl<T> LockExt<T> for Mutex<T> {
    fn locked(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn locked_mut(&mut self) -> &mut T {
        self.get_mut().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use std::{panic, sync::Arc, thread};

    use super::*;

    #[test]
    fn a_lock_poisoned_by_a_panic_still_hands_over_its_value() {
        let shared = Arc::new(Mutex::new(7));
        let held = shared.clone();
        let _ = thread::spawn(move || {
            let _guard = held.lock().unwrap();
            panic::panic_any("a thread panics holding the lock");
        })
        .join();

        assert!(shared.is_poisoned());
        assert_eq!(*shared.locked(), 7);
        *shared.locked() = 8;
        assert_eq!(*shared.locked(), 8);
    }
}
