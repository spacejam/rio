use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Condvar, Mutex},
    task::{Context, Poll, Waker},
};

use super::{io_uring::Cq, FastLock};

#[derive(Debug)]
struct CompletionState<T> {
    fused: bool,
    item: Option<T>,
    waker: Option<Waker>,
}

impl<T> Default for CompletionState<T> {
    fn default() -> CompletionState<T> {
        CompletionState {
            fused: false,
            item: None,
            waker: None,
        }
    }
}

/// A Future value which may or may not be filled
#[derive(Debug)]
pub struct Completion<T> {
    mu: Arc<Mutex<CompletionState<T>>>,
    cv: Arc<Condvar>,
}

/// The completer side of the Future
#[derive(Debug)]
pub struct CompletionFiller<T> {
    mu: Arc<Mutex<CompletionState<T>>>,
    cv: Arc<Condvar>,
    cq: Arc<FastLock<Cq>>,
}

/// Create a new `CompletionFiller` and the `Completion`
/// that will be filled by its completion.
pub fn pair<T>(
    cq: Arc<FastLock<Cq>>,
) -> (Completion<T>, CompletionFiller<T>) {
    let mu =
        Arc::new(Mutex::new(CompletionState::default()));
    let cv = Arc::new(Condvar::new());
    let future = Completion {
        mu: mu.clone(),
        cv: cv.clone(),
    };
    let filler = CompletionFiller { mu, cv, cq };

    (future, filler)
}

impl<T> Completion<T> {
    /// Block on the `Completion`'s completion
    /// or dropping of the `CompletionFiller`
    pub fn wait(self) -> T {
        let mut inner = self.mu.lock().unwrap();
        while inner.item.is_none() {
            inner = self.cv.wait(inner).unwrap();
        }
        inner.item.take().unwrap()
    }
}

impl<T> Future for Completion<T> {
    type Output = T;

    fn poll(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Self::Output> {
        let mut state = self.mu.lock().unwrap();
        if state.fused {
            return Poll::Pending;
        }
        if !state.fused && state.item.is_some() {
            state.fused = true;
            Poll::Ready(state.item.take().unwrap())
        } else {
            state.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

impl<T> CompletionFiller<T> {
    /// Complete the `Completion`
    pub fn fill(self, inner: T) {
        let mut state = self.mu.lock().unwrap();

        if let Some(waker) = state.waker.take() {
            waker.wake();
        }

        state.item = Some(inner);

        let _notified = self.cv.notify_all();
    }
}
