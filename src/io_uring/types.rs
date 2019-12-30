use std::sync::atomic::AtomicU32;

#[allow(non_camel_case_types)]
pub type __kernel_rwf_t = libc::c_int;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct Uring {
    pub sq: Sq,
    pub cq: Cq,
    pub flags: libc::c_uint,
    pub ring_fd: libc::c_int,
}

#[repr(C)]
#[derive(Default, Debug, Copy, Clone)]
pub struct Params {
    pub sq_entries: u32,
    pub cq_entries: u32,
    pub flags: u32,
    pub sq_thread_cpu: u32,
    pub sq_thread_idle: u32,
    pub features: u32,
    pub resv: [u32; 4usize],
    pub sq_off: SqringOffsets,
    pub cq_off: CqringOffsets,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct Sq {
    pub khead: *mut AtomicU32,
    pub ktail: *mut AtomicU32,
    pub kring_mask: *mut libc::c_uint,
    pub kring_entries: *mut libc::c_uint,
    pub kflags: *mut libc::c_uint,
    pub kdropped: *mut libc::c_uint,
    pub array: *mut libc::c_uint,
    pub sqes: *mut Sqe,
    pub sqe_head: libc::c_uint,
    pub sqe_tail: libc::c_uint,
    pub ring_sz: usize,
    pub ring_ptr: *mut libc::c_void,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct Cq {
    pub khead: *mut AtomicU32,
    pub ktail: *mut AtomicU32,
    pub kring_mask: *mut libc::c_uint,
    pub kring_entries: *mut libc::c_uint,
    pub koverflow: *mut libc::c_uint,
    pub cqes: *mut Cqe,
    pub ring_sz: usize,
    pub ring_ptr: *mut libc::c_void,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Sqe {
    pub opcode: u8,
    pub flags: u8,
    pub ioprio: u16,
    pub fd: i32,
    pub __bindgen_anon_1: Sqe__bindgen_ty_1,
    pub addr: u64,
    pub len: u32,
    pub __bindgen_anon_2: Sqe__bindgen_ty_2,
    pub user_data: u64,
    pub __bindgen_anon_3: Sqe__bindgen_ty_3,
}
#[repr(C)]
#[derive(Copy, Clone)]
pub union Sqe__bindgen_ty_1 {
    pub off: u64,
    pub addr2: u64,
    _bindgen_union_align: u64,
}
#[repr(C)]
#[derive(Copy, Clone)]
pub union Sqe__bindgen_ty_2 {
    pub rw_flags: __kernel_rwf_t,
    pub fsync_flags: u32,
    pub poll_events: u16,
    pub sync_range_flags: u32,
    pub msg_flags: u32,
    pub timeout_flags: u32,
    pub accept_flags: u32,
    pub cancel_flags: u32,
    pub open_flags: u32,
    pub statx_flags: u32,
    _bindgen_union_align: u32,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union Sqe__bindgen_ty_3 {
    pub buf_index: u16,
    pub __pad2: [u64; 3usize],
    _bindgen_union_align: [u64; 3usize],
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct Cqe {
    pub user_data: u64,
    pub res: i32,
    pub flags: u32,
}

#[repr(C)]
#[derive(Default, Debug, Copy, Clone)]
pub struct SqringOffsets {
    pub head: u32,
    pub tail: u32,
    pub ring_mask: u32,
    pub ring_entries: u32,
    pub flags: u32,
    pub dropped: u32,
    pub array: u32,
    pub resv1: u32,
    pub resv2: u64,
}

#[repr(C)]
#[derive(Default, Debug, Copy, Clone)]
pub struct CqringOffsets {
    pub head: u32,
    pub tail: u32,
    pub ring_mask: u32,
    pub ring_entries: u32,
    pub overflow: u32,
    pub cqes: u32,
    pub resv: [u64; 2usize],
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct io_uring_files_update {
    pub offset: u32,
    pub fds: *mut i32,
}

#[cfg(test)]
mod tests {
    #![allow(non_camel_case_types)]
    #![allow(non_snake_case)]
    use super::*;

    #[test]
    fn bindgen_test_layout_Sqe__bindgen_ty_1() {
        assert_eq!(
            ::std::mem::size_of::<Sqe__bindgen_ty_1>(),
            8usize,
            concat!(
                "Size of: ",
                stringify!(Sqe__bindgen_ty_1)
            )
        );
        assert_eq!(
            ::std::mem::align_of::<Sqe__bindgen_ty_1>(),
            8usize,
            concat!(
                "Alignment of ",
                stringify!(Sqe__bindgen_ty_1)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Sqe__bindgen_ty_1>(
                )))
                .off as *const _ as usize
            },
            0usize,
            concat!(
                "Offset of field: ",
                stringify!(Sqe__bindgen_ty_1),
                "::",
                stringify!(off)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Sqe__bindgen_ty_1>(
                )))
                .addr2 as *const _ as usize
            },
            0usize,
            concat!(
                "Offset of field: ",
                stringify!(Sqe__bindgen_ty_1),
                "::",
                stringify!(addr2)
            )
        );
    }

    #[test]
    fn bindgen_test_layout_Sqe__bindgen_ty_2() {
        assert_eq!(
            ::std::mem::size_of::<Sqe__bindgen_ty_2>(),
            4usize,
            concat!(
                "Size of: ",
                stringify!(Sqe__bindgen_ty_2)
            )
        );
        assert_eq!(
            ::std::mem::align_of::<Sqe__bindgen_ty_2>(),
            4usize,
            concat!(
                "Alignment of ",
                stringify!(Sqe__bindgen_ty_2)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Sqe__bindgen_ty_2>(
                )))
                .rw_flags as *const _
                    as usize
            },
            0usize,
            concat!(
                "Offset of field: ",
                stringify!(Sqe__bindgen_ty_2),
                "::",
                stringify!(rw_flags)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Sqe__bindgen_ty_2>(
                )))
                .fsync_flags as *const _
                    as usize
            },
            0usize,
            concat!(
                "Offset of field: ",
                stringify!(Sqe__bindgen_ty_2),
                "::",
                stringify!(fsync_flags)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Sqe__bindgen_ty_2>(
                )))
                .poll_events as *const _
                    as usize
            },
            0usize,
            concat!(
                "Offset of field: ",
                stringify!(Sqe__bindgen_ty_2),
                "::",
                stringify!(poll_events)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Sqe__bindgen_ty_2>(
                )))
                .sync_range_flags
                    as *const _ as usize
            },
            0usize,
            concat!(
                "Offset of field: ",
                stringify!(Sqe__bindgen_ty_2),
                "::",
                stringify!(sync_range_flags)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Sqe__bindgen_ty_2>(
                )))
                .msg_flags as *const _
                    as usize
            },
            0usize,
            concat!(
                "Offset of field: ",
                stringify!(Sqe__bindgen_ty_2),
                "::",
                stringify!(msg_flags)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Sqe__bindgen_ty_2>(
                )))
                .timeout_flags as *const _
                    as usize
            },
            0usize,
            concat!(
                "Offset of field: ",
                stringify!(Sqe__bindgen_ty_2),
                "::",
                stringify!(timeout_flags)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Sqe__bindgen_ty_2>(
                )))
                .accept_flags as *const _
                    as usize
            },
            0usize,
            concat!(
                "Offset of field: ",
                stringify!(Sqe__bindgen_ty_2),
                "::",
                stringify!(accept_flags)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Sqe__bindgen_ty_2>(
                )))
                .cancel_flags as *const _
                    as usize
            },
            0usize,
            concat!(
                "Offset of field: ",
                stringify!(Sqe__bindgen_ty_2),
                "::",
                stringify!(cancel_flags)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Sqe__bindgen_ty_2>(
                )))
                .open_flags as *const _
                    as usize
            },
            0usize,
            concat!(
                "Offset of field: ",
                stringify!(Sqe__bindgen_ty_2),
                "::",
                stringify!(open_flags)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Sqe__bindgen_ty_2>(
                )))
                .statx_flags as *const _
                    as usize
            },
            0usize,
            concat!(
                "Offset of field: ",
                stringify!(Sqe__bindgen_ty_2),
                "::",
                stringify!(statx_flags)
            )
        );
    }

    #[test]
    fn bindgen_test_layout_Sqe__bindgen_ty_3() {
        assert_eq!(
            ::std::mem::size_of::<Sqe__bindgen_ty_3>(),
            24usize,
            concat!(
                "Size of: ",
                stringify!(Sqe__bindgen_ty_3)
            )
        );
        assert_eq!(
            ::std::mem::align_of::<Sqe__bindgen_ty_3>(),
            8usize,
            concat!(
                "Alignment of ",
                stringify!(Sqe__bindgen_ty_3)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Sqe__bindgen_ty_3>(
                )))
                .buf_index as *const _
                    as usize
            },
            0usize,
            concat!(
                "Offset of field: ",
                stringify!(Sqe__bindgen_ty_3),
                "::",
                stringify!(buf_index)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Sqe__bindgen_ty_3>(
                )))
                .__pad2 as *const _ as usize
            },
            0usize,
            concat!(
                "Offset of field: ",
                stringify!(Sqe__bindgen_ty_3),
                "::",
                stringify!(__pad2)
            )
        );
    }
    #[test]
    fn bindgen_test_layout_Sqe() {
        assert_eq!(
            ::std::mem::size_of::<Sqe>(),
            64usize,
            concat!("Size of: ", stringify!(Sqe))
        );
        assert_eq!(
            ::std::mem::align_of::<Sqe>(),
            8usize,
            concat!("Alignment of ", stringify!(Sqe))
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Sqe>())).opcode
                    as *const _ as usize
            },
            0usize,
            concat!(
                "Offset of field: ",
                stringify!(Sqe),
                "::",
                stringify!(opcode)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Sqe>())).flags
                    as *const _ as usize
            },
            1usize,
            concat!(
                "Offset of field: ",
                stringify!(Sqe),
                "::",
                stringify!(flags)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Sqe>())).ioprio
                    as *const _ as usize
            },
            2usize,
            concat!(
                "Offset of field: ",
                stringify!(Sqe),
                "::",
                stringify!(ioprio)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Sqe>())).fd
                    as *const _ as usize
            },
            4usize,
            concat!(
                "Offset of field: ",
                stringify!(Sqe),
                "::",
                stringify!(fd)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Sqe>())).addr
                    as *const _ as usize
            },
            16usize,
            concat!(
                "Offset of field: ",
                stringify!(Sqe),
                "::",
                stringify!(addr)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Sqe>())).len
                    as *const _ as usize
            },
            24usize,
            concat!(
                "Offset of field: ",
                stringify!(Sqe),
                "::",
                stringify!(len)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Sqe>())).user_data
                    as *const _ as usize
            },
            32usize,
            concat!(
                "Offset of field: ",
                stringify!(Sqe),
                "::",
                stringify!(user_data)
            )
        );
    }

    #[test]
    fn bindgen_test_layout_Cqe() {
        assert_eq!(
            ::std::mem::size_of::<Cqe>(),
            16usize,
            concat!("Size of: ", stringify!(Cqe))
        );
        assert_eq!(
            ::std::mem::align_of::<Cqe>(),
            8usize,
            concat!("Alignment of ", stringify!(Cqe))
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Cqe>())).user_data
                    as *const _ as usize
            },
            0usize,
            concat!(
                "Offset of field: ",
                stringify!(Cqe),
                "::",
                stringify!(user_data)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Cqe>())).res
                    as *const _ as usize
            },
            8usize,
            concat!(
                "Offset of field: ",
                stringify!(Cqe),
                "::",
                stringify!(res)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Cqe>())).flags
                    as *const _ as usize
            },
            12usize,
            concat!(
                "Offset of field: ",
                stringify!(Cqe),
                "::",
                stringify!(flags)
            )
        );
    }

    #[test]
    fn bindgen_test_layout_SqringOffsets() {
        assert_eq!(
            ::std::mem::size_of::<SqringOffsets>(),
            40usize,
            concat!("Size of: ", stringify!(SqringOffsets))
        );
        assert_eq!(
            ::std::mem::align_of::<SqringOffsets>(),
            8usize,
            concat!(
                "Alignment of ",
                stringify!(SqringOffsets)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<SqringOffsets>()))
                    .head as *const _
                    as usize
            },
            0usize,
            concat!(
                "Offset of field: ",
                stringify!(SqringOffsets),
                "::",
                stringify!(head)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<SqringOffsets>()))
                    .tail as *const _
                    as usize
            },
            4usize,
            concat!(
                "Offset of field: ",
                stringify!(SqringOffsets),
                "::",
                stringify!(tail)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<SqringOffsets>()))
                    .ring_mask as *const _
                    as usize
            },
            8usize,
            concat!(
                "Offset of field: ",
                stringify!(SqringOffsets),
                "::",
                stringify!(ring_mask)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<SqringOffsets>()))
                    .ring_entries
                    as *const _ as usize
            },
            12usize,
            concat!(
                "Offset of field: ",
                stringify!(SqringOffsets),
                "::",
                stringify!(ring_entries)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<SqringOffsets>()))
                    .flags as *const _
                    as usize
            },
            16usize,
            concat!(
                "Offset of field: ",
                stringify!(SqringOffsets),
                "::",
                stringify!(flags)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<SqringOffsets>()))
                    .dropped as *const _
                    as usize
            },
            20usize,
            concat!(
                "Offset of field: ",
                stringify!(SqringOffsets),
                "::",
                stringify!(dropped)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<SqringOffsets>()))
                    .array as *const _
                    as usize
            },
            24usize,
            concat!(
                "Offset of field: ",
                stringify!(SqringOffsets),
                "::",
                stringify!(array)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<SqringOffsets>()))
                    .resv1 as *const _
                    as usize
            },
            28usize,
            concat!(
                "Offset of field: ",
                stringify!(SqringOffsets),
                "::",
                stringify!(resv1)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<SqringOffsets>()))
                    .resv2 as *const _
                    as usize
            },
            32usize,
            concat!(
                "Offset of field: ",
                stringify!(SqringOffsets),
                "::",
                stringify!(resv2)
            )
        );
    }

    #[test]
    fn bindgen_test_layout_CqringOffsets() {
        assert_eq!(
            ::std::mem::size_of::<CqringOffsets>(),
            40usize,
            concat!("Size of: ", stringify!(CqringOffsets))
        );
        assert_eq!(
            ::std::mem::align_of::<CqringOffsets>(),
            8usize,
            concat!(
                "Alignment of ",
                stringify!(CqringOffsets)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<CqringOffsets>()))
                    .head as *const _
                    as usize
            },
            0usize,
            concat!(
                "Offset of field: ",
                stringify!(CqringOffsets),
                "::",
                stringify!(head)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<CqringOffsets>()))
                    .tail as *const _
                    as usize
            },
            4usize,
            concat!(
                "Offset of field: ",
                stringify!(CqringOffsets),
                "::",
                stringify!(tail)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<CqringOffsets>()))
                    .ring_mask as *const _
                    as usize
            },
            8usize,
            concat!(
                "Offset of field: ",
                stringify!(CqringOffsets),
                "::",
                stringify!(ring_mask)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<CqringOffsets>()))
                    .ring_entries
                    as *const _ as usize
            },
            12usize,
            concat!(
                "Offset of field: ",
                stringify!(CqringOffsets),
                "::",
                stringify!(ring_entries)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<CqringOffsets>()))
                    .overflow as *const _
                    as usize
            },
            16usize,
            concat!(
                "Offset of field: ",
                stringify!(CqringOffsets),
                "::",
                stringify!(overflow)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<CqringOffsets>()))
                    .cqes as *const _
                    as usize
            },
            20usize,
            concat!(
                "Offset of field: ",
                stringify!(CqringOffsets),
                "::",
                stringify!(cqes)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<CqringOffsets>()))
                    .resv as *const _
                    as usize
            },
            24usize,
            concat!(
                "Offset of field: ",
                stringify!(CqringOffsets),
                "::",
                stringify!(resv)
            )
        );
    }

    #[test]
    fn bindgen_test_layout_Params() {
        assert_eq!(
            ::std::mem::size_of::<Params>(),
            120usize,
            concat!("Size of: ", stringify!(Params))
        );
        assert_eq!(
            ::std::mem::align_of::<Params>(),
            8usize,
            concat!("Alignment of ", stringify!(Params))
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Params>()))
                    .sq_entries as *const _
                    as usize
            },
            0usize,
            concat!(
                "Offset of field: ",
                stringify!(Params),
                "::",
                stringify!(sq_entries)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Params>()))
                    .cq_entries as *const _
                    as usize
            },
            4usize,
            concat!(
                "Offset of field: ",
                stringify!(Params),
                "::",
                stringify!(cq_entries)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Params>())).flags
                    as *const _ as usize
            },
            8usize,
            concat!(
                "Offset of field: ",
                stringify!(Params),
                "::",
                stringify!(flags)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Params>()))
                    .sq_thread_cpu
                    as *const _ as usize
            },
            12usize,
            concat!(
                "Offset of field: ",
                stringify!(Params),
                "::",
                stringify!(sq_thread_cpu)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Params>()))
                    .sq_thread_idle
                    as *const _ as usize
            },
            16usize,
            concat!(
                "Offset of field: ",
                stringify!(Params),
                "::",
                stringify!(sq_thread_idle)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Params>())).features
                    as *const _ as usize
            },
            20usize,
            concat!(
                "Offset of field: ",
                stringify!(Params),
                "::",
                stringify!(features)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Params>())).resv
                    as *const _ as usize
            },
            24usize,
            concat!(
                "Offset of field: ",
                stringify!(Params),
                "::",
                stringify!(resv)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Params>())).sq_off
                    as *const _ as usize
            },
            40usize,
            concat!(
                "Offset of field: ",
                stringify!(Params),
                "::",
                stringify!(sq_off)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Params>())).cq_off
                    as *const _ as usize
            },
            80usize,
            concat!(
                "Offset of field: ",
                stringify!(Params),
                "::",
                stringify!(cq_off)
            )
        );
    }

    #[test]
    fn bindgen_test_layout_io_uring_files_update() {
        assert_eq!(
            ::std::mem::size_of::<io_uring_files_update>(),
            16usize,
            concat!(
                "Size of: ",
                stringify!(io_uring_files_update)
            )
        );
        assert_eq!(
            ::std::mem::align_of::<io_uring_files_update>(),
            8usize,
            concat!(
                "Alignment of ",
                stringify!(io_uring_files_update)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<
                    io_uring_files_update,
                >()))
                .offset as *const _ as usize
            },
            0usize,
            concat!(
                "Offset of field: ",
                stringify!(io_uring_files_update),
                "::",
                stringify!(offset)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<
                    io_uring_files_update,
                >()))
                .fds as *const _ as usize
            },
            8usize,
            concat!(
                "Offset of field: ",
                stringify!(io_uring_files_update),
                "::",
                stringify!(fds)
            )
        );
    }

    #[test]
    fn bindgen_test_layout_io_uring() {
        assert_eq!(
            ::std::mem::size_of::<Uring>(),
            160usize,
            concat!("Size of: ", stringify!(Uring))
        );
        assert_eq!(
            ::std::mem::align_of::<Uring>(),
            8usize,
            concat!("Alignment of ", stringify!(Uring))
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Uring>())).sq
                    as *const _ as usize
            },
            0usize,
            concat!(
                "Offset of field: ",
                stringify!(Uring),
                "::",
                stringify!(sq)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Uring>())).cq
                    as *const _ as usize
            },
            88usize,
            concat!(
                "Offset of field: ",
                stringify!(Uring),
                "::",
                stringify!(cq)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Uring>())).flags
                    as *const _ as usize
            },
            152usize,
            concat!(
                "Offset of field: ",
                stringify!(Uring),
                "::",
                stringify!(flags)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Uring>())).ring_fd
                    as *const _ as usize
            },
            156usize,
            concat!(
                "Offset of field: ",
                stringify!(Uring),
                "::",
                stringify!(ring_fd)
            )
        );
    }

    #[test]
    fn bindgen_test_layout_Sq() {
        assert_eq!(
            ::std::mem::size_of::<Sq>(),
            88usize,
            concat!("Size of: ", stringify!(Sq))
        );
        assert_eq!(
            ::std::mem::align_of::<Sq>(),
            8usize,
            concat!("Alignment of ", stringify!(Sq))
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Sq>())).khead
                    as *const _ as usize
            },
            0usize,
            concat!(
                "Offset of field: ",
                stringify!(Sq),
                "::",
                stringify!(khead)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Sq>())).ktail
                    as *const _ as usize
            },
            8usize,
            concat!(
                "Offset of field: ",
                stringify!(Sq),
                "::",
                stringify!(ktail)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Sq>())).kring_mask
                    as *const _ as usize
            },
            16usize,
            concat!(
                "Offset of field: ",
                stringify!(Sq),
                "::",
                stringify!(kring_mask)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Sq>())).kring_entries
                    as *const _ as usize
            },
            24usize,
            concat!(
                "Offset of field: ",
                stringify!(Sq),
                "::",
                stringify!(kring_entries)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Sq>())).kflags
                    as *const _ as usize
            },
            32usize,
            concat!(
                "Offset of field: ",
                stringify!(Sq),
                "::",
                stringify!(kflags)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Sq>())).kdropped
                    as *const _ as usize
            },
            40usize,
            concat!(
                "Offset of field: ",
                stringify!(Sq),
                "::",
                stringify!(kdropped)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Sq>())).array
                    as *const _ as usize
            },
            48usize,
            concat!(
                "Offset of field: ",
                stringify!(Sq),
                "::",
                stringify!(array)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Sq>())).sqes
                    as *const _ as usize
            },
            56usize,
            concat!(
                "Offset of field: ",
                stringify!(Sq),
                "::",
                stringify!(sqes)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Sq>())).sqe_head
                    as *const _ as usize
            },
            64usize,
            concat!(
                "Offset of field: ",
                stringify!(Sq),
                "::",
                stringify!(sqe_head)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Sq>())).sqe_tail
                    as *const _ as usize
            },
            68usize,
            concat!(
                "Offset of field: ",
                stringify!(Sq),
                "::",
                stringify!(sqe_tail)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Sq>())).ring_sz
                    as *const _ as usize
            },
            72usize,
            concat!(
                "Offset of field: ",
                stringify!(Sq),
                "::",
                stringify!(ring_sz)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Sq>())).ring_ptr
                    as *const _ as usize
            },
            80usize,
            concat!(
                "Offset of field: ",
                stringify!(Sq),
                "::",
                stringify!(ring_ptr)
            )
        );
    }

    #[test]
    fn bindgen_test_layout_Cq() {
        assert_eq!(
            ::std::mem::size_of::<Cq>(),
            64usize,
            concat!("Size of: ", stringify!(Cq))
        );
        assert_eq!(
            ::std::mem::align_of::<Cq>(),
            8usize,
            concat!("Alignment of ", stringify!(Cq))
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Cq>())).khead
                    as *const _ as usize
            },
            0usize,
            concat!(
                "Offset of field: ",
                stringify!(Cq),
                "::",
                stringify!(khead)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Cq>())).ktail
                    as *const _ as usize
            },
            8usize,
            concat!(
                "Offset of field: ",
                stringify!(Cq),
                "::",
                stringify!(ktail)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Cq>())).kring_mask
                    as *const _ as usize
            },
            16usize,
            concat!(
                "Offset of field: ",
                stringify!(Cq),
                "::",
                stringify!(kring_mask)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Cq>())).kring_entries
                    as *const _ as usize
            },
            24usize,
            concat!(
                "Offset of field: ",
                stringify!(Cq),
                "::",
                stringify!(kring_entries)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Cq>())).koverflow
                    as *const _ as usize
            },
            32usize,
            concat!(
                "Offset of field: ",
                stringify!(Cq),
                "::",
                stringify!(koverflow)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Cq>())).cqes
                    as *const _ as usize
            },
            40usize,
            concat!(
                "Offset of field: ",
                stringify!(Cq),
                "::",
                stringify!(cqes)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Cq>())).ring_sz
                    as *const _ as usize
            },
            48usize,
            concat!(
                "Offset of field: ",
                stringify!(Cq),
                "::",
                stringify!(ring_sz)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<Cq>())).ring_ptr
                    as *const _ as usize
            },
            56usize,
            concat!(
                "Offset of field: ",
                stringify!(Cq),
                "::",
                stringify!(ring_ptr)
            )
        );
    }
}
