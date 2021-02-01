use std::{
    io::self,
    net::{UdpSocket},
};

fn main() -> io::Result<()> {
    let ring = rio::new();
    let socket = UdpSocket::bind("0.0.0.0:34254")?;

    extreme::run(async {
        let buffer = &mut [0u8; 1024];
        loop {
            let (amt, peer) = ring.recv_from(&socket, &buffer).await?;
            let peer_bstr = &buffer[..amt];
            println!("Got bytes: {} with bytestring {:?} from peer {:?}",
                     amt, &peer_bstr, peer);
            let sent = ring.send_to(&socket, &peer_bstr, &peer).await?;
            println!("Sent bytes: {} to peer", sent);
            assert_eq!(amt, sent);
        }
    })
}
