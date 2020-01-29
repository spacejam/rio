use std::{
    io,
    net::{TcpListener, TcpStream},
};

const RESP: &'static str = "HTTP/1.0 200 OK\r\n\r\n";

static COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn counter() -> io::Result<()> {
    let mut last = 0;
    loop {
        let current = COUNT.load(std::sync::atomic::Ordering::Relaxed);
        let diff = current - last;
        println!("{}", diff);
        last = current;
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

fn serve(ring: rio::Rio, acceptor: TcpListener) -> io::Result<()> {
    extreme::run(async move {
        loop {
            let stream = ring.accept(&acceptor).wait()?;
            let mut buf = RESP;
            while !buf.is_empty() {
                let written_bytes =
                    ring.write_at(&stream, &buf, 0).await?;
                buf = &buf[written_bytes..];
            }
            COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    })
}

fn main() -> io::Result<()> {
    let ring = rio::new()?;
    let acceptor = TcpListener::bind("127.0.0.1:6666")?;

    let mut threads = vec![];

    for _ in 0..1 {
        let acceptor = acceptor.try_clone().unwrap();
        let ring = ring.clone();
        threads.push(std::thread::spawn(move || {
            serve(ring, acceptor)
        }));
    }

    threads.push(std::thread::spawn(counter));

    for thread in threads.into_iter() {
        thread.join().unwrap().unwrap();
    }

    Ok(())
}
