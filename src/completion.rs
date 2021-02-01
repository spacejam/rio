use std::{
    future::Future,
    io,
    marker::PhantomData,
    pin::Pin,
    sync::{Arc, Condvar, Mutex},
    task::{Context, Poll, Waker},
};

use super::{
    CqeData, FromCqeData, Measure, Uring, M,
};

#[derive(Debug)]
struct CompletionState {
    done: bool,
    item: Option<io::Result<CqeData>>,
    waker: Option<Waker>,
}

impl Default for CompletionState {
    fn default() -> CompletionState {
        CompletionState {
            done: false,
            item: None,
            waker: None,
        }
    }
}

/// A Future value which may or may not be filled
///
/// # Safety
///
/// To prevent undefined behavior in the form of
/// use-after-free, never allow a Completion's
/// lifetime to end without dropping it. This can
/// happen with `std::mem::forget`, cycles in
/// `Arc` or `Rc`, and in other ways.
#[derive(Debug)]
pub struct Completion<'a, C: FromCqeData> {
    lifetime: PhantomData<&'a C>,
    mu: Arc<Mutex<CompletionState>>,
    cv: Arc<Condvar>,
    uring: &'a Uring,
    pub(crate) sqe_id: u64,
}

/// The completer side of the Future
#[derive(Debug)]
pub struct Filler {
    mu: Arc<Mutex<CompletionState>>,
    cv: Arc<Condvar>,
}

/// Create a new `Filler` and the `Completion`
/// that will be filled by its completion.
pub fn pair<'a, C: FromCqeData>(
    uring: &'a Uring,
) -> (Completion<'a, C>, Filler) {
    let mu =
        Arc::new(Mutex::new(CompletionState::default()));
    let cv = Arc::new(Condvar::new());
    let future = Completion {
        lifetime: PhantomData,
        mu: mu.clone(),
        cv: cv.clone(),
        sqe_id: 0,
        uring,
    };
    let filler = Filler { mu, cv };

    (future, filler)
}

impl<'a, C: FromCqeData> Completion<'a, C> {
    /// Block on the `Completion`'s completion
    /// or dropping of the `Filler`
    pub fn wait(self) -> io::Result<C>
    where
        C: FromCqeData,
    {
        self.wait_inner().unwrap()
    }

    fn wait_inner(&self) -> Option<io::Result<C>>
    where
        C: FromCqeData,
    {
        debug_assert_ne!(
            self.sqe_id,
            0,
            "sqe_id was never filled-in for this Completion",
        );

        self.uring
            .ensure_submitted(self.sqe_id)
            .expect("failed to submit SQE from wait_inner");

        let _ = Measure::new(&M.wait);

        let mut inner = self.mu.lock().unwrap();

        while !inner.done {
            inner = self.cv.wait(inner).unwrap();
        }

        inner.item.take().map(|io_result| {
            io_result.map(FromCqeData::from_cqe_data)
        })
    }
}

impl<'a, C: FromCqeData> Drop for Completion<'a, C> {
    fn drop(&mut self) {
        self.wait_inner();
    }
}

impl<'a, C: FromCqeData> Future for Completion<'a, C> {
    type Output = io::Result<C>;

    fn poll(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Self::Output> {
        self.uring
            .ensure_submitted(self.sqe_id)
            .expect("failed to submit SQE from wait_inner");

        let mut state = self.mu.lock().unwrap();
        if state.item.is_some() {
            Poll::Ready(
                state
                    .item
                    .take()
                    .unwrap()
                    .map(FromCqeData::from_cqe_data),
            )
        } else {
            if !state.done {
                state.waker = Some(cx.waker().clone());
            }
            Poll::Pending
        }
    }
}

impl Filler {
    /// Complete the `Completion`
    pub fn fill(self, inner: io::Result<CqeData>) {
        let mut state = self.mu.lock().unwrap();

        if let Some(waker) = state.waker.take() {
            waker.wake();
        }

        state.item = Some(inner);
        state.done = true;

        self.cv.notify_all();
    }
}
