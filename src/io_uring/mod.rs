use std::{
    collections::HashMap,
    convert::TryFrom,
    fs::File,
    io::{self, IoSlice, IoSliceMut},
    os::unix::io::AsRawFd,
    slice::from_raw_parts_mut,
    sync::{
        atomic::{
            AtomicU32,
            Ordering::{Acquire, Release},
        },
        Arc,
    },
};

use super::{
    fastlock::FastLock,
    promise::{pair, Promise, PromiseFiller},
};

mod constants;
mod kernel_types;
mod syscall;

use kernel_types::{
    io_uring_cqe, io_uring_params, io_uring_sqe,
};

use constants::*;

use syscall::{enter, setup};

/// Nice bindings for the shiny new linux IO system
#[derive(Debug)]
pub struct Uring {
    sq: Sq,
    cq: Arc<FastLock<Cq>>,
    flags: u32,
    ring_fd: i32,
    max_id: u64,
}

pub enum ReaperStyle {
    Default,
    IoPoll,
    SqPoll,
    PinnedSqPoll(u32),
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
    khead: &'static AtomicU32,
    ktail: &'static AtomicU32,
    kring_mask: u32,
    kring_entries: u32,
    koverflow: &'static AtomicU32,
    cqes: &'static mut [io_uring_cqe],
    ring_ptr: *const libc::c_void,
    ring_sz: usize,
    pending: HashMap<u64, PromiseFiller<io::Result<()>>>,
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
            let promise_filler = cq
                .pending
                .remove(&id)
                .expect("expect a queued promise filler");
            let result = if res < 0 {
                Err(io::Error::from_raw_os_error(-1 * res))
            } else {
                Ok(())
            };

            promise_filler.fill(result);

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

fn completion_marker(
    ring_fd: i32,
    cq_mu: Arc<FastLock<Cq>>,
) {
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
            return;
        }
        if let Err(e) = block_for_cqe(ring_fd) {
            eprintln!("error in cqe reaper: {:?}", e);
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
        at: u64,
    ) {
        *self = io_uring_sqe {
            opcode,
            flags: 0,
            ioprio: 0,
            fd: file.as_raw_fd(),
            addr: addr as u64,
            len: u32::try_from(len).unwrap(),
            off: u64::try_from(at).unwrap(),
            ..*self
        };
        self.__bindgen_anon_1.rw_flags = 0;
        self.__bindgen_anon_2.__pad2 = [0; 3];
    }

    pub fn drain_everything_first(&mut self) {
        self.flags = self.flags | IOSQE_IO_DRAIN as u8
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
                koverflow: &*(cq_ring_ptr
                    .add(params.cq_off.overflow as usize)
                    as *const AtomicU32),
                cqes: from_raw_parts_mut(
                    cq_ring_ptr
                        .add(params.cq_off.cqes as usize)
                        as _,
                    params.cq_off.cqes as usize,
                ),
                pending: HashMap::new(),
            }
        };

        let cq_arc = Arc::new(FastLock::new(cq));
        let completion_cq_arc = cq_arc.clone();

        std::thread::spawn(move || {
            completion_marker(ring_fd, completion_cq_arc)
        });

        Ok(Uring {
            flags: params.flags,
            ring_fd,
            sq,
            cq: cq_arc,
            max_id: 0,
        })
    }

    /// Sync the file. This does not work with O_DIRECT.
    /// Sets a flag to guarantee that all previously
    /// submitted operations happen first.
    ///
    /// # Examples
    ///
    /// ```
    /// ring.enqueue_fsync(&file);
    /// ring.submit_all().expect("submit");
    /// let cqe = ring.wait_cqe().unwrap();
    /// cqe.seen();
    /// ```
    pub fn fsync(
        &mut self,
        file: &File,
    ) -> io::Result<Promise<io::Result<()>>> {
        let (promise, sqe) = self.get_sqe()?;
        sqe.prep_rw(
            IORING_OP_FSYNC,
            file,
            std::ptr::null_mut(),
            0,
            0,
        );
        sqe.drain_everything_first();
        Ok(promise)
    }

    pub fn write(
        &mut self,
        file: &File,
        iov: &IoSlice,
        at: u64,
    ) -> io::Result<Promise<io::Result<()>>> {
        let (promise, sqe) = self.get_sqe()?;
        sqe.prep_rw(
            IORING_OP_WRITEV,
            file,
            iov as *const _ as _,
            1,
            at,
        );
        Ok(promise)
    }

    pub fn read(
        &mut self,
        file: &File,
        iov: &mut IoSliceMut,
        at: u64,
    ) -> io::Result<Promise<io::Result<()>>> {
        let (promise, sqe) = self.get_sqe()?;
        sqe.prep_rw(
            IORING_OP_READV,
            file,
            iov as *mut _ as _,
            1,
            at,
        );
        Ok(promise)
    }

    pub(crate) fn get_sqe(
        &mut self,
    ) -> io::Result<(
        Promise<io::Result<()>>,
        &mut io_uring_sqe,
    )> {
        loop {
            let next = self.sq.sqe_tail + 1;

            if (self.flags & IORING_SETUP_SQPOLL) == 0 {
                // non-polling mode
                let head = self.sq.sqe_head;
                if next - head <= self.sq.kring_entries {
                    let idx = self.sq.sqe_tail
                        & self.sq.kring_mask;
                    let sqe =
                        &mut self.sq.sqes[idx as usize];
                    self.sq.sqe_tail = next;
                    self.max_id += 1;
                    let id = self.max_id;
                    sqe.user_data = id;

                    let (promise, filler) =
                        pair(self.cq.clone());

                    let mut cq = self.cq.spin_lock();
                    assert!(cq
                        .pending
                        .insert(sqe.user_data, filler)
                        .is_none());

                    return Ok((promise, sqe));
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

    fn reap_ready_cqes(&mut self) -> usize {
        let mut cq = if let Some(cq) = self.cq.try_lock() {
            cq
        } else {
            return 0;
        };

        cq.reap_ready_cqes()
    }
}
