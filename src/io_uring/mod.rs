use std::{
    convert::TryFrom,
    fs::File,
    io::{self, IoSlice, IoSliceMut},
    os::unix::io::AsRawFd,
    slice::from_raw_parts_mut,
    sync::atomic::Ordering::{Acquire, Release},
};

mod constants;
mod kernel_types;
mod syscall;

use kernel_types::{
    io_uring_cqe, io_uring_params, io_uring_sqe,
};

use constants::*;

use syscall::{enter, setup};

use std::sync::atomic::AtomicU32;

/// Nice bindings for the shiny new linux IO system
#[derive(Debug)]
pub struct Uring {
    sq: Sq,
    cq: Cq,
    flags: u32,
    ring_fd: i32,
}

/// Sprays uring submissions.
#[derive(Debug)]
pub struct Sq {
    khead: &'static AtomicU32,
    ktail: &'static AtomicU32,
    kring_mask: u32,
    kring_entries: u32,
    kflags: *const libc::c_uint,
    kdropped: *const libc::c_uint,
    array: *mut libc::c_uint,
    sqes: &'static mut [io_uring_sqe],
    sqe_head: libc::c_uint,
    sqe_tail: libc::c_uint,
    ring_ptr: *const libc::c_void,
    ring_sz: usize,
    sqes_sz: usize,
}

impl Drop for Sq {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(
                self.sqes.as_ptr() as *mut libc::c_void,
                self.sqes_sz,
            );
        }
        unsafe {
            libc::munmap(
                self.ring_ptr as *mut libc::c_void,
                self.ring_sz,
            );
        }
    }
}

/// Consumes uring completions.
#[derive(Debug)]
pub struct Cq {
    pub khead: &'static AtomicU32,
    pub ktail: &'static AtomicU32,
    pub kring_mask: u32,
    pub kring_entries: u32,
    pub koverflow: *const AtomicU32,
    pub cqes: &'static mut [io_uring_cqe],
    pub ring_ptr: *const libc::c_void,
    pub ring_sz: usize,
}

impl Drop for Cq {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(
                self.ring_ptr as *mut libc::c_void,
                self.ring_sz,
            );
        }
    }
}

#[derive(Debug)]
pub struct Cqe<'a> {
    cqe: &'a io_uring_cqe,
    uring: &'a Uring,
    seen: bool,
}

impl<'a> std::ops::Deref for Cqe<'a> {
    type Target = io_uring_cqe;

    fn deref(&self) -> &io_uring_cqe {
        &self.cqe
    }
}

impl<'a> Cqe<'a> {
    pub fn status(&self) -> io::Result<()> {
        if self.cqe.res < 0 {
            Err(io::Error::from_raw_os_error(
                -1 * self.cqe.res,
            ))
        } else {
            Ok(())
        }
    }

    pub fn seen(mut self) {
        self.seen_inner();
    }

    fn seen_inner(&mut self) {
        if !self.seen {
            self.seen = true;
            self.uring.cq.khead.fetch_add(1, Release);
        }
    }
}

impl<'a> Drop for Cqe<'a> {
    fn drop(&mut self) {
        self.seen_inner();
    }
}

fn uring_mmap(
    size: usize,
    ring_fd: i32,
    offset: i64,
) -> *mut libc::c_void {
    unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED | libc::MAP_POPULATE,
            ring_fd,
            offset,
        ) as _
    }
}

impl io_uring_sqe {
    fn prep_rw(
        &mut self,
        op: u32,
        file: &File,
        addr: *mut libc::c_void,
        len: usize,
        at: u64,
    ) {
        *self = io_uring_sqe {
            opcode: u8::try_from(op).unwrap(),
            flags: 0,
            ioprio: 0,
            fd: file.as_raw_fd(),
            addr: addr as u64,
            len: u32::try_from(len).unwrap(),
            user_data: 0,
            off: u64::try_from(at).unwrap(),
            ..*self
        };
        self.__bindgen_anon_1.rw_flags = 0;
        self.__bindgen_anon_2.__pad2 = [0; 3];
    }
}

impl Uring {
    pub fn new(depth: usize) -> io::Result<Uring> {
        let mut params = io_uring_params::default();

        let ring_fd =
            setup(depth as _, &mut params as *mut _)?;

        if ring_fd < 0 {
            return Err(io::Error::last_os_error());
        }

        let sq_ring_sz = params.sq_off.array as usize
            + (params.sq_entries as usize
                * std::mem::size_of::<u32>());

        // TODO IORING_FEAT_SINGLE_MMAP for sq

        let sq_ring_ptr = uring_mmap(
            sq_ring_sz,
            ring_fd,
            IORING_OFF_SQ_RING as libc::off_t,
        );

        if sq_ring_ptr.is_null()
            || sq_ring_ptr == libc::MAP_FAILED
        {
            return Err(io::Error::last_os_error());
        }

        // size = p->sq_entries * sizeof(struct io_uring_sqe);
        let sqes_sz: usize = params.sq_entries as usize
            * std::mem::size_of::<io_uring_sqe>();

        let sqes_ptr: *mut io_uring_sqe = uring_mmap(
            sqes_sz,
            ring_fd,
            IORING_OFF_SQES as libc::off_t,
        ) as _;

        if sqes_ptr.is_null()
            || sqes_ptr
                == libc::MAP_FAILED as *mut io_uring_sqe
        {
            return Err(io::Error::last_os_error());
        }

        let sq = unsafe {
            Sq {
                sqe_head: 0,
                sqe_tail: 0,
                ring_ptr: sq_ring_ptr,
                ring_sz: sq_ring_sz,
                sqes_sz: sqes_sz,
                sqes: from_raw_parts_mut(
                    sqes_ptr,
                    params.sq_entries as usize,
                ),
                khead: &*(sq_ring_ptr
                    .add(params.sq_off.head as usize)
                    as *const AtomicU32),
                ktail: &*(sq_ring_ptr
                    .add(params.sq_off.tail as usize)
                    as *const AtomicU32),
                kring_mask: *(sq_ring_ptr
                    .add(params.sq_off.ring_mask as usize)
                    as *mut u32),
                kring_entries: *(sq_ring_ptr.add(
                    params.sq_off.ring_entries as usize,
                )
                    as *const u32),
                kflags: sq_ring_ptr
                    .add(params.sq_off.flags as usize)
                    as _,
                kdropped: sq_ring_ptr
                    .add(params.sq_off.dropped as usize)
                    as _,
                array: sq_ring_ptr
                    .add(params.sq_off.array as usize)
                    as _,
            }
        };

        // TODO IORING_FEAT_SINGLE_MMAP for cq
        let cq_ring_sz = params.cq_off.cqes as usize
            + (params.cq_entries as usize
                * std::mem::size_of::<u32>());

        let cq_ring_ptr = uring_mmap(
            cq_ring_sz,
            ring_fd,
            IORING_OFF_CQ_RING as libc::off_t,
        );

        if cq_ring_ptr.is_null()
            || cq_ring_ptr == libc::MAP_FAILED
        {
            return Err(io::Error::last_os_error());
        }

        let cq = unsafe {
            Cq {
                ring_ptr: cq_ring_ptr,
                ring_sz: cq_ring_sz,
                khead: &*(cq_ring_ptr
                    .add(params.cq_off.head as usize)
                    as *const AtomicU32),
                ktail: &*(cq_ring_ptr
                    .add(params.cq_off.tail as usize)
                    as *const AtomicU32),
                kring_mask: *(cq_ring_ptr
                    .add(params.cq_off.ring_mask as usize)
                    as *mut u32),
                kring_entries: *(cq_ring_ptr.add(
                    params.cq_off.ring_entries as usize,
                )
                    as *const u32),
                koverflow: cq_ring_ptr
                    .add(params.cq_off.overflow as usize)
                    as _,
                cqes: from_raw_parts_mut(
                    cq_ring_ptr
                        .add(params.cq_off.cqes as usize)
                        as _,
                    params.cq_off.cqes as usize,
                ),
            }
        };

        Ok(Uring {
            flags: params.flags,
            ring_fd,
            cq,
            sq,
        })
    }

    /// Sync the file. This does not work with O_DIRECT.
    ///
    /// # Examples
    ///
    /// ```
    /// ring.enqueue_fsync(&file);
    /// ring.submit_all().expect("submit");
    /// let cqe = ring.wait_cqe().unwrap();
    /// cqe.seen();
    /// ```
    pub fn enqueue_fsync(&mut self, file: &File) -> bool {
        if let Some(sqe) = self.get_sqe() {
            sqe.prep_rw(
                IORING_OP_FSYNC,
                file,
                std::ptr::null_mut(),
                0,
                0,
            );
            true
        } else {
            false
        }
    }

    pub fn enqueue_write(
        &mut self,
        file: &File,
        iov: &IoSlice,
        at: u64,
    ) -> bool {
        if let Some(sqe) = self.get_sqe() {
            sqe.prep_rw(
                IORING_OP_WRITEV,
                file,
                iov as *const _ as _,
                1,
                at,
            );
            true
        } else {
            false
        }
    }

    pub fn enqueue_read(
        &mut self,
        file: &File,
        iov: &mut IoSliceMut,
        at: u64,
    ) -> bool {
        if let Some(sqe) = self.get_sqe() {
            sqe.prep_rw(
                IORING_OP_READV,
                file,
                iov as *mut _ as _,
                1,
                at,
            );
            true
        } else {
            false
        }
    }

    pub(crate) fn get_sqe(
        &mut self,
    ) -> Option<&mut io_uring_sqe> {
        let next = self.sq.sqe_tail + 1;

        if (self.flags & IORING_SETUP_SQPOLL) == 0 {
            // non-polling mode
            let head = self.sq.sqe_head;
            if next - head <= self.sq.kring_entries {
                let idx =
                    self.sq.sqe_tail & self.sq.kring_mask;
                let ret = &mut self.sq.sqes[idx as usize];
                self.sq.sqe_tail = next;
                Some(ret)
            } else {
                None
            }
        } else {
            // polling mode
            todo!()
        }
    }

    fn flush(&mut self) -> u32 {
        let mask: u32 = self.sq.kring_mask;
        if self.sq.sqe_head == self.sq.sqe_tail {
            return 0;
        }

        let mut ktail = self.sq.ktail.load(Acquire);
        let to_submit = self.sq.sqe_tail - self.sq.sqe_head;
        for _ in 0..to_submit {
            let index = ktail & mask;
            unsafe {
                *(self.sq.array.add(index as usize)) =
                    self.sq.sqe_head & mask;
            }
            ktail += 1;
            self.sq.sqe_head += 1;
        }

        let swapped = self.sq.ktail.swap(ktail, Release);

        assert_eq!(swapped, ktail - to_submit);

        to_submit
    }

    pub fn submit_all(&mut self) -> io::Result<()> {
        // TODO skip submission if we don't need to do it
        // TODO for polling, keep flags at 0
        let flags = IORING_ENTER_GETEVENTS;
        let mut submitted = self.flush();
        while submitted > 0 {
            let ret = enter(
                self.ring_fd,
                submitted,
                0,
                flags,
                std::ptr::null_mut(),
            )?;
            submitted -= u32::try_from(ret).unwrap();
        }
        Ok(())
    }

    pub fn wait_cqe(&self) -> io::Result<Cqe> {
        loop {
            if let Some(index) = self.peek_cqe() {
                let cqe = &self.cq.cqes[index as usize];
                return if cqe.res < 0 {
                    Err(io::Error::from_raw_os_error(
                        -1 * cqe.res,
                    ))
                } else {
                    Ok(Cqe {
                        cqe,
                        uring: self,
                        seen: false,
                    })
                };
            }
            self.block_for_cqe()?;
        }
    }

    /// Grabs a completed `io_uring_cqe` if it's available
    pub(crate) fn peek_cqe(&self) -> Option<usize> {
        let head = self.cq.khead.load(Acquire);
        let tail = self.cq.ktail.load(Acquire);

        if head != tail {
            let index = head & self.cq.kring_mask;
            Some(index as usize)
        } else {
            None
        }
    }

    fn block_for_cqe(&self) -> io::Result<()> {
        let flags = IORING_ENTER_GETEVENTS;
        let submit = 0;
        let wait = 1;
        let sigset = std::ptr::null_mut();

        enter(self.ring_fd, submit, wait, flags, sigset)?;

        Ok(())
    }
}
