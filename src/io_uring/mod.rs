use std::{
    cell::UnsafeCell,
    convert::TryFrom,
    fs::File,
    io,
    net::{TcpListener, TcpStream},
    ops::Neg,
    os::unix::io::{AsRawFd, FromRawFd},
    sync::{
        atomic::{
            AtomicU32, AtomicU64,
            Ordering::{Acquire, Relaxed, Release},
        },
        Arc, Condvar, Mutex,
    },
};

use super::{
    pair, AsIoVec, AsIoVecMut, Completion, Filler, FromCqe,
    Measure, M,
};

mod config;
mod constants;
mod cq;
mod in_flight;
mod kernel_types;
mod sq;
mod syscall;
mod ticket_queue;
mod uring;

pub(crate) use {
    constants::*,
    cq::Cq,
    in_flight::InFlight,
    kernel_types::{
        io_uring_cqe, io_uring_params, io_uring_sqe,
    },
    sq::Sq,
    syscall::{enter, setup},
    ticket_queue::TicketQueue,
};

pub use {
    config::Config,
    uring::{Rio, Uring},
};

/// Specify whether `io_uring` should
/// run operations in a specific order.
/// By default, it will run independent
/// operations in any order it can to
/// speed things up. This can be constrained
/// by either submitting chains of `Link`
/// events, which are executed one after the other,
/// or by specifying the `Drain` ordering
/// which causes all previously submitted operations
/// to complete first.
#[derive(Clone, Debug, Copy)]
pub enum Ordering {
    /// No ordering requirements
    None,
    /// `Ordering::Link` causes the next
    /// submitted operation to wait until
    /// this one finishes. Useful for
    /// things like file copy, fsync-after-write,
    /// or proxies.
    Link,
    /// `Ordering::Drain` causes all previously
    /// submitted operations to complete before
    /// this one begins.
    Drain,
}

fn uring_mmap(
    size: usize,
    ring_fd: i32,
    offset: i64,
) -> io::Result<*mut libc::c_void> {
    #[allow(unsafe_code)]
    let ptr = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED | libc::MAP_POPULATE,
            ring_fd,
            offset,
        )
    };

    if ptr.is_null() || ptr == libc::MAP_FAILED {
        let mut err = io::Error::last_os_error();
        if let Some(12) = err.raw_os_error() {
            err = io::Error::new(
                io::ErrorKind::Other,
                "Not enough lockable memory. You probably \
                 need to raise the memlock rlimit, which \
                 often defaults to a pretty low number.",
            );
        }
        return Err(err);
    }

    Ok(ptr)
}

impl FromCqe for TcpStream {
    fn from_cqe(cqe: io_uring_cqe) -> TcpStream {
        #[allow(unsafe_code)]
        unsafe {
            TcpStream::from_raw_fd(cqe.res)
        }
    }
}
