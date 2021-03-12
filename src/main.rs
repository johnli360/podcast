use std::net::TcpListener;
use podaemon::read_lines;

mod player;
use player::*;

fn main() {

    let listener = TcpListener::bind("127.0.0.1:51234").unwrap();
    println!("listening on: {}", listener.local_addr().unwrap());

    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                    print!("connected!");

                    let lines = read_lines(s);
                    lines.iter().for_each(|x| println!("line: {}", x));

                    play(lines);
                    print!("player finished");

                } ,
            Err(e) => print!("Err: {}", e),
        }
    }

    drop(listener);
}
