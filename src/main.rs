use std::net::TcpListener;
use podaemon::read_lines;
use std::thread;

mod player;
use player::*;

fn main() -> Result<(), std::io::Error> {

    let listener_local = TcpListener::bind("127.0.0.1:51234")?;
    let listener_lan   = TcpListener::bind("192.168.0.98:51234")?;

    let local_thread = thread::spawn(|| listen(listener_local));
    let lan_thread = thread::spawn(|| listen(listener_lan));

    local_thread.join().unwrap();
    lan_thread.join().unwrap();
    Ok(())

}

fn listen(listener : TcpListener) {

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
}
