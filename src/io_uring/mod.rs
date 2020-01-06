use std::{
    collections::HashMap,
    convert::TryFrom,
    fs::File,
    io::{self, IoSlice, IoSliceMut},
    os::unix::io::AsRawFd,
    sync::{
        atomic::{
            AtomicU32,
            Ordering::{Acquire, Release},
        },
        Arc,
    },
};

use super::{pair, Completion, CompletionFiller, FastLock};

mod config;
mod constants;
mod kernel_types;
mod syscall;

use kernel_types::{
    io_uring_cqe, io_uring_params, io_uring_sqe,
};

use constants::*;

use syscall::{enter, setup};

pub use config::Config;

/// Nice bindings for the shiny new linux IO system
#[derive(Debug)]
pub struct Uring {
    sq: Sq,
    cq: Arc<FastLock<Cq>>,
    flags: u32,
    ring_fd: i32,
    max_id: u64,
}

pub enum Ordering {
    None,
    Link,
    Drain,
}

/// Sprays uring submissions.
#[derive(Debug)]
pub struct Sq {
    khead: &'static AtomicU32,
    ktail: &'static AtomicU32,
    kring_mask: &'static u32,
    kring_entries: &'static u32,
    kflags: &'static u32,
    kdropped: *const u32,
    array: &'static mut [u32],
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
    khead: &'static AtomicU32,
    ktail: &'static AtomicU32,
    kring_mask: &'static u32,
    kring_entries: &'static u32,
    koverflow: &'static AtomicU32,
    cqes: &'static mut [io_uring_cqe],
    ring_ptr: *const libc::c_void,
    ring_sz: usize,
    pending: HashMap<u64, CompletionFiller<io::Result<()>>>,
}

unsafe impl Send for Cq {}

impl Cq {
    fn reap_ready_cqes(&mut self) -> usize {
        let mut head = self.khead.load(Acquire);
        let tail = self.ktail.load(Acquire);
        let count = tail - head;

        // hack to get around mutable usage in loop
        // limitation as of rust 1.40
        let mut cq_opt = Some(self);

        while head != tail {
            let cq = cq_opt.take().unwrap();
            let index = head & cq.kring_mask;
            let cqe = &cq.cqes[index as usize];
            let id = cqe.user_data;
            let res = cqe.res;
            let completion_filler =
                cq.pending.remove(&id).expect(
                    "expect a queued completion filler",
                );
            let result = if res < 0 {
                Err(io::Error::from_raw_os_error(-1 * res))
            } else {
                Ok(())
            };

            completion_filler.fill(result);

            cq.khead.fetch_add(1, Release);
            cq_opt = Some(cq);
            head += 1;
        }

        count as usize
    }
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

fn reaper(ring_fd: i32, cq_mu: Arc<FastLock<Cq>>) {
    fn block_for_cqe(ring_fd: i32) -> io::Result<()> {
        let flags = IORING_ENTER_GETEVENTS;
        let submit = 0;
        let wait = 1;
        let sigset = std::ptr::null_mut();

        enter(ring_fd, submit, wait, flags, sigset)?;

        Ok(())
    }

    loop {
        if Arc::strong_count(&cq_mu) == 1 {
            // system shutdown
            eprintln!("shutting down io_uring completion marker thread");
            return;
        }
        if let Err(e) = block_for_cqe(ring_fd) {
            panic!("error in cqe reaper: {:?}", e);
        } else {
            let mut cq = cq_mu.spin_lock();
            cq.reap_ready_cqes();
        }
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
        opcode: u8,
        file: &File,
        addr: *mut libc::c_void,
        len: usize,
        off: u64,
        ordering: Ordering,
    ) {
        assert_ne!(
            self.user_data, 0,
            "expected user_data to already be set"
        );
        *self = io_uring_sqe {
            opcode,
            flags: 0,
            ioprio: 0,
            fd: file.as_raw_fd(),
            addr: addr as u64,
            len: u32::try_from(len).unwrap(),
            off,
            ..*self
        };

        self.__bindgen_anon_1.rw_flags = 0;
        self.__bindgen_anon_2.__pad2 = [0; 3];

        self.apply_order(ordering);
    }

    pub fn apply_order(&mut self, ordering: Ordering) {
        match ordering {
            Ordering::None => {}
            Ordering::Link => {
                self.flags = self.flags | IOSQE_IO_LINK
            }
            Ordering::Drain => {
                self.flags =
                    self.flags | IOSQE_IO_DRAIN as u8
            }
        }
    }
}

impl Uring {
    pub fn fsync(
        &mut self,
        file: &File,
    ) -> io::Result<Completion<io::Result<()>>> {
        self.fsync_ordered(file, Ordering::None)
    }

    pub fn fsync_ordered(
        &mut self,
        file: &File,
        ordering: Ordering,
    ) -> io::Result<Completion<io::Result<()>>> {
        let (completion, sqe) = self.get_sqe()?;
        sqe.prep_rw(
            IORING_OP_FSYNC,
            file,
            std::ptr::null_mut(),
            0,
            0,
            ordering,
        );
        Ok(completion)
    }

    pub fn fdatasync(
        &mut self,
        file: &File,
    ) -> io::Result<Completion<io::Result<()>>> {
        self.fdatasync_ordered(file, Ordering::None)
    }

    pub fn fdatasync_ordered(
        &mut self,
        file: &File,
        ordering: Ordering,
    ) -> io::Result<Completion<io::Result<()>>> {
        let (completion, sqe) = self.get_sqe()?;
        sqe.prep_rw(
            IORING_OP_FSYNC,
            file,
            std::ptr::null_mut(),
            0,
            0,
            ordering,
        );
        sqe.flags = sqe.flags | IORING_FSYNC_DATASYNC;
        Ok(completion)
    }

    pub fn write(
        &mut self,
        file: &File,
        iov: &IoSlice,
        at: u64,
    ) -> io::Result<Completion<io::Result<()>>> {
        self.write_ordered(file, iov, at, Ordering::None)
    }

    pub fn write_ordered(
        &mut self,
        file: &File,
        iov: &IoSlice,
        at: u64,
        ordering: Ordering,
    ) -> io::Result<Completion<io::Result<()>>> {
        let (completion, sqe) = self.get_sqe()?;
        sqe.prep_rw(
            IORING_OP_WRITEV,
            file,
            iov as *const _ as _,
            1,
            at,
            ordering,
        );
        Ok(completion)
    }

    pub fn read(
        &mut self,
        file: &File,
        iov: &mut IoSliceMut,
        at: u64,
    ) -> io::Result<Completion<io::Result<()>>> {
        self.read_ordered(file, iov, at, Ordering::None)
    }

    pub fn read_ordered(
        &mut self,
        file: &File,
        iov: &mut IoSliceMut,
        at: u64,
        ordering: Ordering,
    ) -> io::Result<Completion<io::Result<()>>> {
        let (completion, sqe) = self.get_sqe()?;
        sqe.prep_rw(
            IORING_OP_READV,
            file,
            iov as *mut _ as _,
            1,
            at,
            ordering,
        );
        Ok(completion)
    }

    fn get_sqe(
        &mut self,
    ) -> io::Result<(
        Completion<io::Result<()>>,
        &mut io_uring_sqe,
    )> {
        loop {
            let next = self.sq.sqe_tail + 1;

            if (self.flags & IORING_SETUP_SQPOLL) == 0 {
                // non-polling mode
                let head = self.sq.sqe_head;
                if next - head <= *self.sq.kring_entries {
                    let idx = self.sq.sqe_tail
                        & self.sq.kring_mask;
                    let sqe =
                        &mut self.sq.sqes[idx as usize];
                    self.sq.sqe_tail = next;
                    self.max_id += 1;
                    let id = self.max_id;
                    sqe.user_data = id;

                    let (completion, filler) =
                        pair(self.cq.clone());

                    let mut cq = self.cq.spin_lock();
                    assert!(cq
                        .pending
                        .insert(sqe.user_data, filler)
                        .is_none());

                    return Ok((completion, sqe));
                } else {
                    self.submit_all()?;
                    self.reap_ready_cqes();
                }
            } else {
                // polling mode
                todo!()
            }
        }
    }

    fn flush(&mut self) -> u32 {
        let mask: u32 = *self.sq.kring_mask;
        if self.sq.sqe_head == self.sq.sqe_tail {
            return 0;
        }

        let mut ktail = self.sq.ktail.load(Acquire);
        let to_submit = self.sq.sqe_tail - self.sq.sqe_head;
        for _ in 0..to_submit {
            let index = ktail & mask;
            self.sq.array[index as usize] =
                self.sq.sqe_head & mask;
            ktail += 1;
            self.sq.sqe_head += 1;
        }

        let swapped = self.sq.ktail.swap(ktail, Release);

        assert_eq!(swapped, ktail - to_submit);

        to_submit
    }

    pub fn submit_all(&mut self) -> io::Result<()> {
        if self.flags & IORING_SETUP_SQPOLL != 0 {
            // skip submission if we don't need to do it
            if self.sq.kflags & IORING_SQ_NEED_WAKEUP != 0 {
                let to_submit =
                    self.sq.sqe_tail - self.sq.sqe_head;
                enter(
                    self.ring_fd,
                    to_submit,
                    0,
                    IORING_ENTER_SQ_WAKEUP,
                    std::ptr::null_mut(),
                )?;
            }
        } else {
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
        }
        Ok(())
    }

    fn reap_ready_cqes(&mut self) -> usize {
        if let Some(mut cq) = self.cq.try_lock() {
            cq.reap_ready_cqes()
        } else {
            0
        }
    }
}
