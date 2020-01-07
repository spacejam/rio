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
    let mut config = rio::Config::default();
    config.print_profile_on_drop = true;
    let ring = config.start().expect("create uring");

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

    let pre = std::time::Instant::now();
    for i in 0..(10 * 1024) {
        let at = i * CHUNK_SIZE;

        let completion =
            ring.write_at(&file, &out_io_slice, at)?;
        completions.push(completion);
    }

    let post_submit = std::time::Instant::now();

    ring.submit_all()?;

    for completion in completions.into_iter() {
        completion.wait()?;
    }

    let post_complete = std::time::Instant::now();

    dbg!(post_submit - pre, post_complete - post_submit);

    Ok(())
}
