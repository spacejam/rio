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
    pub khead: *const AtomicU32,
    pub ktail: *const AtomicU32,
    pub kring_mask: *const libc::c_uint,
    pub kring_entries: *const libc::c_uint,
    pub kflags: *const libc::c_uint,
    pub kdropped: *const libc::c_uint,
    pub array: *mut libc::c_uint,
    pub sqes: *mut io_uring_sqe,
    pub sqe_head: libc::c_uint,
    pub sqe_tail: libc::c_uint,
    pub ring_sz: usize,
    pub ring_ptr: *const libc::c_void,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct Cq {
    pub khead: *const AtomicU32,
    pub ktail: *const AtomicU32,
    pub kring_mask: *const libc::c_uint,
    pub kring_entries: *const libc::c_uint,
    pub koverflow: *const AtomicU32,
    pub cqes: *mut io_uring_cqe,
    pub ring_sz: usize,
    pub ring_ptr: *const libc::c_void,
}
