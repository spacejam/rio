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
    ticket_queue: Arc<TicketQueue>,
    in_flight: Arc<InFlight>,
    flags: u32,
    ring_fd: i32,
    config: Config,
}

struct InFlight {
    iovecs: UnsafeCell<Vec<libc::iovec>>,
    fillers: UnsafeCell<
        Vec<Option<Filler<io::Result<io_uring_cqe>>>>,
    >,
}

impl std::fmt::Debug for InFlight {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        write!(f, "InFlight {{ .. }}")
    }
}

impl InFlight {
    fn new(size: usize) -> InFlight {
        let iovecs = UnsafeCell::new(vec![
            libc::iovec {
                iov_base: std::ptr::null_mut(),
                iov_len: 0
            };
            size
        ]);
        let mut filler_vec = Vec::with_capacity(size);
        for _ in 0..size {
            filler_vec.push(None);
        }
        let fillers = UnsafeCell::new(filler_vec);
        InFlight { iovecs, fillers }
    }

    fn insert(
        &self,
        ticket: usize,
        iovec: Option<libc::iovec>,
        filler: Filler<io::Result<io_uring_cqe>>,
    ) -> *mut libc::iovec {
        #[allow(unsafe_code)]
        unsafe {
            let iovec_ptr = self.iovecs.get();
            if let Some(iovec) = iovec {
                (*iovec_ptr)[ticket] = iovec;
            }
            (*self.fillers.get())[ticket] = Some(filler);
            if iovec.is_some() {
                (*iovec_ptr).as_mut_ptr().add(ticket)
            } else {
                std::ptr::null_mut()
            }
        }
    }

    fn take_filler(
        &self,
        ticket: usize,
    ) -> Filler<io::Result<io_uring_cqe>> {
        #[allow(unsafe_code)]
        unsafe {
            (*self.fillers.get())[ticket].take().unwrap()
        }
    }
}

#[derive(Debug)]
struct TicketQueue {
    tickets: Mutex<Vec<usize>>,
    cv: Condvar,
}

impl TicketQueue {
    fn new(size: usize) -> TicketQueue {
        let tickets = Mutex::new((0..size).collect());
        TicketQueue {
            tickets,
            cv: Condvar::new(),
        }
    }

    fn push_multi(&self, mut new_tickets: Vec<usize>) {
        let _ = Measure::new(&M.ticket_queue_push);
        let mut tickets = self.tickets.lock().unwrap();
        tickets.append(&mut new_tickets);
        self.cv.notify_one();
    }

    fn pop(&self) -> usize {
        let _ = Measure::new(&M.ticket_queue_pop);
        let mut tickets = self.tickets.lock().unwrap();
        while tickets.is_empty() {
            tickets = self.cv.wait(tickets).unwrap();
        }
        tickets.pop().unwrap()
    }
}

#[allow(unsafe_code)]
unsafe impl Send for Uring {}

#[allow(unsafe_code)]
unsafe impl Sync for Uring {}

impl Drop for Uring {
    fn drop(&mut self) {
        let poison_pill_res = self.with_sqe(None, |sqe| {
            sqe.prep_rw(
                IORING_OP_NOP,
                0,
                1,
                0,
                Ordering::Drain,
            );
            // set the poison pill
            sqe.user_data ^= u64::max_value();
        });

        if let Err(e) = poison_pill_res {
            eprintln!(
                "failed to flush poison pill to the ring: {:?}",
                e
            );
        }

        if let Err(e) = self.submit_all() {
            eprintln!(
                "failed to submit pending items: {:?}",
                e
            );
        }

        if self.config.print_profile_on_drop {
            M.print_profile();
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
        if ring_flags & IORING_SETUP_SQPOLL == 0 {
            // non-SQPOLL mode, we need to use
            // `enter` to submit our SQEs.

            // TODO for polling, keep flags at 0

            let flags = IORING_ENTER_GETEVENTS;
            let mut submitted = self.flush();
            while submitted > 0 {
                let _ = Measure::new(&M.enter_sqe);
                let ret = enter(
                    ring_fd,
                    submitted,
                    0,
                    flags,
                    std::ptr::null_mut(),
                )?;
                submitted -= u32::try_from(ret).unwrap();
            }
        } else if self.kflags & IORING_SQ_NEED_WAKEUP != 0 {
            let to_submit = self.sqe_tail - self.sqe_head;
            let _ = Measure::new(&M.enter_sqe);
            enter(
                ring_fd,
                to_submit,
                0,
                IORING_ENTER_SQ_WAKEUP,
                std::ptr::null_mut(),
            )?;
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
    ticket_queue: Arc<TicketQueue>,
    in_flight: Arc<InFlight>,
}

#[allow(unsafe_code)]
unsafe impl Send for Cq {}

impl Cq {
    fn reaper(&mut self, ring_fd: i32) {
        fn block_for_cqe(ring_fd: i32) -> io::Result<()> {
            let flags = IORING_ENTER_GETEVENTS;
            let submit = 0;
            let wait = 1;
            let sigset = std::ptr::null_mut();

            let _ = Measure::new(&M.enter_cqe);
            enter(ring_fd, submit, wait, flags, sigset)?;

            Ok(())
        }

        loop {
            if let Err(e) = block_for_cqe(ring_fd) {
                panic!("error in cqe reaper: {:?}", e);
            } else {
                assert_eq!(self.koverflow.load(Relaxed), 0);
                if self.reap_ready_cqes().is_none() {
                    // poison pill detected, time to shut down
                    return;
                }
            }
        }
    }

    fn reap_ready_cqes(&mut self) -> Option<usize> {
        let _ = Measure::new(&M.reap_ready);
        let mut head = self.khead.load(Acquire);
        let tail = self.ktail.load(Acquire);
        let count = tail - head;

        // hack to get around mutable usage in loop
        // limitation as of rust 1.40
        let mut cq_opt = Some(self);

        let mut to_push =
            Vec::with_capacity(count as usize);

        while head != tail {
            let cq = cq_opt.take().unwrap();
            let index = head & cq.kring_mask;
            let cqe = &cq.cqes[index as usize];

            // we detect a poison pill by seeing if
            // the user_data is really big, which it
            // will tend not to be. if it's not a
            // poison pill, it will be up to as large
            // as the completion queue length.
            let (ticket, poisoned) =
                if cqe.user_data > u64::max_value() / 2 {
                    (cqe.user_data ^ u64::max_value(), true)
                } else {
                    (cqe.user_data, false)
                };

            let res = cqe.res;

            let completion_filler =
                cq.in_flight.take_filler(ticket as usize);
            to_push.push(ticket as usize);

            let result = if res < 0 {
                Err(io::Error::from_raw_os_error(res.neg()))
            } else {
                Ok(*cqe)
            };

            completion_filler.fill(result);

            cq.khead.fetch_add(1, Release);
            cq_opt = Some(cq);
            head += 1;

            if poisoned {
                return None;
            }
        }

        cq_opt
            .take()
            .unwrap()
            .ticket_queue
            .push_multi(to_push);

        Some(count as usize)
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

impl Uring {
    /// Block until all items in the submission queue
    /// are submitted to the kernel. This can
    /// be avoided by using the `SQPOLL` mode
    /// (a privileged operation) on the `Config`
    /// struct.
    pub fn submit_all(&self) -> io::Result<()> {
        let mut sq = {
            let _get_sq_mu = Measure::new(&M.sq_mu_wait);
            self.sq.lock().unwrap()
        };
        let _hold_sq_mu = Measure::new(&M.sq_mu_hold);
        sq.submit_all(self.flags, self.ring_fd)
    }

    /// Flushes all buffered writes, and associated
    /// metadata changes.
    ///
    /// # Warning
    ///
    /// You usually don't want to do this without
    /// linking to a previous write, because
    /// `io_uring` will execute operations out-of-order.
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
        self.with_sqe(None, |sqe| {
            sqe.prep_rw(
                IORING_OP_FSYNC,
                file.as_raw_fd(),
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
        self.with_sqe(None, |mut sqe| {
            sqe.prep_rw(
                IORING_OP_FSYNC,
                file.as_raw_fd(),
                0,
                0,
                ordering,
            );
            sqe.flags |= IORING_FSYNC_DATASYNC;
        })
    }

    /// Writes data at the provided buffer using
    /// vectored IO. Be sure to check the returned
    /// `io_uring_cqe`'s `res` field to see if a
    /// short write happened. This will contain
    /// the number of bytes written.
    ///
    /// Note that the file argument is generic
    /// for anything that supports AsRawFd:
    /// sockets, files, etc...
    pub fn write_at<'uring, 'file, 'buf, F, B>(
        &'uring self,
        file: &'file F,
        iov: &'buf B,
        at: u64,
    ) -> io::Result<
        Completion<'buf, io::Result<io_uring_cqe>>,
    >
    where
        'file: 'uring + 'buf,
        'buf: 'uring + 'file,
        'uring: 'buf + 'file,
        F: AsRawFd,
        B: AsIoVec,
    {
        self.write_at_ordered(file, iov, at, Ordering::None)
    }

    /// Writes data at the provided buffer using
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
    ///
    /// Note that the file argument is generic
    /// for anything that supports AsRawFd:
    /// sockets, files, etc...
    pub fn write_at_ordered<'uring, 'file, 'buf, F, B>(
        &'uring self,
        file: &'file F,
        iov: &'buf B,
        at: u64,
        ordering: Ordering,
    ) -> io::Result<
        Completion<'buf, io::Result<io_uring_cqe>>,
    >
    where
        'file: 'uring + 'buf,
        'buf: 'uring + 'file,
        'uring: 'buf + 'file,
        F: AsRawFd,
        B: AsIoVec,
    {
        self.with_sqe(Some(iov.into_new_iovec()), |sqe| {
            sqe.prep_rw(
                IORING_OP_WRITEV,
                file.as_raw_fd(),
                1,
                at,
                ordering,
            )
        })
    }

    /// Reads data into the provided buffer using
    /// vectored IO. Be sure to check the returned
    /// `io_uring_cqe`'s `res` field to see if a
    /// short read happened. This will contain
    /// the number of bytes read.
    ///
    /// Note that the file argument is generic
    /// for anything that supports AsRawFd:
    /// sockets, files, etc...
    pub fn read_at<'uring, 'file, 'buf, F, B>(
        &'uring self,
        file: &'file F,
        iov: &'buf B,
        at: u64,
    ) -> io::Result<
        Completion<'buf, io::Result<io_uring_cqe>>,
    >
    where
        'file: 'uring + 'buf,
        'buf: 'uring + 'file,
        'uring: 'buf + 'file,
        F: AsRawFd,
        B: AsIoVec,
    {
        self.read_at_ordered(file, iov, at, Ordering::None)
    }

    /// Reads data into the provided buffer using
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
    ///
    /// Note that the file argument is generic
    /// for anything that supports AsRawFd:
    /// sockets, files, etc...
    pub fn read_at_ordered<'uring, 'file, 'buf, F, B>(
        &'uring self,
        file: &'file F,
        iov: &'buf B,
        at: u64,
        ordering: Ordering,
    ) -> io::Result<
        Completion<'buf, io::Result<io_uring_cqe>>,
    >
    where
        'file: 'uring + 'buf,
        'buf: 'uring + 'file,
        'uring: 'buf + 'file,
        F: AsRawFd,
        B: AsIoVec,
    {
        self.with_sqe(Some(iov.into_new_iovec()), |sqe| {
            sqe.prep_rw(
                IORING_OP_READV,
                file.as_raw_fd(),
                1,
                at,
                ordering,
            )
        })
    }

    /// Don't do anything. This is
    /// mostly for debugging and tuning.
    pub fn nop<'uring>(
        &'uring self,
    ) -> io::Result<
        Completion<'uring, io::Result<io_uring_cqe>>,
    > {
        self.nop_ordered(Ordering::None)
    }

    /// Don't do anything. This is
    /// mostly for debugging and tuning.
    pub fn nop_ordered<'uring>(
        &'uring self,
        ordering: Ordering,
    ) -> io::Result<
        Completion<'uring, io::Result<io_uring_cqe>>,
    > {
        self.with_sqe(None, |sqe| {
            sqe.prep_rw(IORING_OP_NOP, 0, 1, 0, ordering)
        })
    }

    fn with_sqe<'uring, 'buf, F>(
        &'uring self,
        iovec: Option<libc::iovec>,
        f: F,
    ) -> io::Result<
        Completion<'buf, io::Result<io_uring_cqe>>,
    >
    where
        'buf: 'uring,
        'uring: 'buf,
        F: FnOnce(&mut io_uring_sqe),
    {
        let ticket = self.ticket_queue.pop();
        let (completion, filler) = pair();

        let iovec_ptr =
            self.in_flight.insert(ticket, iovec, filler);

        let mut sq = {
            let _get_sq_mu = Measure::new(&M.sq_mu_wait);
            self.sq.lock().unwrap()
        };
        let _hold_sq_mu = Measure::new(&M.sq_mu_hold);

        let sqe = {
            let _get_sqe = Measure::new(&M.get_sqe);
            loop {
                if let Some(sqe) =
                    sq.try_get_sqe(self.flags)
                {
                    break sqe;
                } else {
                    sq.submit_all(
                        self.flags,
                        self.ring_fd,
                    )?;
                };
            }
        };

        sqe.user_data = ticket as u64;
        sqe.addr = iovec_ptr as u64;
        f(sqe);

        Ok(completion)
    }
}
