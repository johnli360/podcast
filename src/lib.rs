use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::net::*;
use std::str;

pub fn read_lines(stream: TcpStream) -> Vec<String> {
    let reader = BufReader::new(stream);
    reader
        .lines()
        .map(|res| match res {
            Ok(l) => l,
            Err(e) => {
                println!("malformed line: {}", e);
                String::from("")
            }
        })
        .collect()
}

pub fn do_stuff(mut stream: TcpStream) {
    let mut buf: [u8; 64] = [0; 64];

    loop {
        match stream.read(&mut buf) {
            Err(e) => println!("Err: {}", e),
            Ok(0) => {
                println!("Done");
                break;
            }
            Ok(cuont) => print!(
                "Read: {} {}",
                cuont,
                str::from_utf8(&buf[0..cuont]).unwrap()
            ),
        };
    }
    drop(buf);
}
