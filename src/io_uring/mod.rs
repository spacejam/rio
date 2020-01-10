use std::{
    cell::UnsafeCell,
    convert::TryFrom,
    fs::File,
    io,
    ops::Neg,
    os::unix::io::AsRawFd,
    sync::{
        atomic::{
            AtomicU32,
            Ordering::{Acquire, Relaxed, Release},
        },
        Arc, Condvar, Mutex,
    },
};

use super::{
    pair, AsIoVec, Completion, Filler, Measure, M,
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

use {
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

pub use {config::Config, uring::Uring};

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
) -> *mut libc::c_void {
    #[allow(unsafe_code)]
    unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED | libc::MAP_POPULATE,
            ring_fd,
            offset,
        )
    }
}

impl io_uring_sqe {
    fn prep_rw(
        &mut self,
        opcode: u8,
        file_descriptor: i32,
        len: usize,
        off: u64,
        ordering: Ordering,
    ) {
        *self = io_uring_sqe {
            opcode,
            flags: 0,
            ioprio: 0,
            fd: file_descriptor,
            len: u32::try_from(len).unwrap(),
            off,
            ..*self
        };

        self.__bindgen_anon_1.rw_flags = 0;
        self.__bindgen_anon_2.__pad2 = [0; 3];

        self.apply_order(ordering);
    }

    fn apply_order(&mut self, ordering: Ordering) {
        match ordering {
            Ordering::None => {}
            Ordering::Link => self.flags |= IOSQE_IO_LINK,
            Ordering::Drain => self.flags |= IOSQE_IO_DRAIN,
        }
    }
}
