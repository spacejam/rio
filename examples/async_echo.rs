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
            if let Err(_) = proxy(&ring, &stream, &stream).await {
                eprintln!("client disconnected");
            }
        }
    })
}
