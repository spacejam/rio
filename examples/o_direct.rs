use std::{
    fs::OpenOptions, io::Result,
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

    let out_buf = Aligned([42; CHUNK_SIZE as usize]);
    let out_slice: &[u8] = &out_buf.0;

    let mut in_buf = Aligned([42; CHUNK_SIZE as usize]);
    let in_slice: &mut [u8] = &mut in_buf.0;

    let mut completions = vec![];

    let pre = std::time::Instant::now();
    for i in 0..(10 * 1024) {
        let at = i * CHUNK_SIZE;

        // By setting the `Link` order,
        // we specify that the following
        // read should happen after this
        // write.
        let write = ring.write_at_ordered(
            &file,
            &out_slice,
            at,
            rio::Ordering::Link,
        )?;
        completions.push(write);

        // This operation will not start
        // until the previous linked one
        // finishes.
        let read = ring.read_at(&file, &in_slice, at)?;
        completions.push(read);
    }

    let post_submit = std::time::Instant::now();

    for completion in completions.into_iter() {
        completion.wait()?;
    }

    let post_complete = std::time::Instant::now();

    dbg!(post_submit - pre, post_complete - post_submit);

    Ok(())
}
