use std::ptr::null_mut;

use super::*;

pub(crate) struct InFlight {
    iovecs: UnsafeCell<Vec<libc::iovec>>,
    msghdrs: UnsafeCell<Vec<libc::msghdr>>,
    fillers: UnsafeCell<Vec<Option<Filler>>>,
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
    pub(crate) fn new(size: usize) -> InFlight {
        let iovecs = UnsafeCell::new(vec![
            libc::iovec {
                iov_base: null_mut(),
                iov_len: 0
            };
            size
        ]);
        let msghdrs = UnsafeCell::new(vec![
            #[allow(unsafe_code)]
            unsafe { MaybeUninit::<libc::msghdr>::zeroed().assume_init() };
            size
        ]);

        let mut filler_vec = Vec::with_capacity(size);
        for _ in 0..size {
            filler_vec.push(None);
        }
        let fillers = UnsafeCell::new(filler_vec);
        InFlight {
            iovecs,
            msghdrs,
            fillers,
        }
    }

    pub(crate) fn insert(
        &self,
        ticket: usize,
        iovec: Option<libc::iovec>,
        msghdr: bool,
        filler: Filler,
    ) -> u64 {
        #[allow(unsafe_code)]
        unsafe {
            let iovec_ptr = self.iovecs.get();
            let msghdr_ptr = self.msghdrs.get();
            if let Some(iovec) = iovec {
                (*iovec_ptr)[ticket] = iovec;

                if msghdr {
                    (*msghdr_ptr)[ticket].msg_iov =
                        (*iovec_ptr)
                            .as_mut_ptr()
                            .add(ticket);
                    (*msghdr_ptr)[ticket].msg_iovlen = 1;
                }
            }
            (*self.fillers.get())[ticket] = Some(filler);
            if iovec.is_some() {
                if msghdr {
                    (*msghdr_ptr).as_mut_ptr().add(ticket)
                        as u64
                } else {
                    (*iovec_ptr).as_mut_ptr().add(ticket)
                        as u64
                }
            } else {
                0
            }
        }
    }

    pub(crate) fn take_filler(
        &self,
        ticket: usize,
    ) -> Filler {
        #[allow(unsafe_code)]
        unsafe {
            (*self.fillers.get())[ticket].take().unwrap()
        }
    }
}
