//! The listeners a bridge hands TSX: `on_<what>(listener, cx) -> Stop` calls the listener now and
//! after every change, until the returned function is called or what registered it goes.
//!
//! nsh has no change events for what the apps watch (a camera's state, a folder, a file), so each
//! listener looks at a stated interval and tells TSX only when what it reads differs from last
//! time. [`listen`] is for a quick read, made on the main thread; [`listen_later`] for one that
//! takes a moment (a folder's listing), awaited off it.

use std::{cell::RefCell, future::Future, rc::Rc, time::Duration};

use nsh_core::prelude::*;

/// Calls `listener` with `read()` now and whenever it reads differently, checking every `every`,
/// until the returned `Stop` is called or what called this goes.
pub fn listen<T: Clone + PartialEq + JsValue + 'static>(
    cx: CallCx<'_>,
    listener: Callback<T>,
    every: Duration,
    read: impl Fn() -> T + 'static,
) -> Stop {
    let listener = Rc::new(RefCell::new(Some(listener)));
    let held = listener.clone();
    cx.start(async move {
        let mut last = None;
        loop {
            let Some(listener) = held.borrow().clone() else {
                return;
            };
            let now = read();
            if last.as_ref() != Some(&now) {
                listener.post(now.clone());
                last = Some(now);
            }
            timer(every).await;
        }
    });
    Stop::new(move || drop(listener.take()))
}

/// As [`listen`], for a value that takes a moment to read, such as a folder's listing: `read` is
/// awaited, so the main thread carries on meanwhile, and the next check comes `every` after the
/// answer. A read that answers `None` couldn't be made this time, and tells the listener nothing.
pub fn listen_later<T, R>(
    cx: CallCx<'_>,
    listener: Callback<T>,
    every: Duration,
    read: impl Fn() -> R + 'static,
) -> Stop
where
    T: Clone + PartialEq + JsValue + 'static,
    R: Future<Output = Option<T>> + 'static,
{
    let listener = Rc::new(RefCell::new(Some(listener)));
    let held = listener.clone();
    cx.start(async move {
        let mut last = None;
        loop {
            if held.borrow().is_none() {
                return;
            }
            let now = read().await;
            // Stopped while the read was on its way.
            let Some(listener) = held.borrow().clone() else {
                return;
            };
            if let Some(now) = now
                && last.as_ref() != Some(&now)
            {
                listener.post(now.clone());
                last = Some(now);
            }
            timer(every).await;
        }
    });
    Stop::new(move || drop(listener.take()))
}
