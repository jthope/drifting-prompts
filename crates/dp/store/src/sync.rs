//! Taking a lock that a panicking thread left poisoned.
//!
//! Rust marks a `Mutex` as poisoned when a thread panics while holding it, and `lock` returns an
//! error from then on. What the apps guard is a value that is replaced whole, a list, or a
//! recorder that checks its own state, so what a poisoned lock holds is still usable. Refusing it
//! would be worse: a frozen picture, a status that never changes again, a save that never happens.
//! Every lock in the suite goes through [`LockExt`], so they all recover the same way.
//!
//! In C# terms, [`LockExt`] is an extension method on `Mutex<T>`: bring the trait into scope with
//! `use dp_store::sync::LockExt;` and every `Mutex` has `locked()`.

use std::sync::{Mutex, MutexGuard, PoisonError};

/// Locking that carries on past a panic in another thread.
pub trait LockExt<T> {
    /// Takes the lock, even if a thread panicked holding it.
    fn locked(&self) -> MutexGuard<'_, T>;

    /// The value, for whoever has the `Mutex` to itself, so no lock is needed; even if a thread
    /// panicked holding it.
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
