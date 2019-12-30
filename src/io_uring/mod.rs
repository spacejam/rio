use std::{
    convert::TryFrom,
    fs::File,
    io::{self, IoSlice, IoSliceMut},
    os::unix::io::AsRawFd,
    sync::atomic::Ordering::{Acquire, Release},
};

mod constants;
mod syscall;
mod types;

pub use types::{Cqe, Params, Sqe, Uring};

pub(crate) use constants::{
    IORING_ENTER_GETEVENTS, IORING_OFF_CQ_RING,
    IORING_OFF_SQES, IORING_OFF_SQ_RING, IORING_OP_FSYNC,
    IORING_OP_READV, IORING_OP_WRITEV, IORING_SETUP_SQPOLL,
};

use syscall::{enter, setup};

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

impl Uring {
    pub fn new(depth: usize) -> io::Result<Uring> {
        let mut params = Params::default();

        let ring_fd = unsafe {
            setup(depth as _, &mut params as *mut _)
        };
        if ring_fd < 0 {
            println!("src/io_uring/mod.rs:50");
            return Err(io::Error::last_os_error());
        }

        let mut uring: Uring =
            unsafe { std::mem::zeroed() };

        uring.sq.ring_sz = params.sq_off.array as usize
            + (params.sq_entries as usize
                * std::mem::size_of::<u32>());

        uring.cq.ring_sz = params.cq_off.cqes as usize
            + (params.cq_entries as usize
                * std::mem::size_of::<u32>());

        // TODO IORING_FEAT_SINGLE_MMAP for sq

        let sq_ring_ptr = uring_mmap(
            uring.sq.ring_sz,
            ring_fd,
            IORING_OFF_SQ_RING as libc::off_t,
        );

        if sq_ring_ptr.is_null()
            || sq_ring_ptr == libc::MAP_FAILED
        {
            println!("src/io_uring/mod.rs:80");
            return Err(io::Error::last_os_error());
        }

        uring.sq.ring_ptr = sq_ring_ptr;

        // TODO IORING_FEAT_SINGLE_MMAP for cq

        let cq_ring_ptr = uring_mmap(
            uring.cq.ring_sz,
            ring_fd,
            IORING_OFF_CQ_RING as libc::off_t,
        );

        if cq_ring_ptr.is_null()
            || cq_ring_ptr == libc::MAP_FAILED
        {
            println!("src/io_uring/mod.rs:102");
            return Err(io::Error::last_os_error());
        }

        uring.cq.ring_ptr = cq_ring_ptr;

        // sq->khead = sq->ring_ptr + p->sq_off.head;
        uring.sq.khead = unsafe {
            uring
                .sq
                .ring_ptr
                .add(params.sq_off.head as usize)
                as _
        };

        // sq->ktail = sq->ring_ptr + p->sq_off.tail;
        uring.sq.ktail = unsafe {
            uring
                .sq
                .ring_ptr
                .add(params.sq_off.tail as usize)
                as _
        };

        // sq->kring_mask = sq->ring_ptr + p->sq_off.ring_mask;
        uring.sq.kring_mask = unsafe {
            uring
                .sq
                .ring_ptr
                .add(params.sq_off.ring_mask as usize)
                as _
        };

        // sq->kring_entries = sq->ring_ptr + p->sq_off.ring_entries;
        uring.sq.kring_entries =
            unsafe {
                uring.sq.ring_ptr.add(
                    params.sq_off.ring_entries as usize,
                ) as _
            };

        // sq->kflags = sq->ring_ptr + p->sq_off.flags;
        uring.sq.kflags = unsafe {
            uring
                .sq
                .ring_ptr
                .add(params.sq_off.flags as usize)
                as _
        };

        // sq->kdropped = sq->ring_ptr + p->sq_off.dropped;
        uring.sq.kdropped = unsafe {
            uring
                .sq
                .ring_ptr
                .add(params.sq_off.dropped as usize)
                as _
        };

        // sq->array = sq->ring_ptr + p->sq_off.array;
        uring.sq.array = unsafe {
            uring
                .sq
                .ring_ptr
                .add(params.sq_off.array as usize)
                as _
        };

        // size = p->sq_entries * sizeof(struct io_uring_sqe);
        let size: usize = params.sq_entries as usize
            * std::mem::size_of::<Sqe>();

        let sqes_ptr: *mut Sqe = uring_mmap(
            size,
            ring_fd,
            IORING_OFF_SQES as libc::off_t,
        ) as _;

        if sqes_ptr.is_null()
            || sqes_ptr == libc::MAP_FAILED as *mut Sqe
        {
            println!("src/io_uring/mod.rs:189");
            return Err(io::Error::last_os_error());
        }

        uring.sq.sqes = sqes_ptr;

        // cq->khead = cq->ring_ptr + p->cq_off.head;
        uring.cq.khead = unsafe {
            uring
                .cq
                .ring_ptr
                .add(params.cq_off.head as usize)
                as _
        };

        // cq->ktail = cq->ring_ptr + p->cq_off.tail;
        uring.cq.ktail = unsafe {
            uring
                .cq
                .ring_ptr
                .add(params.cq_off.tail as usize)
                as _
        };

        // cq->kring_mask = cq->ring_ptr + p->cq_off.ring_mask;
        uring.cq.kring_mask = unsafe {
            uring
                .cq
                .ring_ptr
                .add(params.cq_off.ring_mask as usize)
                as _
        };

        // cq->kring_entries = cq->ring_ptr + p->cq_off.ring_entries;
        uring.cq.kring_entries =
            unsafe {
                uring.cq.ring_ptr.add(
                    params.cq_off.ring_entries as usize,
                ) as _
            };

        // cq->koverflow = cq->ring_ptr + p->cq_off.overflow;
        uring.cq.koverflow = unsafe {
            uring
                .cq
                .ring_ptr
                .add(params.cq_off.overflow as usize)
                as _
        };

        // cq->cqes = cq->ring_ptr + p->cq_off.cqes;
        uring.cq.cqes = unsafe {
            uring
                .cq
                .ring_ptr
                .add(params.cq_off.cqes as usize)
                as _
        };

        uring.flags = params.flags;
        uring.ring_fd = ring_fd;

        Ok(uring)
    }

    pub(crate) fn get_sqe(&mut self) -> Option<&mut Sqe> {
        let next = self.sq.sqe_tail + 1;
        println!("next is {}", next);

        if (self.flags & IORING_SETUP_SQPOLL) == 0 {
            // non-polling mode
            let head = self.sq.sqe_head;
            println!("head is {:?}", head);
            println!(
                "kring_entries is {:?}",
                self.sq.kring_entries
            );
            if next - head
                <= unsafe { *self.sq.kring_entries }
            {
                let idx = self.sq.sqe_tail
                    & unsafe { *self.sq.kring_mask };
                let ret = unsafe {
                    self.sq.sqes.add(idx as usize)
                };
                self.sq.sqe_tail = next;
                unsafe { Some(&mut *ret) }
            } else {
                println!("src/io_uring/mod.rs:88");
                None
            }
        } else {
            // polling mode
            todo!()
        }
    }

    pub fn enqueue_fsync(&mut self, file: &File) -> bool {
        let mut sqe = if let Some(sqe) = self.get_sqe() {
            sqe
        } else {
            return false;
        };
        sqe.opcode = IORING_OP_FSYNC as u8;
        sqe.fd = file.as_raw_fd();
        sqe.addr = 0;
        sqe.len = 0;
        sqe.__bindgen_anon_1.off = 0;
        sqe.flags = 0;
        sqe.ioprio = 0;
        sqe.__bindgen_anon_2.rw_flags = 0;
        sqe.user_data = 0;
        sqe.__bindgen_anon_3.__pad2 = [0; 3];

        true
    }

    pub fn enqueue_write(
        &mut self,
        file: &File,
        iov: IoSlice,
        at: usize,
    ) -> bool {
        let mut sqe = if let Some(sqe) = self.get_sqe() {
            sqe
        } else {
            return false;
        };
        sqe.opcode = IORING_OP_WRITEV as u8;
        sqe.fd = file.as_raw_fd();
        sqe.addr = iov.as_ptr() as _;
        sqe.len = iov.len() as _;
        sqe.__bindgen_anon_1.off = at as _;
        sqe.flags = 0;
        sqe.ioprio = 0;
        sqe.__bindgen_anon_2.rw_flags = 0;
        sqe.user_data = 0;
        sqe.__bindgen_anon_3.__pad2 = [0; 3];

        true
    }

    pub fn enqueue_read(
        &mut self,
        file: &File,
        iov: IoSliceMut,
        at: usize,
    ) -> bool {
        let mut sqe = if let Some(sqe) = self.get_sqe() {
            sqe
        } else {
            return false;
        };
        sqe.opcode = IORING_OP_READV as u8;
        sqe.fd = file.as_raw_fd();
        sqe.addr = iov.as_ptr() as _;
        sqe.len = iov.len() as _;
        sqe.__bindgen_anon_1.off = at as _;
        sqe.flags = 0;
        sqe.ioprio = 0;
        sqe.__bindgen_anon_2.rw_flags = 0;
        sqe.user_data = 0;
        sqe.__bindgen_anon_3.__pad2 = [0; 3];

        true
    }

    fn flush(&mut self) -> u32 {
        let mask: u32 = unsafe { *self.sq.kring_mask };
        if self.sq.sqe_head == self.sq.sqe_tail {
            return 0;
        }

        let mut ktail =
            unsafe { (*self.sq.ktail).load(Acquire) };
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

        let swapped = unsafe {
            (*self.sq.ktail).swap(ktail, Release)
        };

        assert_eq!(swapped, ktail - to_submit);

        to_submit
    }

    pub fn submit_all(&mut self) -> io::Result<()> {
        // TODO skip submission if we don't need to do it
        // TODO for polling, keep flags at 0
        let flags = IORING_ENTER_GETEVENTS;
        let mut submitted = self.flush();
        while submitted > 0 {
            let ret = unsafe {
                enter(
                    self.ring_fd,
                    submitted,
                    0,
                    flags,
                    std::ptr::null_mut(),
                )
            };
            if ret < 0 {
                println!("enter call not supported");
                return Err(io::Error::last_os_error());
            }
            submitted -= u32::try_from(ret).unwrap();
        }
        Ok(())
    }

    pub fn wait_cqe<'a>(
        &mut self,
    ) -> io::Result<&'a mut Cqe> {
        loop {
            if let Some(cqe) = self.peek_cqe() {
                return if cqe.res < 0 {
                    Err(io::Error::from_raw_os_error(
                        -1 * cqe.res,
                    ))
                } else {
                    Ok(cqe)
                };
            } else {
                self.wait_for_cqe()?;
            }
        }
    }

    /// Grabs a completed `Cqe` if it's available
    pub(crate) fn peek_cqe<'a>(
        &mut self,
    ) -> Option<&'a mut Cqe> {
        let head =
            unsafe { (*self.cq.khead).load(Acquire) };
        let tail =
            unsafe { (*self.cq.ktail).load(Acquire) };

        if head != tail {
            let index =
                head & unsafe { *self.cq.kring_mask };
            let cqe = unsafe {
                &mut *self.cq.cqes.add(index as usize)
            };
            Some(cqe)
        } else {
            None
        }
    }

    fn wait_for_cqe(&mut self) -> io::Result<()> {
        let flags = IORING_ENTER_GETEVENTS;
        let submit = 0;
        let wait = 1;
        let sigset = std::ptr::null_mut();

        let ret = unsafe {
            enter(self.ring_fd, submit, wait, flags, sigset)
        };
        if ret < 0 {
            return Err(io::Error::last_os_error());
        }

        Ok(())
    }

    pub fn seen(&mut self, _cqe: &mut Cqe) {
        unsafe {
            (*self.cq.khead).fetch_add(1, Release);
        }
    }
}
