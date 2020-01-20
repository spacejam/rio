use std::{
    io::self,
    net::{TcpListener, TcpStream},
};

async fn proxy(ring: &rio::Rio, a: &TcpStream, b: &TcpStream) -> io::Result<()> {
    // for kernel 5.6 and later, io_uring will support
    // recv/send which will more gracefully handle
    // reads of larger sizes.
    //
    // this example uses send/recv, see the `async.rs` example
    // for one that uses tcp send/recv, which lets us use larger
    // buffers.
    let buf = vec![0_u8; 1];
    loop {
        let read = ring.read_at_ordered(
            a,
            &buf,
            0,
            rio::Ordering::Link,
        )?;
        let write = ring.write_at(b, &buf, 0)?;
        read.await?;
        write.await?;
    }
}

fn main() -> io::Result<()> {
    let ring = rio::new()?;
    let acceptor = TcpListener::bind("127.0.0.1:6666")?;

    extreme::run(async {
        // kernel 5.5 and later support TCP accept
        loop {
            let stream = ring.accept(&acceptor)?.await?;
            proxy(&ring, &stream, &stream).await;
        }
    })
}
