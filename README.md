* [documentation](https://docs.rs/rio)
* [chat](https://discord.gg/Z6VsXds)
* [sponsor](https://github.com/sponsors/spacejam)

# rio

pre-alpha misuse-resistant bindings for io_uring, focusing
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

# examples that will be broken in the next day or two

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
let out_io_slice = std::io::IoSlice::new(&dater);
let completion = ring.read_at(&file, &in_io_slice, at)?;

// if using threadulous
completion.wait()?;

// if using asyncoos
completion.await?
```

speedy O_DIRECT shi0t (try this at home / run the o_direct example)

```rust
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
```

btw if ur here from the internet u can fuck right the F off
