use std::{
    future::Future,
    marker::PhantomData,
    pin::Pin,
    sync::{Arc, Condvar, Mutex},
    task::{Context, Poll, Waker},
};

use super::{io_uring::Cq, Measure, M};

#[derive(Debug)]
struct CompletionState<C> {
    done: bool,
    item: Option<C>,
    waker: Option<Waker>,
}

impl<C> Default for CompletionState<C> {
    fn default() -> CompletionState<C> {
        CompletionState {
            done: false,
            item: None,
            waker: None,
        }
    }
}

/// A Future value which may or may not be filled
#[derive(Debug)]
pub struct Completion<'a, C> {
    lifetime: PhantomData<&'a ()>,
    mu: Arc<Mutex<CompletionState<C>>>,
    cv: Arc<Condvar>,
    cq: Arc<Mutex<Cq>>,
}

/// The completer side of the Future
#[derive(Debug)]
pub struct Filler<C> {
    mu: Arc<Mutex<CompletionState<C>>>,
    cv: Arc<Condvar>,
}

/// Create a new `Filler` and the `Completion`
/// that will be filled by its completion.
pub fn pair<'a, C>(
    cq: Arc<Mutex<Cq>>,
) -> (Completion<'a, C>, Filler<C>) {
    let mu =
        Arc::new(Mutex::new(CompletionState::default()));
    let cv = Arc::new(Condvar::new());
    let future = Completion {
        lifetime: PhantomData,
        mu: mu.clone(),
        cv: cv.clone(),
        cq,
    };
    let filler = Filler { mu, cv };

    (future, filler)
}

impl<'a, C> Completion<'a, C> {
    /// Block on the `Completion`'s completion
    /// or dropping of the `Filler`
    pub fn wait(self) -> C {
        self.wait_inner().unwrap()
    }

    fn wait_inner(&self) -> Option<C> {
        let _ = Measure::new(&M.wait);
        loop {
            let mut inner = self.mu.lock().unwrap();

            if inner.item.is_some() {
                return inner.item.take();
            }

            if inner.done {
                return None;
            }

            drop(inner);

            if let Ok(mut cq) = self.cq.try_lock() {
                cq.reap_ready_cqes();
            }

            let mut inner = self.mu.lock().unwrap();

            if inner.item.is_some() {
                return inner.item.take();
            }

            if inner.done {
                return None;
            }

            drop(
                self.cv
                    .wait_timeout(
                        inner,
                        std::time::Duration::from_millis(
                            10,
                        ),
                    )
                    .unwrap(),
            );
        }
    }
}

impl<'a, C> Drop for Completion<'a, C> {
    fn drop(&mut self) {
        self.wait_inner();
    }
}

impl<'a, C> Future for Completion<'a, C> {
    type Output = C;

    fn poll(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Self::Output> {
        let mut state = self.mu.lock().unwrap();
        if state.item.is_some() {
            Poll::Ready(state.item.take().unwrap())
        } else {
            if !state.done {
                state.waker = Some(cx.waker().clone());
            }
            Poll::Pending
        }
    }
}

impl<C> Filler<C> {
    /// Complete the `Completion`
    pub fn fill(self, inner: C) {
        let mut state = self.mu.lock().unwrap();

        if let Some(waker) = state.waker.take() {
            waker.wake();
        }

        state.item = Some(inner);
        state.done = true;

        self.cv.notify_all();
    }
}
