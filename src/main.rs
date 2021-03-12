use std::{convert::TryFrom, error::Error, io::Read, iter::FromIterator, net::TcpListener, str::from_utf8, thread::{sleep}};
use std::io::BufReader;
use std::io::BufRead;
use std::net::*;
use std::str;

// fn main() -> Result<> {
fn main() {

    let listener = TcpListener::bind("127.0.0.1:51234").unwrap();
    println!("listening on: {}", listener.local_addr().unwrap());

    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                    print!("connected!\n");
                    // do_stuff(s);
                    read_lines(s).iter().for_each(|x| println!("line: {}", x));
                } ,
            Err(e) => print!("Err: {}", e),
        }
    }

    // Unneccesary?
    drop(listener);
}

fn read_lines(stream : TcpStream) -> Vec<String> {
    let reader = BufReader::new(stream);
    reader.lines()
        .map(|res|  {
            match res {
                Ok(l) => l,
                Err(e) => {
                    println!("malformed line: {}", e);
                    String::from("") },
            }
        })
        .collect()
}

fn do_stuff(mut stream : TcpStream) {
    let mut buf : [ u8 ; 64 ] = [ 0 ; 64 ];

    loop {

        match stream.read(&mut buf) {
            Err(e) => println!("Err: {}", e),
            Ok(0) => { println!("Done"); break },
            Ok(cuont) =>
                print!("Read: {} {}",
                       cuont,
                       str::from_utf8(&buf[0..cuont]).unwrap()),
        };
    }
    drop(buf);
}
