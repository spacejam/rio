use std::{
    fs::OpenOptions,
    io::{IoSlice, IoSliceMut, Result},
    os::unix::fs::OpenOptionsExt,
};

#[repr(align(4096))]
struct Aligned([u8; 4096 * 256]);

fn main() -> Result<()> {
    // start the ring
    let mut ring = rio::new().expect("create uring");

    // open output file
    let file = OpenOptions::new()
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

    // create input buffer
    let mut in_buf = Aligned([0; 4096 * 256]);
    let mut in_io_slice = IoSliceMut::new(&mut in_buf.0);

    let mut promises = vec![];

    for _ in 0..1000 {
        // write
        let promise =
            ring.write(&file, &out_io_slice, at)?;
        promises.push(promise);

        // read
        let promise =
            ring.read(&file, &mut in_io_slice, at)?;
        promises.push(promise);
    }

    for promise in promises.into_iter() {
        promise
            .wait()
            .expect("should be able to write and read");
    }

    assert_eq!(out_buf.0[..], in_buf.0[..]);

    Ok(())
}
