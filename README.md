* [documentation](https://docs.rs/rio)
* [chat](https://discord.gg/Z6VsXds)
* [sponsor](https://github.com/sponsors/spacejam)

# rio

misuse-resistant bindings for io_uring, the hottest
thing to happen to linux IO in a long time.

#### Innovations

* only relies on libc, no need for c/bindgen to
  complicate things, nobody wants that
* the completions work great with threads or an
  async runtime (`Completion` implements Future)
* takes advantage of Rust's lifetimes and RAII to guarantee
  that the kernel will never asynchronously write to memory
  that Rust has destroyed.
* uses Rust marker traits to guarantee that a buffer will never
  be written into unless it is writable memory. (prevents
  you from trying to write data into static read-only memory)
* no need to mess with `IoSlice` / `libc::iovec` directly.
  rio maintains these in the background for you.
* If left to its own devices, io_uring will allow you to
  submit more IO operations than would actually fit in
  the completion queue, allowing completions to be dropped
  and causing leaks of any userspace thing waiting for
  the completion. rio exerts backpressure on submitters
  when the number of in-flight requests reaches this
  threshold, to guarantee that no completions will
  be dropped due to completion queue overflow.
* rio will handle submission queue submissions
  automatically. If you start waiting for a
  `Completion`, rio will make sure that we
  have already submitted at least this request
  to the kernel. Other io_uring libraries force
  you to handle this manually, which is another
  possible source of misuse.

This is intended to be the core of [sled's](http://sled.rs) writepath.
It is built with a specific high-level
application in mind: a high performance storage
engine and replication system.

#### What's io_uring?

io_uring is the biggest thing to happen to the
linux kernel in a very long time. It will change
everything. Anything that uses epoll right now
will be rewritten to use io_uring if it wants
to stay relevant. It started as a way to do real
async disk IO without needing to use O_DIRECT, but
its scope has expanded and it will continue to support
more and more kernel functionality over time due to
its ability to batch large numbers different syscalls.
In kernel 5.5 support is added for more networking
operations like `accept(2)`, `sendmsg(2)`, and `recvmsg(2)`.
In 5.6 support is being added for `recv(2)` and `send(2)`.
io_uring [has been measured to dramatically outperform
epoll-based networking, with io_uring outperforming
epoll-based setups more and more under heavier load](https://twitter.com/markpapadakis/status/1216978559601926145).
I started rio to gain an early deep understanding of this
amazing new interface, so that I could use it ASAP and
responsibly with [sled](http://sled.rs).

io_uring unlocks the following kernel features:

* fully-async interface for a growing number of syscalls
* async disk IO without using O_DIRECT as you have
  to do with AIO
* batching hundreds of disk and network IO operations
  into a single syscall, which is especially wonderful
  in a post-meltdown/spectre world where our syscalls have
  [dramatically slowed down](http://www.brendangregg.com/blog/2018-02-09/kpti-kaiser-meltdown-performance.html)
* 0-syscall IO operation submission, if configured in
  SQPOLL mode
* configurable completion polling for trading CPU for
  low latency
* Allows expression of sophisticated 0-copy broadcast
  semantics, similar to splice(2) or sendfile(2) but
  working with many file-like objects without ever
  needing to bounce memory and mappings into userspace
  en-route.
* Allows IO buffers and file descriptors to be registered
  for cheap reuse (remapping buffers and file descriptors
  for use in the kernel has a significant cost).

To read more about io_uring, check out:

* [Ringing in a new asynchronous I/O API](https://lwn.net/Articles/776703/)
* [Efficient IO with io_uring](https://kernel.dk/io_uring.pdf)
* [What’s new with io_uring](https://kernel.dk/io_uring-whatsnew.pdf)
* Follow [Jens Axboe on Twitter](https://twitter.com/axboe) to follow dev progress

For some slides with interesting io_uring performance results,
check out slides 43-53 of [this presentation deck by Jens](https://www.slideshare.net/ennael/kernel-recipes-2019-faster-io-through-iouring).

#### why not use those other Rust io_uring libraries?

* they haven't copied `rio`'s features yet, which you pretty much
  have to use anyway to responsibly use `io_uring` due to the
  sharp edges of the API. All of the libraries I've seen
  as of January 13 2020 are totally easy to overflow the
  completion queue with, as well as easy to express
  use-after-frees with, don't seem to be async-friendly,
  etc...

#### examples that will be broken in the next day or two

async tcp echo server:

```rust
use std::{
    io::self,
    net::{TcpListener, TcpStream},
};

async fn proxy(ring: &rio::Rio, a: &TcpStream, b: &TcpStream) -> io::Result<()> {
    let buf = vec![0_u8; 512];
    loop {
        let read_bytes = ring.read_at(a, &buf, 0).await?;
        let buf = &buf[..read_bytes];
        ring.write_at(b, &buf, 0).await?;
    }
}

fn main() -> io::Result<()> {
    let ring = rio::new()?;
    let acceptor = TcpListener::bind("127.0.0.1:6666")?;

    extreme::run(async {
        // kernel 5.5 and later support TCP accept
        loop {
            let stream = ring.accept(&acceptor).await?;
            dbg!(proxy(&ring, &stream, &stream).await);
        }
    })
}
```

file reading:

```rust
let mut ring = rio::new().expect("create uring");
let file = std::fs::open("file").expect("openat");
let data: &mut [u8] = &mut [0; 66];
let completion = ring.read_at(&file, &mut data, at);

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
let completion = ring.write_at(&file, &dater, at);

// if using threads
completion.wait()?;

// if using async
completion.await?
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
        );
        completions.push(write);

        let read = ring.read_at(&file, &in_slice, at);
        completions.push(read);
    }

    for completion in completions.into_iter() {
        completion.wait()?;
    }

    Ok(())
}
```
