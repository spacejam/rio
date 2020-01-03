//! A steamy river of uring. Fast IO with a API that doesn't make me eyes bleed. GPL-666'd.

use std::io;

mod fastlock;
mod promise;

#[cfg(target_os = "linux")]
mod io_uring;

#[cfg(target_os = "linux")]
pub use io_uring::Uring as Rio;

/// Create a new IO system.
pub fn new() -> io::Result<Rio> {
    Rio::new(256)
}
