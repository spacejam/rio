use std::io::{Read, Seek, SeekFrom, Write};
use std::mem::forget;
use std::ptr;
use std::slice;
use std::time::Instant;
use std::{fs::OpenOptions, io::Result};

const BUF_SIZE: usize = 1024 * 32;
const FILE_SIZE: usize = 1024 * 1024 * 512;
const QUEUE_DEPTH: usize = 32;

pub fn black_box<T>(dummy: T) -> T {
    unsafe {
        let ret = ptr::read_volatile(&dummy);
        forget(dummy);
        ret
    }
}



fn main() -> Result<()> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open("file")
        .unwrap();

    // do this without loading it all in memory
    let buf: Vec<u8> = vec![1; FILE_SIZE];
    file.write_all(&buf).unwrap();
    file.sync_data().unwrap();

    file.seek(SeekFrom::Start(0)).unwrap();
    let mut buffer: [u8; BUF_SIZE] = [0; BUF_SIZE];
    let mut sum: u64 = 0;
    let mut instant = Instant::now();
    loop {
        let n = file.read(&mut buffer).unwrap();
        sum += buffer[0] as u64;
        sum += n as u64;
        if n < BUF_SIZE {
            break;
        }
    }

    println!(
        "read(2) done in {}ms, checksum: {}",
        instant.elapsed().as_millis(),
        sum
    );

    let mut buffers: Vec<Vec<u8>> = vec![vec![0; BUF_SIZE]; QUEUE_DEPTH];
    let config = rio::Config {
        depth: QUEUE_DEPTH * 2,
        io_poll: false,
        sq_poll: true,
        sq_poll_affinity: 1,
        print_profile_on_drop: false,
        raw_params: None,
    };

    let  ring = config.start().unwrap();
    use std::os::unix::io::AsRawFd;
    dbg!(ring.register(&[file.as_raw_fd()])).unwrap();
    let mut bytes_left: usize = FILE_SIZE;
    let mut offset: usize = 0;
    let mut done: bool = false;
    instant = Instant::now();
    sum = 0;

    while !done {
        let ptr = buffers.as_mut_ptr();
        let mut completions = vec![];

        for i in 0..QUEUE_DEPTH {
            unsafe {
                let buf = &slice::from_raw_parts_mut(ptr.offset(i as isize), 1)[0];
                completions.push((ring.registered_file_read_at(0, buf, offset as u64), i))
            }

            if bytes_left > BUF_SIZE {
                bytes_left -= BUF_SIZE;
                offset += BUF_SIZE;
            } else {
                done = true;
                break;
            }
        }

        for (completion, _i) in completions.into_iter() {
            sum += dbg!(completion.wait()).unwrap() as u64;
        }

        for buf in &buffers {
            sum += buf[0] as u64;
        }
    }

    println!(
        "io_uring(2) done in {}ms, checksum: {}",
        instant.elapsed().as_millis(),
        sum
    );

    Ok(())
}
