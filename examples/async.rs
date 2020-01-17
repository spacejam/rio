use std::{
    io::self,
    net::{TcpListener, TcpStream},
};

async fn proxy(ring: &rio::Rio, a: &TcpStream, b: &TcpStream) -> io::Result<()> {
    // for kernel 5.6 and later, io_uring will support
    // recv/send which will more gracefully handle
    // reads of larger sizes.
    let buf = vec![0_u8; 1024];
    loop {
        let read = ring.recv_ordered(
            a,
            &buf,
            rio::Ordering::Link,
        )?;
        let write = ring.send(b, &buf)?;
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
