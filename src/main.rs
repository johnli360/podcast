use podaemon::read_lines;
use std::net::TcpListener;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;

mod player;
use player::*;

type CMD = Vec<String>;

fn main() -> Result<(), std::io::Error> {
    let listener_local = TcpListener::bind("127.0.0.1:51234")?;
    let listener_lan = TcpListener::bind("192.168.0.108:51234")?;

    let (tx, rx): (Sender<CMD>, Receiver<CMD>) = mpsc::channel();
    let tx2 = tx.clone();

    let local_thread = thread::spawn(|| listen(tx, listener_local));
    let lan_thread = thread::spawn(|| listen(tx2, listener_lan));

    ploop(rx);

    local_thread.join().unwrap();
    lan_thread.join().unwrap();
    Ok(())
}

fn ploop(queue: Receiver<CMD>) {
    for cmd in queue.iter() {
        play(cmd, true);
    }
}

fn listen(queue: Sender<CMD>, listener: TcpListener) {
    println!("listening on: {}", listener.local_addr().unwrap());
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                print!("Received: ");
                let lines = read_lines(s);
                lines.iter().for_each(|x| println!("{}", x));

                if let Err(msg) = queue.send(lines) {
                    eprintln!("unable to write to channel: {}", msg);
                }
            }
            Err(e) => print!("Err: {}", e),
        }
    }
}
