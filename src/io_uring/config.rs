use std::sync::Arc;

use super::*;

/// Configuration for the underlying `io_uring` system.
#[derive(Clone, Debug, Copy)]
pub struct Config {
    /// The number of entries in the submission queue.
    /// The completion queue size may be specified by
    /// using `raw_params` instead. By default, the
    /// kernel will choose a completion queue that is 2x
    /// the submission queue's size.
    pub depth: usize,
    /// Enable `SQPOLL` mode, which spawns a kernel
    /// thread that polls for submissions without
    /// needing to block as often to submit.
    ///
    /// This is a privileged operation, and
    /// will cause `start` to fail if run
    /// by a non-privileged user.
    pub sq_poll: bool,
    /// Specify a particular CPU to pin the
    /// `SQPOLL` thread onto.
    pub sq_poll_affinity: u32,
    /// Specify that the user will directly
    /// poll the hardware for operation completion
    /// rather than using the completion queue.
    ///
    /// CURRENTLY UNSUPPORTED
    pub io_poll: bool,
    /// Print a profile table on drop, showing where
    /// time was spent.
    pub print_profile_on_drop: bool,
    /// setting `raw_params` overrides everything else
    pub raw_params: Option<io_uring_params>,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            depth: 256,
            sq_poll: false,
            io_poll: false,
            sq_poll_affinity: 0,
            raw_params: None,
            print_profile_on_drop: false,
        }
    }
}

impl Config {
    /// Start the `Rio` system.
    pub fn start(mut self) -> io::Result<Uring> {
        let mut params =
            if let Some(params) = self.raw_params.take() {
                params
            } else {
                let mut params = io_uring_params::default();

                if self.sq_poll {
                    // set SQPOLL mode to avoid needing wakeup
                    params.flags = IORING_SETUP_SQPOLL;
                    params.sq_thread_cpu =
                        self.sq_poll_affinity;
                }

                params
            };

        let params_ptr: *mut io_uring_params = &mut params;

        let ring_fd = setup(
            u32::try_from(self.depth).unwrap(),
            params_ptr,
        )?;

        if ring_fd < 0 {
            let mut err = io::Error::last_os_error();
            if let Some(12) = err.raw_os_error() {
                err = io::Error::new(
                io::ErrorKind::Other,
                "Not enough lockable memory. You probably \
                 need to raise the memlock rlimit, which \
                 often defaults to a pretty low number.",
            );
            }
            return Err(err);
        }

        let in_flight = Arc::new(InFlight::new(
            params.cq_entries as usize,
        ));

        let ticket_queue = Arc::new(TicketQueue::new(
            params.cq_entries as usize,
        ));

        let sq = Sq::new(&params, ring_fd)?;
        let cq = Cq::new(
            &params,
            ring_fd,
            in_flight.clone(),
            ticket_queue.clone(),
        )?;

        std::thread::spawn(move || {
            let mut cq = cq;
            cq.reaper(ring_fd)
        });

        Ok(Uring::new(
            self,
            params.flags,
            ring_fd,
            sq,
            in_flight,
            ticket_queue,
        ))
    }
}
