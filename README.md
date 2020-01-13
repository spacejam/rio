* [documentation](https://docs.rs/rio)
* [chat](https://discord.gg/Z6VsXds)
* [sponsor](https://github.com/sponsors/spacejam)

# rio

misuse-resistant bindings for io_uring, the hottest
thing to happen to linux IO in a long time.

#### Innovations

* only relies on libc, no need for c/bindgen to complicate things, nobody wants that
* the completions work great with threads or an async runtime (`Completion` implements Future)
* takes advantage of Rust's lifetimes and RAII to guarantee
  that the kernel will never asynchronously write to memory
  that Rust has destroyed.
* uses Rust marker traits to guarantee that a buffer will never
  be written into unless it is writable memory. (prevents
  you from trying to write data into static read-only memory)
* no need to mess with `IoSlice` / `libc::iovec` directly.
  rio maintains these in the background for you.

This is intended to be the core of [sled's](http://sled.rs) writepath.
It is built with a specific high-level
application in mind: a high performance storage
engine and replication system.

#### What's io_uring?

io_uring is the biggest thing to happen to the
linux kernel in a very long time. It will change
everything. Anything that uses epoll right now
will be rewritten to use io_uring if it wants
to stay relevant. I built rio to gain an early
deep understanding of this amazing new interface,
so that I could use it ASAP  and responsibly with
[sled](http://sled.rs).

io_uring unlocks the following kernel features:

* real, fully-async disk IO without using O_DIRECT
  as you have to do with AIO
* batching hundreds of disk and network IO operations
  into a single syscall, which is especially wonderful
  in a post-meltdown/spectre world where our syscalls have
  [dramatically slowed down](http://www.brendangregg.com/blog/2018-02-09/kpti-kaiser-meltdown-performance.html)
* 0-syscall IO operation submission, if configured in
  SQPOLL mode
* 0-syscall IO operation completion polling, unless
  configured in IOPOLL mode.
* Allows expression of sophisticated 0-copy broadcast
  semantics, similar to splice(2) or sendfile(2) but
  working with many file-like objects without ever
  needing to bounce memory and mappings into userspace
  en-route.
* Allows IO buffers and file descriptors to be registered
  for cheap reuse (remapping buffers and file descriptors
  for use in the kernel has a significant cost).

To read more about io_uring, check out:

* [Efficient IO with io_uring](https://kernel.dk/io_uring.pdf)
* [Ringing in a new asynchronous I/O API](https://lwn.net/Articles/776703/)
* Follow [Jens Axboe on Twitter](https://twitter.com/axboe) to follow dev progress

#### why not use those other Rust io_uring libraries?

* they haven't copied `rio`'s features yet, which you pretty much
  have to use anyway to responsibly use `io_uring` due to the
  sharp edges of the API.

#### examples that will be broken in the next day or two

file reading:

```rust
let mut ring = rio::new().expect("create uring");
let file = std::fs::open("file").expect("openat");
let dater: &mut [u8] = &[0; 66];
let completion = ring.read(&file, &mut dater, at)?;

// if using threads
completion.wait()?;

// if using async
completion.await?
```

file writing:

```rust
let mut ring = rio::new().expect("create uring");
let file = std::fs::create("file").expect("openat");
let dater: &[u8] = &[6; 66];
let completion = ring.read_at(&file, &dater, at)?;

// if using threads
completion.wait()?;

// if using async
completion.await?
```

tcp echo server:

```rust
use std::{
    io::{self, prelude::*},
    net::{TcpListener, TcpStream},
};

fn proxy(a: &TcpStream, b: &TcpStream) -> io::Result<()> {
    let ring = rio::new()?;

    // for kernel 5.6 and later, io_uring will support
    // recv/send which will more gracefully handle
    // reads of larger sizes.
    let mut buf = vec![0_u8; 1];
    loop {
        let read = ring.read_at_ordered(
            a,
            &mut buf,
            0,
            rio::Ordering::Link,
        )?;
        let write = ring.write_at(b, &buf, 0)?;
        ring.submit_all()?;
        read.wait()?;
        write.wait()?;
    }
}

fn main() -> io::Result<()> {
    let acceptor = TcpListener::bind("127.0.0.1:6666")?;

    // kernel 5.5 and later support TCP accept
    for stream_res in acceptor.incoming() {
        let stream = stream_res?;
        proxy(&stream, &stream);
    }

    Ok(())
}
```

speedy O_DIRECT shi0t (try this at home / run the o_direct example)

```rust
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
    let ring = rio::new()?;

    // open output file, with `O_DIRECT` set
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .custom_flags(libc::O_DIRECT)
        .open("file")?;

    let out_buf = Aligned([42; CHUNK_SIZE as usize]);
    let out_slice: &[u8] = &out_buf.0;

    let in_buf = Aligned([42; CHUNK_SIZE as usize]);
    let in_slice: &[u8] = &in_buf.0;

    let mut completions = vec![];

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

        let read = ring.read_at(&file, &in_slice, at)?;
        completions.push(read);
    }

    // Submissions will happen lazily when we fill up
    // the submission queue, but we should hit this
    // ourselves for now. In the future there might
    // be a thread that does this automatically
    // at some interval if there's work to submit.
    ring.submit_all()?;

    for completion in completions.into_iter() {
        completion.wait()?;
    }

    Ok(())
}
```
