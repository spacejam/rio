use std::{
    fs::OpenOptions,
    io::{prelude::*, IoSlice, IoSliceMut, Result},
    os::unix::fs::OpenOptionsExt,
};

use rio::Rio;

#[repr(align(4096))]
struct Aligned([u8; 4096 * 256]);

fn main() -> Result<()> {
    // start the ring
    const RING_SIZE: usize = 256;
    let mut ring =
        Rio::new(RING_SIZE).expect("create uring");

    // open output file
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .custom_flags(libc::O_DIRECT)
        .open("file")
        .expect("open file");

    // create output buffer
    let out_buf = Aligned([42; 4096 * 256]);
    let out_io_slice = IoSlice::new(&out_buf.0);
    let at = 0;

    // write
    ring.enqueue_write(&file, &out_io_slice, at);
    ring.submit_all().expect("submit");
    let cqe = ring.wait_cqe().expect("write wait_cqe");
    cqe.seen();

    // create input buffer
    let mut in_buf = Aligned([0; 4096 * 256]);
    let mut in_io_slice = IoSliceMut::new(&mut in_buf.0);

    // read
    ring.enqueue_read(&file, &mut in_io_slice, at);
    ring.submit_all().expect("submit");
    let cqe = ring.wait_cqe().expect("read wait_cqe");
    cqe.seen();

    assert_eq!(out_buf.0[..], in_buf.0[..]);

    Ok(())
}
