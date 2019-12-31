* [documentation](https://docs.rs/sled)
* [chat](https://discord.gg/Z6VsXds)
* [sponsor](https://github.com/sponsors/spacejam)

# rio

minimal, misuse-resistant bindings for io_uring

```rust
let rio = rio::new();

// open output file
let file = OpenOptions::new()
    .read(true)
    .write(true)
    .create(true)
    .open("file")
    .expect("open file");

let out_buf = [42; 4096];
let out_io_slice = IoSlice::new(&out_buf);
let at = 0;

// write
ring.enqueue_write(&file, &out_io_slice, at);
ring.submit_all().expect("submit");
let cqe = ring.wait_cqe().expect("write wait_cqe");
cqe.seen();

// fsync
ring.enqueue_fsync(&file);
ring.submit_all().expect("submit");
let cqe = ring.wait_cqe().unwrap();
cqe.seen();

// create input buffer
let mut in_buf = [0; 4096];
let mut in_io_slice = IoSliceMut::new(&mut in_buf);

ring.enqueue_read(&file, &mut in_io_slice, at);

ring.submit_all().expect("submit");
let cqe = ring.wait_cqe().unwrap();
cqe.seen();

assert_eq!(out_buf[..], in_buf[..]);
```
