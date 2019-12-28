#[cfg(target_os = "linux")]
mod io_uring;

pub use io_uring::{
    Cqe, CqringOffsets, MyUring, Sqe, SqringOffsets, Uring,
};
