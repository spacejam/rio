use std::ptr::null_mut;
use std::net::{SocketAddr, IpAddr, Ipv4Addr};

use super::*;

pub(crate) struct InFlight {
    iovecs: UnsafeCell<Vec<libc::iovec>>,
    msghdrs: UnsafeCell<Vec<libc::msghdr>>,
    fillers: UnsafeCell<Vec<Option<Filler>>>,
    addresses: UnsafeCell<Vec<Option<SocketAddr>>>,
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
        let mut addresses_vec = Vec::with_capacity(size);
        for _ in 0..size {
            filler_vec.push(None);
            addresses_vec.push(None);
        }
        let fillers = UnsafeCell::new(filler_vec);
        let addresses = UnsafeCell::new(addresses_vec);
        InFlight {
            iovecs,
            msghdrs,
            fillers,
            addresses,
        }
    }

    pub(crate) fn insert(
        &self,
        ticket: usize,
        iovec: Option<libc::iovec>,
        address: Option<(*const libc::sockaddr, libc::socklen_t)>,
        msghdr: bool,
        filler: Filler,
    ) -> u64 {
        #[allow(unsafe_code)]
        unsafe {
            let iovec_ptr = self.iovecs.get();
            let msghdr_ptr = self.msghdrs.get();
            let addresses_ptr = self.addresses.get();
            if let Some(iovec) = iovec {
                (*iovec_ptr)[ticket] = iovec;

                if msghdr {
                    (*msghdr_ptr)[ticket].msg_iov =
                        (*iovec_ptr)
                            .as_mut_ptr()
                            .add(ticket);
                    (*msghdr_ptr)[ticket].msg_iovlen = 1;
                    if let Some((sname, slen)) = address {
                        (*addresses_ptr)[ticket] = None;
                        (*msghdr_ptr)[ticket].msg_name = sname as *mut libc::c_void;
                        (*msghdr_ptr)[ticket].msg_namelen = slen;
                    } else {
                        (*addresses_ptr)[ticket] =
                            Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0));
                        let (sname, slen) = addr2raw((*addresses_ptr)[ticket].as_ref().unwrap());
                        (*msghdr_ptr)[ticket].msg_name = sname as *mut libc::c_void;
                        (*msghdr_ptr)[ticket].msg_namelen = slen;
                    }
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

    pub(crate) fn take_address(
        &self,
        ticket: usize,
    ) -> Option<SocketAddr> {
        #[allow(unsafe_code)]
        unsafe {
            (*self.addresses.get())[ticket].take()
        }
    }
}
