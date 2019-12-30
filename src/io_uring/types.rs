use super::{io_uring_cqe, io_uring_sqe};

use std::sync::atomic::AtomicU32;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct Uring {
    pub sq: Sq,
    pub cq: Cq,
    pub flags: libc::c_uint,
    pub ring_fd: libc::c_int,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct Sq {
    pub khead: *mut AtomicU32,
    pub ktail: *mut AtomicU32,
    pub kring_mask: *mut libc::c_uint,
    pub kring_entries: *mut libc::c_uint,
    pub kflags: *mut libc::c_uint,
    pub kdropped: *mut libc::c_uint,
    pub array: *mut libc::c_uint,
    pub sqes: *mut io_uring_sqe,
    pub sqe_head: libc::c_uint,
    pub sqe_tail: libc::c_uint,
    pub ring_sz: usize,
    pub ring_ptr: *mut libc::c_void,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct Cq {
    pub khead: *mut AtomicU32,
    pub ktail: *mut AtomicU32,
    pub kring_mask: *mut libc::c_uint,
    pub kring_entries: *mut libc::c_uint,
    pub koverflow: *mut libc::c_uint,
    pub cqes: *mut io_uring_cqe,
    pub ring_sz: usize,
    pub ring_ptr: *mut libc::c_void,
}
