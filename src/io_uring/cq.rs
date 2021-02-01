#![allow(unsafe_code)]

use std::slice::from_raw_parts_mut;

use super::*;

/// Consumes uring completions.
#[derive(Debug)]
pub struct Cq {
    khead: *mut AtomicU32,
    ktail: *mut AtomicU32,
    kring_mask: *mut u32,
    koverflow: *mut AtomicU32,
    cqes: *mut [io_uring_cqe],
    ticket_queue: Arc<TicketQueue>,
    in_flight: Arc<InFlight>,
    ring_ptr: *const libc::c_void,
    ring_mmap_sz: usize,
}

#[allow(unsafe_code)]
unsafe impl Send for Cq {}

impl Drop for Cq {
    fn drop(&mut self) {
        #[allow(unsafe_code)]
        unsafe {
            libc::munmap(
                self.ring_ptr as *mut libc::c_void,
                self.ring_mmap_sz,
            );
        }
    }
}

impl Cq {
    pub(crate) fn new(
        params: &io_uring_params,
        ring_fd: i32,
        in_flight: Arc<InFlight>,
        ticket_queue: Arc<TicketQueue>,
    ) -> io::Result<Cq> {
        // TODO IORING_FEAT_SINGLE_MMAP for cq
        let cq_ring_mmap_sz = params.cq_off.cqes as usize
            + (params.cq_entries as usize
                * std::mem::size_of::<io_uring_cqe>());

        let cq_ring_ptr = uring_mmap(
            cq_ring_mmap_sz,
            ring_fd,
            IORING_OFF_CQ_RING,
        )?;

        #[allow(unsafe_code)]
        Ok(unsafe {
            Cq {
                ring_ptr: cq_ring_ptr,
                ring_mmap_sz: cq_ring_mmap_sz,
                khead: cq_ring_ptr
                    .add(params.cq_off.head as usize)
                    as *mut AtomicU32,
                ktail: cq_ring_ptr
                    .add(params.cq_off.tail as usize)
                    as *mut AtomicU32,
                kring_mask: cq_ring_ptr
                    .add(params.cq_off.ring_mask as usize)
                    as *mut u32,
                koverflow: cq_ring_ptr
                    .add(params.cq_off.overflow as usize)
                    as *mut AtomicU32,
                cqes: from_raw_parts_mut(
                    cq_ring_ptr
                        .add(params.cq_off.cqes as usize)
                        as _,
                    params.cq_entries as usize,
                ),
                in_flight: in_flight.clone(),
                ticket_queue: ticket_queue.clone(),
            }
        })
    }

    pub(crate) fn reaper(&mut self, ring_fd: i32) {
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
                assert_eq!(
                    unsafe {
                        (*self.koverflow).load(Relaxed)
                    },
                    0
                );
                if self.reap_ready_cqes().is_none() {
                    // poison pill detected, time to shut down
                    return;
                }
            }
        }
    }

    fn reap_ready_cqes(&mut self) -> Option<usize> {
        let _ = Measure::new(&M.reap_ready);
        let mut head =
            unsafe { &*self.khead }.load(Acquire);
        let tail = unsafe { &*self.ktail }.load(Acquire);
        let count = tail - head;

        // hack to get around mutable usage in loop
        // limitation as of rust 1.40
        let mut cq_opt = Some(self);

        let mut to_push =
            Vec::with_capacity(count as usize);

        while head != tail {
            let cq = cq_opt.take().unwrap();
            let index = head & unsafe { *cq.kring_mask };
            let cqe = &unsafe { &*cq.cqes }[index as usize];

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
                let address = cq.in_flight.take_address(ticket as usize);
                Ok(CqeData{
                    cqe: *cqe,
                    address: address,
                })
            };

            completion_filler.fill(result);

            unsafe { &*cq.khead }.fetch_add(1, Release);
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
