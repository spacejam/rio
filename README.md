* [documentation](https://docs.rs/rio)
* [chat](https://discord.gg/Z6VsXds)
* [sponsor](https://github.com/sponsors/spacejam)

# rio

misuse-resistant bindings for io_uring, focusing
on users who want to do high-performance storage.

* only relies on libc, no need for c/bindgen to complicate things, nobody wants that
* the completions implement Future, if ur asyncy
* as it gets built out, will leverage as much of the Rust type system as possible to prevent misuse

This is a very early-stage project, but it will
be the core of [sled's](http://sled.rs) IO stack
over time. It is built with a specific high-level
application in mind: a high performance storage
engine and replication system.

sled expects to use the following features:

* SQE linking for dependency specification
* SQPOLL mode for 0-syscall operation
* registered files & IO buffers for lower overhead
* write, read, connect, fsync, fdatasync, O_DIRECT

# examples

readn

```rust
let mut ring = rio::new().expect("create uring");
let file = std::fs::open("poop_file").expect("openat");
let mut dater = [0; 66];
let mut in_io_slice = std::io::IoSliceMut::new(&mut dater);
let completion = ring.read(&file, &mut in_io_slice, at)?;

// if using threaddies
completion.wait()?;

// if using asyncus
completion.await?
```

writen

```rust
let mut ring = rio::new().expect("create uring");
let file = std::fs::create("poop_file").expect("openat");
let dater = [6; 66];
let out_io_slice = std::io::IoSlice::new(&mut dater);
let completion = ring.read(&file, &in_io_slice, at)?;

// if using threadulous
completion.wait()?;

// if using asyncoos
completion.await?
```

speedy O_DIRECT shi0t

```rust
use std::{
    fs::OpenOptions,
    io::{IoSlice, IoSliceMut, Result},
    os::unix::fs::OpenOptionsExt,
};

const CHUNK_SIZE: u64 = 4096 * 256;

// `O_DIRECT` requires all reads and writes
// to be aligned to the block device's block
// size. 4096 might not be the best, or even
// a valid one, for yours!
#[repr(align(4096))]
struct Aligned([u8; CHUNK_SIZE as usize]);

// start the ring
let mut ring = rio::new().expect("create uring");

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

// create input buffer
let mut in_buf = Aligned([0; CHUNK_SIZE as usize]);
let mut in_io_slice = IoSliceMut::new(&mut in_buf.0);

let mut completions = vec![];

for i in 0..(4 * 1024) {
    let at = i * CHUNK_SIZE;

    // Write using `Ordering::Link`,
    // causing the next operation to wait
    // for the this operation
    // to complete before starting.
    //
    // If this operation does not
    // fully complete, the next linked
    // operation fails with `ECANCELED`.
    //
    // io_uring executes unchained
    // operations out-of-order to
    // improve performance. It interleaves
    // operations from different chains
    // to improve performance.
    let completion = ring.write_ordered(
        &file,
        &out_io_slice,
        at,
        rio::Ordering::Link,
    )?;
    completions.push(completion);

    let completion =
        ring.read(&file, &mut in_io_slice, at)?;
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
```

btw if ur here from the internet u can fuck right the F off
