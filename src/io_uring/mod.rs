use std::{
    convert::TryFrom,
    fs::File,
    io::{self, IoSlice, IoSliceMut},
    os::unix::io::AsRawFd,
    sync::atomic::Ordering::{Acquire, Release},
};

use libc::{c_void, mmap};

mod io_uring;
mod syscall;

pub use io_uring::{
    Cqe, CqringOffsets, Params, Sqe, SqringOffsets, Uring,
    IORING_ENTER_GETEVENTS, IORING_OFF_SQ_RING,
    IORING_OP_READV, IORING_OP_WRITEV, IORING_SETUP_SQPOLL,
};

use syscall::{enter, register, setup};

pub struct MyUring {
    ptr: *mut c_void,
    params: Params,
    pending: usize,
}

impl std::ops::Deref for MyUring {
    type Target = Uring;

    fn deref(&self) -> &Uring {
        unsafe { &*(self.ptr as *mut Uring) }
    }
}

impl std::ops::DerefMut for MyUring {
    fn deref_mut(&mut self) -> &mut Uring {
        unsafe { &mut *(self.ptr as *mut Uring) }
    }
}

impl MyUring {
    pub fn new(depth: usize) -> MyUring {
        let mut params: Params =
            unsafe { std::mem::zeroed() };
        let ring_fd = unsafe {
            setup(depth as _, &mut params as *mut Params)
        };

        let mmap_sz: libc::size_t = params.sq_off.array
            as libc::size_t
            + (params.sq_entries as libc::size_t
                * std::mem::size_of::<u32>());

        let ptr: *mut c_void = unsafe {
            mmap(
                std::ptr::null_mut(),
                mmap_sz,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED | libc::MAP_POPULATE,
                ring_fd,
                IORING_OFF_SQ_RING as libc::off_t,
            ) as _
        };

        MyUring {
            ptr,
            params,
            pending: 0,
        }
    }

    pub fn get_sqe(&mut self) -> Option<&mut Sqe> {
        let next = self.sq.sqe_tail + 1;

        if (self.flags() & IORING_SETUP_SQPOLL) == 0 {
            // non-polling mode
            let head = self.sq.sqe_head;
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
                None
            }
        } else {
            // polling mode
            todo!()
        }
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

        self.pending += 1;

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

        self.pending += 1;

        true
    }

    fn flush(&mut self) -> u32 {
        let mask: u32 = unsafe { *self.sq.kring_mask };
        if self.sq.sqe_head == self.sq.sqe_tail {
            return 0;
        }

        let mut ktail =
            unsafe { (*self.sq.ktail).load(Acquire) };
        let mut to_submit =
            self.sq.sqe_tail - self.sq.sqe_head;
        for index in (0..to_submit).rev() {
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
                return Err(io::Error::last_os_error());
            }
            submitted -= u32::try_from(ret).unwrap();
        }
        Ok(())
    }

    pub fn head(&self) -> *mut c_void {
        unsafe {
            self.ptr.add(self.params.sq_off.head as usize)
        }
    }
    pub fn tail(&self) -> *mut c_void {
        unsafe {
            self.ptr.add(self.params.sq_off.tail as usize)
        }
    }
    pub fn ring_mask(&self) -> *mut c_void {
        unsafe {
            self.ptr
                .add(self.params.sq_off.ring_mask as usize)
        }
    }
    pub fn ring_entries(&self) -> *mut c_void {
        unsafe {
            self.ptr.add(
                self.params.sq_off.ring_entries as usize,
            )
        }
    }
    pub fn flags(&self) -> u32 {
        let ptr: *mut c_void = unsafe {
            self.ptr.add(self.params.sq_off.flags as usize)
        };
        let casted: *mut u32 = ptr as _;
        unsafe { *casted }
    }
    pub fn dropped(&self) -> *mut c_void {
        unsafe {
            self.ptr
                .add(self.params.sq_off.dropped as usize)
        }
    }
    pub fn array(&self) -> *mut c_void {
        unsafe {
            self.ptr.add(self.params.sq_off.array as usize)
        }
    }
}
