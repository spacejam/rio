use super::*;

pub(crate) struct InFlight {
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
    pub(crate) fn new(size: usize) -> InFlight {
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

    pub(crate) fn insert(
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

    pub(crate) fn take_filler(
        &self,
        ticket: usize,
    ) -> Filler<io::Result<io_uring_cqe>> {
        #[allow(unsafe_code)]
        unsafe {
            (*self.fillers.get())[ticket].take().unwrap()
        }
    }
}
