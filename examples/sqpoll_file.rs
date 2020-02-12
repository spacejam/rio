use std::{fs::OpenOptions, io::Result, os::unix::io::AsRawFd};

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
        .write(true)
        .create(true)
        .truncate(true)
        .open("file")
        .expect("open file");

    let out_buf = Aligned([42; CHUNK_SIZE as usize]);
    let out_slice: &[u8] = &out_buf.0;

    let mut in_buf = Aligned([42; CHUNK_SIZE as usize]);
    let in_slice: &mut [u8] = &mut in_buf.0;

    let mut completions = vec![];

    let pre = std::time::Instant::now();
    dbg!();
    for i in 0..(10) {
        let at = i * CHUNK_SIZE;

        // By setting the `Link` order,
        // we specify that the following
        // read should happen after this
        // write.
        let write = ring.write_at_ordered(&file, &out_slice, at, rio::Ordering::Link);
        completions.push(write);
    }

    for completion in completions.drain(..) {
        completion.wait()?;
    }
    dbg!();

    let mut config = rio::Config::default();
    config.sq_poll = true;
    config.print_profile_on_drop = true;
    let ring = config.start().expect("create uring");

    let mut completions = vec![];

    // open output file, with `O_DIRECT` set
    let file = dbg!(OpenOptions::new()
        .read(true)
        .open("file")
        .expect("open file"));

    let fds = dbg!([file.as_raw_fd()]);
    dbg!(ring.register(&fds).expect("Failed to register"));

    // This operation will not start
    // until the previous linked one
    // finishes.

    for i in 0..(10) {
        dbg!();
        let at = i * CHUNK_SIZE;
        let read = ring.registered_file_read_at(0, &in_slice, at);
        completions.push(read);
    }

    let post_submit = std::time::Instant::now();

    dbg!();
    for completion in completions.drain(..) {
        dbg!(completion.wait()?);
    }

    let post_complete = std::time::Instant::now();

    dbg!(post_submit - pre, post_complete - post_submit);

    Ok(())
}
