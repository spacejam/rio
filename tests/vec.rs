use std::io::prelude::*;

#[test]
fn test_vec_value() {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open("data")
        .unwrap();

    let ring = rio::new().unwrap();

    let buffer: Vec<u8> = b"hello world!".to_vec();
    ring.write_at(&file, &buffer, 0).wait().unwrap();

    let mut contents = vec![];
    file.read_to_end(&mut contents).unwrap();

    assert_eq!(contents, b"hello world!".to_vec());

    std::fs::remove_file("data").unwrap();
}
