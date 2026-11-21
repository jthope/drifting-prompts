//! The bridges' listeners. nsh has no change events for what the apps watch, so each looks at a
//! stated interval and tells TSX only of a change.

use std::{cell::RefCell, future::Future, rc::Rc, time::Duration};

use nsh_core::prelude::*;

/// Calls `listener` with `read()` now and whenever it reads differently, checking every `every`.
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

/// As [`listen`], for a slow `read`, awaited off the main thread; a `None` answer tells nothing.
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
