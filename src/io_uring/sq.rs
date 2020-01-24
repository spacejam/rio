use std::slice::from_raw_parts_mut;

use super::*;

/// Sprays uring submissions.
#[derive(Debug)]
pub(crate) struct Sq {
    khead: &'static AtomicU32,
    ktail: &'static AtomicU32,
    kring_mask: &'static u32,
    kflags: &'static AtomicU32,
    kdropped: &'static AtomicU32,
    array: &'static mut [AtomicU32],
    sqes: &'static mut [io_uring_sqe],
    sqe_head: u32,
    sqe_tail: u32,
    ring_ptr: *const libc::c_void,
    ring_mmap_sz: usize,
    sqes_mmap_sz: usize,
}

impl Drop for Sq {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        unsafe {
            libc::munmap(
                self.sqes.as_ptr() as *mut libc::c_void,
                self.sqes_mmap_sz,
            );
        }
        unsafe {
            libc::munmap(
                self.ring_ptr as *mut libc::c_void,
                self.ring_mmap_sz,
            );
        }
    }
}

impl Sq {
    pub(crate) fn new(
        params: &io_uring_params,
        ring_fd: i32,
    ) -> io::Result<Sq> {
        let sq_ring_mmap_sz = params.sq_off.array as usize
            + (params.sq_entries as usize
                * std::mem::size_of::<u32>());

        // TODO IORING_FEAT_SINGLE_MMAP for sq

        let sq_ring_ptr = uring_mmap(
            sq_ring_mmap_sz,
            ring_fd,
            IORING_OFF_SQ_RING,
        )?;

        let sqes_mmap_sz: usize = params.sq_entries
            as usize
            * std::mem::size_of::<io_uring_sqe>();

        let sqes_ptr: *mut io_uring_sqe = uring_mmap(
            sqes_mmap_sz,
            ring_fd,
            IORING_OFF_SQES,
        )? as _;

        #[allow(unsafe_code)]
        Ok(unsafe {
            Sq {
                sqe_head: 0,
                sqe_tail: 0,
                ring_ptr: sq_ring_ptr,
                ring_mmap_sz: sq_ring_mmap_sz,
                sqes_mmap_sz,
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
                kflags: &*(sq_ring_ptr
                    .add(params.sq_off.flags as usize)
                    as *const AtomicU32),
                kdropped: &*(sq_ring_ptr
                    .add(params.sq_off.dropped as usize)
                    as *const AtomicU32),
                array: from_raw_parts_mut(
                    sq_ring_ptr
                        .add(params.sq_off.array as usize)
                        as _,
                    params.sq_entries as usize,
                ),
            }
        })
    }

    pub(crate) fn try_get_sqe(
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

        if next - head <= self.sqes.len() as u32 {
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
        let to_submit = self.sqe_tail - self.sqe_head;

        let mut ktail = self.ktail.load(Acquire);

        for _ in 0..to_submit {
            let index = ktail & mask;
            self.array[index as usize]
                .store(self.sqe_head & mask, Release);
            ktail += 1;
            self.sqe_head += 1;
        }

        let swapped = self.ktail.swap(ktail, Release);
        assert_eq!(swapped, ktail - to_submit);

        to_submit
    }

    pub(crate) fn submit_all(
        &mut self,
        ring_flags: u32,
        ring_fd: i32,
    ) -> io::Result<u64> {
        let submitted = if ring_flags & IORING_SETUP_SQPOLL
            == 0
        {
            // non-SQPOLL mode, we need to use
            // `enter` to submit our SQEs.

            // TODO for polling, keep flags at 0

            let flags = IORING_ENTER_GETEVENTS;
            let flushed = self.flush();
            let mut to_submit = flushed;
            while to_submit > 0 {
                let _ = Measure::new(&M.enter_sqe);
                let ret = enter(
                    ring_fd,
                    to_submit,
                    0,
                    flags,
                    std::ptr::null_mut(),
                )?;
                to_submit -= u32::try_from(ret).unwrap();
            }
            flushed
        } else if self.kflags.load(Acquire)
            & IORING_SQ_NEED_WAKEUP
            != 0
        {
            // the kernel has signalled to us that the
            // SQPOLL thread that checks the submission
            // queue has terminated due to inactivity,
            // and needs to be restarted.
            let to_submit = self.sqe_tail - self.sqe_head;
            let _ = Measure::new(&M.enter_sqe);
            enter(
                ring_fd,
                to_submit,
                0,
                IORING_ENTER_SQ_WAKEUP,
                std::ptr::null_mut(),
            )?;
            0
        } else {
            0
        };
        assert_eq!(self.kdropped.load(Relaxed), 0);
        Ok(u64::from(submitted))
    }
}
