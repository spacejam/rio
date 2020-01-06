use std::{
    fs::OpenOptions,
    io::{IoSlice, Result},
    os::unix::fs::OpenOptionsExt,
};

const CHUNK_SIZE: u64 = 4096 * 256;

// `O_DIRECT` requires all reads and writes
// to be aligned to the block device's block
// size. 4096 might not be the best, or even
// a valid one, for yours!
#[repr(align(4096))]
struct Aligned([u8; CHUNK_SIZE as usize]);

fn main() -> Result<()> {
    // start the ring
    let ring = rio::new().expect("create uring");

    // open output file, with `O_DIRECT` set
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .custom_flags(libc::O_DIRECT)
        .open("file")
        .expect("open file");

    // create output buffer
    let out_buf = Aligned([42; CHUNK_SIZE as usize]);
    let out_io_slice = IoSlice::new(&out_buf.0);

    let mut completions = vec![];

    for i in 0..(4 * 1024) {
        let at = i * CHUNK_SIZE;

        let completion =
            ring.write_at(&file, &out_io_slice, at)?;
        completions.push(completion);
    }

    ring.submit_all()?;

    for completion in completions.into_iter() {
        completion.wait()?;
    }

    Ok(())
}
