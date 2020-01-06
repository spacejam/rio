use std::{
    collections::HashMap,
    slice::from_raw_parts_mut,
    sync::{atomic::AtomicU32, Arc, Mutex},
};

use super::*;

#[derive(Debug)]
pub struct Config {
    pub depth: usize,
    pub sq_poll: bool,
    pub io_poll: bool,
    pub sq_poll_affinity: u32,
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
        }
    }
}

impl Config {
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

        let ring_fd = setup(self.depth as _, params_ptr)?;

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

        #[allow(unsafe_code)]
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
                kring_mask: &*(sq_ring_ptr
                    .add(params.sq_off.ring_mask as usize)
                    as *const u32),
                kring_entries: &*(sq_ring_ptr.add(
                    params.sq_off.ring_entries as usize,
                )
                    as *const u32),
                kflags: &*(sq_ring_ptr
                    .add(params.sq_off.flags as usize)
                    as *const u32),
                kdropped: sq_ring_ptr
                    .add(params.sq_off.dropped as usize)
                    as _,
                array: from_raw_parts_mut(
                    sq_ring_ptr
                        .add(params.sq_off.array as usize)
                        as _,
                    params.sq_entries as usize,
                ),
                max_id: 0,
            }
        };

        // TODO IORING_FEAT_SINGLE_MMAP for cq
        let cq_ring_sz = params.cq_off.cqes as usize
            + (params.cq_entries as usize
                * std::mem::size_of::<io_uring_cqe>());

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

        #[allow(unsafe_code)]
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
                kring_mask: &*(cq_ring_ptr
                    .add(params.cq_off.ring_mask as usize)
                    as *const u32),
                kring_entries: &*(cq_ring_ptr.add(
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
                    params.cq_entries as usize,
                ),
                pending: HashMap::new(),
            }
        };

        let cq_arc = Arc::new(Mutex::new(cq));
        let completion_cq_arc = cq_arc.clone();

        std::thread::spawn(move || {
            reaper(ring_fd, completion_cq_arc)
        });

        Ok(Uring {
            flags: params.flags,
            ring_fd,
            sq: Mutex::new(sq),
            cq: cq_arc,
        })
    }
}
