//! A steamy river of uring. Fast IO with a API that doesn't make me eyes bleed. GPL-666'd.

use std::io;

mod completion;
mod fastlock;

#[cfg(target_os = "linux")]
mod io_uring;

#[cfg(target_os = "linux")]
pub use io_uring::{Ordering, Uring as Rio};

pub use completion::Completion;

use {
    completion::{pair, CompletionFiller},
    fastlock::FastLock,
};

/// Create a new IO system.
pub fn new() -> io::Result<Rio> {
    Rio::new(256)
}
