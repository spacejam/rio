mod io_uring;
mod syscall;

use io_uring::{Cqe, Params, Sqe};
use syscall::{enter, register, setup};
