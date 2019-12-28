use libc::{c_int, c_long, c_uint, syscall};

use super::Params;

const SETUP: libc::c_long = 425;
const ENTER: libc::c_long = 426;
const REGISTER: libc::c_long = 427;

pub unsafe fn setup(
    entries: c_uint,
    p: *mut Params,
) -> c_int {
    syscall(SETUP, entries as c_long, p as c_long) as c_int
}

pub unsafe fn enter(
    fd: c_int,
    to_submit: c_uint,
    min_complete: c_uint,
    flags: c_uint,
    sig: *mut libc::sigset_t,
) -> c_int {
    syscall(
        ENTER,
        fd as c_long,
        to_submit as c_long,
        min_complete as c_long,
        flags as c_long,
        sig as c_long,
        core::mem::size_of::<libc::sigset_t>() as c_long,
    ) as c_int
}

pub unsafe fn register(
    fd: c_int,
    opcode: c_uint,
    arg: *const libc::c_void,
    nr_args: c_uint,
) -> c_int {
    syscall(
        REGISTER,
        fd as c_long,
        opcode as c_long,
        arg as c_long,
        nr_args as c_long,
    ) as c_int
}
