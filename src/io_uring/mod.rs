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
        Arc, Mutex,
    },
};

use super::{pair, Completion, CompletionFiller};

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
    sq: Mutex<Sq>,
    cq: Arc<Mutex<Cq>>,
    flags: u32,
    ring_fd: i32,
}

impl Drop for Uring {
    fn drop(&mut self) {
        if let Err(e) = self.submit_all() {
            eprintln!(
                "failed to submit pending items: {:?}",
                e
            );
        }
    }
}

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
    max_id: u64,
}

impl Sq {
    fn try_get_sqe(
        &mut self,
        ring_flags: u32,
    ) -> Option<&mut io_uring_sqe> {
        let next = self.sqe_tail + 1;

        let head =
            if (ring_flags & IORING_SETUP_SQPOLL) == 0 {
                // non-polling mode
                self.sqe_head
            } else {
                // polling mode
                self.khead.load(Acquire)
            };

        if next - head <= *self.kring_entries {
            let idx = self.sqe_tail & self.kring_mask;
            let sqe = &mut self.sqes[idx as usize];
            self.sqe_tail = next;
            self.max_id += 1;
            let id = self.max_id;
            sqe.user_data = id;

            Some(sqe)
        } else {
            None
        }
    }

    // sets sq.array to point to current sq.sqe_head
    fn flush(&mut self) -> u32 {
        let mask: u32 = *self.kring_mask;
        if self.sqe_head == self.sqe_tail {
            return 0;
        }

        let mut ktail = self.ktail.load(Acquire);
        let to_submit = self.sqe_tail - self.sqe_head;
        for _ in 0..to_submit {
            let index = ktail & mask;
            self.array[index as usize] =
                self.sqe_head & mask;
            ktail += 1;
            self.sqe_head += 1;
        }

        let swapped = self.ktail.swap(ktail, Release);

        assert_eq!(swapped, ktail - to_submit);

        to_submit
    }

    fn submit_all(
        &mut self,
        ring_flags: u32,
        ring_fd: i32,
    ) -> io::Result<()> {
        if ring_flags & IORING_SETUP_SQPOLL != 0 {
            // skip submission if we don't need to do it
            if self.kflags & IORING_SQ_NEED_WAKEUP != 0 {
                let to_submit =
                    self.sqe_tail - self.sqe_head;
                enter(
                    ring_fd,
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
                    ring_fd,
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
}

impl Drop for Sq {
    #[allow(unsafe_code)]
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
    pending: HashMap<
        u64,
        CompletionFiller<io::Result<io_uring_cqe>>,
    >,
}

#[allow(unsafe_code)]
unsafe impl Send for Cq {}

impl Cq {
    pub(crate) fn reap_ready_cqes(&mut self) -> usize {
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
                Ok(*cqe)
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
        #[allow(unsafe_code)]
        unsafe {
            libc::munmap(
                self.ring_ptr as *mut libc::c_void,
                self.ring_sz,
            );
        }
    }
}

fn reaper(ring_fd: i32, cq_mu: Arc<Mutex<Cq>>) {
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
            let mut cq = cq_mu.lock().unwrap();
            cq.reap_ready_cqes();
        }
    }
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

    pub(crate) fn apply_order(
        &mut self,
        ordering: Ordering,
    ) {
        match ordering {
            Ordering::None => {}
            Ordering::Link => self.flags |= IOSQE_IO_LINK,
            Ordering::Drain => self.flags |= IOSQE_IO_DRAIN,
        }
    }
}

impl Uring {
    /// Block until all items in the submission queue
    /// are submitted to the kernel. This can
    /// be avoided by using the `SQPOLL` mode
    /// (a privileged operation) on the `Config`
    /// struct.
    pub fn submit_all(&self) -> io::Result<()> {
        let mut sq = self.sq.lock().unwrap();
        sq.submit_all(self.flags, self.ring_fd)
    }

    /// Flushes all buffered writes, and associated
    /// metadata changes.
    ///
    /// # Warning
    ///
    /// You usually don't want to do this without
    /// linking to a previous write, because
    /// io_uring will execute operations out-of-order.
    /// Without setting a `Link` ordering on the previous
    /// operation, or using `fsync_ordered` with
    /// the `Drain` ordering, causing all previous
    /// operations to complete before itself.
    ///
    /// Additionally, fsync does not ensure that
    /// the file actually exists in its parent
    /// directory. So, for new files, you must
    /// also fsync the parent directory.
    ///
    /// This does nothing for files opened in
    /// `O_DIRECT` mode.
    pub fn fsync<'uring, 'file>(
        &'uring self,
        file: &'file File,
    ) -> io::Result<
        Completion<'file, io::Result<io_uring_cqe>>,
    >
    where
        'file: 'uring,
        'uring: 'file,
    {
        self.fsync_ordered(file, Ordering::None)
    }

    /// Flushes all buffered writes, and associated
    /// metadata changes.
    ///
    /// You probably want to
    /// either use a `Link` ordering on a previous
    /// write (or chain of separate writes), or
    /// use the `Drain` ordering on this operation.
    ///
    /// You may pass in an `Ordering` to specify
    /// two different optional behaviors:
    ///
    /// * `Ordering::Link` causes the next
    ///   submitted operation to wait until
    ///   this one finishes. Useful for
    ///   things like file copy, fsync-after-write,
    ///   or proxies.
    /// * `Ordering::Drain` causes all previously
    ///   submitted operations to complete before
    ///   this one begins.
    ///
    /// # Warning
    ///
    /// fsync does not ensure that
    /// the file actually exists in its parent
    /// directory. So, for new files, you must
    /// also fsync the parent directory.
    /// This does nothing for files opened in
    /// `O_DIRECT` mode.
    pub fn fsync_ordered<'uring, 'file>(
        &'uring self,
        file: &'file File,
        ordering: Ordering,
    ) -> io::Result<
        Completion<'file, io::Result<io_uring_cqe>>,
    >
    where
        'file: 'uring,
        'uring: 'file,
    {
        self.with_sqe(|sqe| {
            sqe.prep_rw(
                IORING_OP_FSYNC,
                file,
                std::ptr::null_mut(),
                0,
                0,
                ordering,
            )
        })
    }

    /// Flushes all buffered writes, and the specific
    /// metadata required to access the data. This
    /// will skip syncing metadata like atime.
    ///
    /// You probably want to
    /// either use a `Link` ordering on a previous
    /// write (or chain of separate writes), or
    /// use the `Drain` ordering on this operation
    /// with the `fdatasync_ordered` method.
    ///
    /// # Warning
    ///
    /// fdatasync does not ensure that
    /// the file actually exists in its parent
    /// directory. So, for new files, you must
    /// also fsync the parent directory.
    /// This does nothing for files opened in
    /// `O_DIRECT` mode.
    pub fn fdatasync<'uring, 'file>(
        &'uring self,
        file: &'file File,
    ) -> io::Result<
        Completion<'file, io::Result<io_uring_cqe>>,
    >
    where
        'file: 'uring,
        'uring: 'file,
    {
        self.fdatasync_ordered(file, Ordering::None)
    }

    /// Flushes all buffered writes, and the specific
    /// metadata required to access the data. This
    /// will skip syncing metadata like atime.
    ///
    /// You probably want to
    /// either use a `Link` ordering on a previous
    /// write (or chain of separate writes), or
    /// use the `Drain` ordering on this operation.
    ///
    /// You may pass in an `Ordering` to specify
    /// two different optional behaviors:
    ///
    /// * `Ordering::Link` causes the next
    ///   submitted operation to wait until
    ///   this one finishes. Useful for
    ///   things like file copy, fsync-after-write,
    ///   or proxies.
    /// * `Ordering::Drain` causes all previously
    ///   submitted operations to complete before
    ///   this one begins.
    ///
    /// # Warning
    ///
    /// fdatasync does not ensure that
    /// the file actually exists in its parent
    /// directory. So, for new files, you must
    /// also fsync the parent directory.
    /// This does nothing for files opened in
    /// `O_DIRECT` mode.
    pub fn fdatasync_ordered<'uring, 'file>(
        &'uring self,
        file: &'file File,
        ordering: Ordering,
    ) -> io::Result<
        Completion<'file, io::Result<io_uring_cqe>>,
    >
    where
        'file: 'uring,
        'uring: 'file,
    {
        self.with_sqe(|mut sqe| {
            sqe.prep_rw(
                IORING_OP_FSYNC,
                file,
                std::ptr::null_mut(),
                0,
                0,
                ordering,
            );
            sqe.flags |= IORING_FSYNC_DATASYNC;
        })
    }

    /// Writes data at the provided `IoSlice` using
    /// vectored IO. Be sure to check the returned
    /// `io_uring_cqe`'s `res` field to see if a
    /// short write happened. This will contain
    /// the number of bytes written.
    pub fn write_at<'uring, 'file, 'buf>(
        &'uring self,
        file: &'file File,
        iov: &'buf IoSlice<'buf>,
        at: u64,
    ) -> io::Result<
        Completion<'buf, io::Result<io_uring_cqe>>,
    >
    where
        'file: 'uring + 'buf,
        'buf: 'uring + 'file,
        'uring: 'buf + 'file,
    {
        self.write_ordered(file, iov, at, Ordering::None)
    }

    /// Writes data at the provided `IoSlice` using
    /// vectored IO.
    ///
    /// Be sure to check the returned
    /// `io_uring_cqe`'s `res` field to see if a
    /// short write happened. This will contain
    /// the number of bytes written.
    ///
    /// You may pass in an `Ordering` to specify
    /// two different optional behaviors:
    ///
    /// * `Ordering::Link` causes the next
    ///   submitted operation to wait until
    ///   this one finishes. Useful for
    ///   things like file copy, fsync-after-write,
    ///   or proxies.
    /// * `Ordering::Drain` causes all previously
    ///   submitted operations to complete before
    ///   this one begins.
    pub fn write_ordered<'uring, 'file, 'buf>(
        &'uring self,
        file: &'file File,
        iov: &'buf IoSlice<'buf>,
        at: u64,
        ordering: Ordering,
    ) -> io::Result<
        Completion<'buf, io::Result<io_uring_cqe>>,
    >
    where
        'file: 'uring + 'buf,
        'buf: 'uring + 'file,
        'uring: 'buf + 'file,
    {
        let iov_ptr: *const IoSlice<'buf> = iov;
        self.with_sqe(|sqe| {
            sqe.prep_rw(
                IORING_OP_WRITEV,
                file,
                iov_ptr as _,
                1,
                at,
                ordering,
            )
        })
    }

    /// Reads data into the provided `IoSliceMut` using
    /// vectored IO. Be sure to check the returned
    /// `io_uring_cqe`'s `res` field to see if a
    /// short read happened. This will contain
    /// the number of bytes read.
    pub fn read_at<'uring, 'file, 'buf>(
        &'uring self,
        file: &'file File,
        iov: &'buf mut IoSliceMut<'buf>,
        at: u64,
    ) -> io::Result<
        Completion<'buf, io::Result<io_uring_cqe>>,
    >
    where
        'file: 'uring + 'buf,
        'buf: 'uring + 'file,
        'uring: 'buf + 'file,
    {
        self.read_ordered(file, iov, at, Ordering::None)
    }

    /// Reads data into the provided `IoSliceMut` using
    /// vectored IO. Be sure to check the returned
    /// `io_uring_cqe`'s `res` field to see if a
    /// short read happened. This will contain
    /// the number of bytes read.
    ///
    /// You may pass in an `Ordering` to specify
    /// two different optional behaviors:
    ///
    /// * `Ordering::Link` causes the next
    ///   submitted operation to wait until
    ///   this one finishes. Useful for
    ///   things like file copy, fsync-after-write,
    ///   or proxies.
    /// * `Ordering::Drain` causes all previously
    ///   submitted operations to complete before
    ///   this one begins.
    pub fn read_ordered<'uring, 'file, 'buf>(
        &'uring self,
        file: &'file File,
        iov: &'buf mut IoSliceMut<'buf>,
        at: u64,
        ordering: Ordering,
    ) -> io::Result<
        Completion<'buf, io::Result<io_uring_cqe>>,
    >
    where
        'file: 'uring + 'buf,
        'buf: 'uring + 'file,
        'uring: 'buf + 'file,
    {
        let iov_ptr: *const IoSliceMut<'buf> = iov;
        self.with_sqe(|sqe| {
            sqe.prep_rw(
                IORING_OP_READV,
                file,
                iov_ptr as _,
                1,
                at,
                ordering,
            )
        })
    }

    fn with_sqe<'uring, 'buf, F>(
        &'uring self,
        f: F,
    ) -> io::Result<
        Completion<'buf, io::Result<io_uring_cqe>>,
    >
    where
        'buf: 'uring,
        'uring: 'buf,
        F: FnOnce(&mut io_uring_sqe),
    {
        let (completion, filler) = pair(self.cq.clone());

        let mut sq = self.sq.lock().unwrap();
        let sqe = loop {
            if let Some(sqe) = sq.try_get_sqe(self.flags) {
                break sqe;
            } else {
                drop(sq);
                self.submit_all()?;
                self.reap_ready_cqes();
                sq = self.sq.lock().unwrap();
            };
        };

        f(sqe);

        let mut cq = self.cq.lock().unwrap();
        assert!(cq
            .pending
            .insert(sqe.user_data, filler)
            .is_none());

        Ok(completion)
    }

    fn reap_ready_cqes(&self) -> usize {
        if let Ok(mut cq) = self.cq.try_lock() {
            cq.reap_ready_cqes()
        } else {
            0
        }
    }
}
