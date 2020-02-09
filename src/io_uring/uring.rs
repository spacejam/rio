use super::*;

/// Nice bindings for the shiny new linux IO system
#[derive(Debug, Clone)]
pub struct Rio(pub(crate) Arc<Uring>);

impl std::ops::Deref for Rio {
    type Target = Uring;

    fn deref(&self) -> &Uring {
        &self.0
    }
}

/// The top-level `io_uring` structure.
#[derive(Debug)]
pub struct Uring {
    sq: Mutex<Sq>,
    ticket_queue: Arc<TicketQueue>,
    in_flight: Arc<InFlight>,
    flags: u32,
    ring_fd: i32,
    config: Config,
    loaded: AtomicU64,
    submitted: AtomicU64,
}

#[allow(unsafe_code)]
unsafe impl Send for Uring {}

#[allow(unsafe_code)]
unsafe impl Sync for Uring {}

impl Drop for Uring {
    fn drop(&mut self) {
        let poison_pill_res = self.with_sqe::<_, ()>(None, false, |sqe| {
            sqe.prep_rw(IORING_OP_NOP, 0, 1, 0, Ordering::Drain);
            // set the poison pill
            sqe.user_data ^= u64::max_value();
        });

        // this waits for the NOP event to complete.
        drop(poison_pill_res);

        if self.config.print_profile_on_drop {
            #[cfg(not(feature = "no_metrics"))]
            M.print_profile();
        }
    }
}

impl Uring {
    pub(crate) fn new(
        config: Config,
        flags: u32,
        ring_fd: i32,
        sq: Sq,
        in_flight: Arc<InFlight>,
        ticket_queue: Arc<TicketQueue>,
    ) -> Uring {
        Uring {
            flags,
            ring_fd,
            sq: Mutex::new(sq),
            config,
            in_flight: in_flight,
            ticket_queue: ticket_queue,
            loaded: 0.into(),
            submitted: 0.into(),
        }
    }

    pub(crate) fn ensure_submitted(&self, sqe_id: u64) -> io::Result<()> {
        let current = self.submitted.load(Acquire);
        if current >= sqe_id {
            return Ok(());
        }
        let mut sq = {
            let _get_sq_mu = Measure::new(&M.sq_mu_wait);
            self.sq.lock().unwrap()
        };
        let _hold_sq_mu = Measure::new(&M.sq_mu_hold);
        let submitted = sq.submit_all(self.flags, self.ring_fd);
        let old = self.submitted.fetch_add(submitted, Release);

        if self.flags & IORING_SETUP_SQPOLL == 0 {
            // we only check this if we're running in
            // non-SQPOLL mode where we have to manually
            // push our submissions to the kernel.
            assert!(
                old + submitted >= sqe_id,
                "failed to submit our expected SQE on ensure_submitted. \
                expected old {} + submitted {} to be >= sqe_id {}",
                old,
                submitted,
                sqe_id,
            );
        }

        Ok(())
    }

    /// Asynchronously accepts a `TcpStream` from
    /// a provided `TcpListener`.
    ///
    /// # Warning
    ///
    /// This only becomes usable on linux kernels
    /// 5.5 and up.
    pub fn accept<'a>(&'a self, tcp_listener: &'a TcpListener) -> Completion<'a, TcpStream> {
        self.with_sqe(None, false, |sqe| {
            sqe.prep_rw(
                IORING_OP_ACCEPT,
                tcp_listener.as_raw_fd(),
                0,
                0,
                Ordering::None,
            )
        })
    }

    /// Send a buffer to the target socket
    /// or file-like destination.
    ///
    /// Returns the length that was successfully
    /// written.
    ///
    /// # Warning
    ///
    /// This only becomes usable on linux kernels
    /// 5.6 and up.
    pub fn send<'a, F, B>(&'a self, stream: &'a F, iov: &'a B) -> Completion<'a, usize>
    where
        F: AsRawFd,
        B: 'a + AsIoVec,
    {
        self.send_ordered(stream, iov, Ordering::None)
    }

    /// Send a buffer to the target socket
    /// or file-like destination.
    ///
    /// Returns the length that was successfully
    /// written.
    ///
    /// Accepts an `Ordering` specification.
    ///
    /// # Warning
    ///
    /// This only becomes usable on linux kernels
    /// 5.6 and up.
    pub fn send_ordered<'a, F, B>(
        &'a self,
        stream: &'a F,
        iov: &'a B,
        ordering: Ordering,
    ) -> Completion<'a, usize>
    where
        F: AsRawFd,
        B: 'a + AsIoVec,
    {
        let iov = iov.into_new_iovec();

        self.with_sqe(None, true, |sqe| {
            sqe.prep_rw(IORING_OP_SEND, stream.as_raw_fd(), 0, 0, ordering);
            sqe.addr = iov.iov_base as u64;
            sqe.len = u32::try_from(iov.iov_len).unwrap();
        })
    }

    /// Receive data from the target socket
    /// or file-like destination, and place
    /// it in the given buffer.
    ///
    /// Returns the length that was successfully
    /// read.
    ///
    /// # Warning
    ///
    /// This only becomes usable on linux kernels
    /// 5.6 and up.
    pub fn recv<'a, F, B>(&'a self, stream: &'a F, iov: &'a B) -> Completion<'a, usize>
    where
        F: AsRawFd,
        B: AsIoVec + AsIoVecMut,
    {
        self.recv_ordered(stream, iov, Ordering::None)
    }

    /// Receive data from the target socket
    /// or file-like destination, and place
    /// it in the given buffer.
    ///
    /// Returns the length that was successfully
    /// read.
    ///
    /// Accepts an `Ordering` specification.
    ///
    /// # Warning
    ///
    /// This only becomes usable on linux kernels
    /// 5.6 and up.
    pub fn recv_ordered<'a, F, B>(
        &'a self,
        stream: &'a F,
        iov: &'a B,
        ordering: Ordering,
    ) -> Completion<'a, usize>
    where
        F: AsRawFd,
        B: AsIoVec + AsIoVecMut,
    {
        let iov = iov.into_new_iovec();

        self.with_sqe(Some(iov), true, |sqe| {
            sqe.prep_rw(IORING_OP_RECV, stream.as_raw_fd(), 0, 0, ordering);
            sqe.len = u32::try_from(iov.iov_len).unwrap();
        })
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
    pub fn fsync<'a>(&'a self, file: &'a File) -> Completion<'a, ()> {
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
    pub fn fsync_ordered<'a>(&'a self, file: &'a File, ordering: Ordering) -> Completion<'a, ()> {
        self.with_sqe(None, false, |sqe| {
            sqe.prep_rw(IORING_OP_FSYNC, file.as_raw_fd(), 0, 0, ordering)
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
    pub fn fdatasync<'a>(&'a self, file: &'a File) -> Completion<'a, ()> {
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
    pub fn fdatasync_ordered<'a>(
        &'a self,
        file: &'a File,
        ordering: Ordering,
    ) -> Completion<'a, ()> {
        self.with_sqe(None, false, |mut sqe| {
            sqe.prep_rw(IORING_OP_FSYNC, file.as_raw_fd(), 0, 0, ordering);
            sqe.flags |= IORING_FSYNC_DATASYNC;
        })
    }

    /// Synchronizes the data associated with a range
    /// in a file. Does not synchronize any metadata
    /// updates, which can cause data loss if you
    /// are not writing to a file whose metadata
    /// has previously been synchronized.
    ///
    /// You probably want to have a prior write
    /// linked to this, or set `Ordering::Drain`
    /// by using `sync_file_range_ordered` instead.
    ///
    /// Under the hood, this uses the "pessimistic"
    /// set of flags:
    /// `SYNC_FILE_RANGE_WRITE | SYNC_FILE_RANGE_WAIT_AFTER`
    pub fn sync_file_range<'a>(
        &'a self,
        file: &'a File,
        offset: u64,
        len: usize,
    ) -> Completion<'a, ()> {
        self.sync_file_range_ordered(file, offset, len, Ordering::None)
    }

    /// Synchronizes the data associated with a range
    /// in a file. Does not synchronize any metadata
    /// updates, which can cause data loss if you
    /// are not writing to a file whose metadata
    /// has previously been synchronized.
    ///
    /// You probably want to have a prior write
    /// linked to this, or set `Ordering::Drain`.
    ///
    /// Under the hood, this uses the "pessimistic"
    /// set of flags:
    /// `SYNC_FILE_RANGE_WRITE | SYNC_FILE_RANGE_WAIT_AFTER`
    pub fn sync_file_range_ordered<'a>(
        &'a self,
        file: &'a File,
        offset: u64,
        len: usize,
        ordering: Ordering,
    ) -> Completion<'a, ()> {
        self.with_sqe(None, false, |mut sqe| {
            sqe.prep_rw(
                IORING_OP_SYNC_FILE_RANGE,
                file.as_raw_fd(),
                len,
                offset,
                ordering,
            );
            sqe.flags |= u8::try_from(
                // We don't use this because it causes
                // EBADF to be thrown. Looking at
                // linux's fs/sync.c, it seems as though
                // it performs an identical operation
                // as SYNC_FILE_RANGE_WAIT_AFTER.
                // libc::SYNC_FILE_RANGE_WAIT_BEFORE |
                libc::SYNC_FILE_RANGE_WRITE | libc::SYNC_FILE_RANGE_WAIT_AFTER,
            )
            .unwrap();
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
    pub fn write_at<'a, F, B>(&'a self, file: &'a F, iov: &'a B, at: u64) -> Completion<'a, usize>
    where
        F: AsRawFd,
        B: 'a + AsIoVec,
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
    pub fn write_at_ordered<'a, F, B>(
        &'a self,
        file: &'a F,
        iov: &'a B,
        at: u64,
        ordering: Ordering,
    ) -> Completion<'a, usize>
    where
        F: AsRawFd,
        B: 'a + AsIoVec,
    {
        self.with_sqe(Some(iov.into_new_iovec()), false, |sqe| {
            sqe.prep_rw(IORING_OP_WRITEV, file.as_raw_fd(), 1, at, ordering)
        })
    }

    /// Reads data into the provided buffer from the
    /// given file-like object, at the given offest,
    /// using vectored IO. Be sure to check the returned
    /// `io_uring_cqe`'s `res` field to see if a
    /// short read happened. This will contain
    /// the number of bytes read.
    ///
    /// Note that the file argument is generic
    /// for anything that supports AsRawFd:
    /// sockets, files, etc...
    pub fn read_at<'a, F, B>(&'a self, file: &'a F, iov: &'a B, at: u64) -> Completion<'a, usize>
    where
        F: AsRawFd,
        B: AsIoVec + AsIoVecMut,
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
    pub fn read_at_ordered<'a, F, B>(
        &'a self,
        file: &'a F,
        iov: &'a B,
        at: u64,
        ordering: Ordering,
    ) -> Completion<'a, usize>
    where
        F: AsRawFd,
        B: AsIoVec + AsIoVecMut,
    {
        self.with_sqe(Some(iov.into_new_iovec()), false, |sqe| {
            sqe.prep_rw(IORING_OP_READV, file.as_raw_fd(), 1, at, ordering)
        })
    }

    /// Don't do anything. This is
    /// mostly for debugging and tuning.
    pub fn nop<'a>(&'a self) -> Completion<'a, ()> {
        self.nop_ordered(Ordering::None)
    }

    /// Don't do anything. This is
    /// mostly for debugging and tuning.
    pub fn nop_ordered<'a>(&'a self, ordering: Ordering) -> Completion<'a, ()> {
        self.with_sqe(None, false, |sqe| {
            sqe.prep_rw(IORING_OP_NOP, 0, 1, 0, ordering)
        })
    }

    /// Block until all items in the submission queue
    /// are submitted to the kernel. This can
    /// be avoided by using the `SQPOLL` mode
    /// (a privileged operation) on the `Config`
    /// struct.
    ///
    /// Note that this is performed automatically
    /// and in a more fine-grained way when a
    /// `Completion` is consumed via `Completion::wait`
    /// or awaited in a Future context.
    ///
    /// You don't need to call this if you are
    /// calling `.wait()` or `.await` on the
    /// `Completion` quickly, but if you are
    /// doing some other stuff that could take
    /// a while first, calling this will ensure
    /// that the operation is being executed
    /// by the kernel in the mean time.
    pub fn submit_all(&self) {
        let mut sq = {
            let _get_sq_mu = Measure::new(&M.sq_mu_wait);
            self.sq.lock().unwrap()
        };
        let _hold_sq_mu = Measure::new(&M.sq_mu_hold);
        sq.submit_all(self.flags, self.ring_fd);
    }

    fn with_sqe<'a, F, C>(
        &'a self,
        iovec: Option<libc::iovec>,
        msghdr: bool,
        f: F,
    ) -> Completion<'a, C>
    where
        F: FnOnce(&mut io_uring_sqe),
        C: FromCqe,
    {
        let ticket = self.ticket_queue.pop();
        let (mut completion, filler) = pair(self);

        let data_ptr = self.in_flight.insert(ticket, iovec, msghdr, filler);

        let mut sq = {
            let _get_sq_mu = Measure::new(&M.sq_mu_wait);
            self.sq.lock().unwrap()
        };
        let _hold_sq_mu = Measure::new(&M.sq_mu_hold);

        completion.sqe_id = self.loaded.fetch_add(1, Release) + 1;

        let sqe = {
            let _get_sqe = Measure::new(&M.get_sqe);
            loop {
                if let Some(sqe) = sq.try_get_sqe(self.flags) {
                    break sqe;
                } else {
                    let submitted = sq.submit_all(self.flags, self.ring_fd);
                    self.submitted.fetch_add(submitted, Release);
                };
            }
        };

        sqe.user_data = ticket as u64;
        sqe.addr = data_ptr;
        f(sqe);

        completion
    }
}
