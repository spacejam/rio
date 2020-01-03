* [documentation](https://docs.rs/sled)
* [chat](https://discord.gg/Z6VsXds)
* [sponsor](https://github.com/sponsors/spacejam)

# rio

misuse-resistant bindings for io_uring, focusing
on users who want to do high-performance storage.

* only relies on libc, no need for c/bindgen to complicate things
* the completions implement Future if you boxed yourself into an async codebase

This is a very early-stage project, but it will
be the core of [sled's](http://sled.rs) IO stack
over time. It is built with a specific high-level
application in mind: a high performance storage
engine and replication system.

```rust
use std::{
    fs::OpenOptions,
    io::{IoSlice, IoSliceMut, Result},
    os::unix::fs::OpenOptionsExt,
};

const BLOCK_SIZE: u64 = 4096;

#[repr(align(4096))]
struct Aligned([u8; BLOCK_SIZE as usize]);

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
    let out_buf = Aligned([42; BLOCK_SIZE as usize]);
    let out_io_slice = IoSlice::new(&out_buf.0);

    // create input buffer
    let mut in_buf = Aligned([0; BLOCK_SIZE as usize]);
    let mut in_io_slice = IoSliceMut::new(&mut in_buf.0);

    let mut completions = vec![];

    for i in 0..1024 {
        let at = i * BLOCK_SIZE;
        // Write
        let completion =
            ring.write(&file, &out_io_slice, at)?;
        completions.push(completion);

        // Read, using `Ordering::Link`,
        // causing the read to wait for the
        // previous operation (write in this
        // case) to complete before starting.
        //
        // If the previous operation does not
        // fully complete, this operation
        // fails with `ECANCELED`.
        //
        // io_uring executes unchained
        // operations out-of-order to
        // improve performance.
        let completion = ring.read_ordered(
            &file,
            &mut in_io_slice,
            at,
            rio::Ordering::Link,
        )?;
        completions.push(completion);
    }

    ring.submit_all()?;

    let mut canceled = 0;
    for completion in completions.into_iter() {
        match completion.wait() {
            Err(e) if e.raw_os_error() == Some(125) => {
                canceled += 1
            }
            Ok(_) => {}
            other => panic!("error: {:?}", other),
        }
    }

    println!(
        "lost {} reads due to incomplete linked writes",
        canceled
    );

    if out_buf.0[..] != in_buf.0[..] {
        eprintln!("read buffer did not properly contain expected written bytes");
    }

    Ok(())
}
```
