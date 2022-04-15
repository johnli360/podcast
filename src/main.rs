use podaemon::read_lines;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc::{self, Receiver, Sender};
// use tokio::stream::StreamExt;
// use std::sync::mpsc::{Receiver, Sender};

mod player;
use player::*;

type CMD = String;

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    gstreamer::init().unwrap();
    // let listener_lan = TcpListener::bind("192.168.0.108:51234").await?;

    // let (tx, rx): (Sender<CMD>, Receiver<CMD>) = tokio::sync::mpsc::channel(64);
    let (tx, rx) = mpsc::channel::<CMD>(64);
    let tx2 = tx.clone();

    // let local_thread = std::thread::spawn(|| listen(tx, listener_local));
    // let lan_thread = std::thread::spawn(|| listen(tx2, listener_lan));
    tokio::spawn(async {
        listen(tx, "192.168.0.108:51234").await;
    });
    tokio::spawn(async {
        listen(tx2,  "127.0.0.1:51234").await;
    });

    // tokio::spawn(async { ploop(rx).await }).await?;
    ploop(rx).await;

    // local_thread.join().unwrap().await;
    // lan_thread.join().unwrap().await;
    // loop {}

    Ok(())
}

// stop
async fn ploop(mut queue: Receiver<CMD>) {
    let mut playas : Vec<Sender<Cmd>>= Vec::new();
    while let Some(cmd) = queue.recv().await {
        if cmd.starts_with("stop") {
            for p in &playas {
                if let Err(err) = p.send(player::Cmd::Shutdown).await {
                    eprintln!("{err}");
                }
            }

        } else {
            let cmd = String::from("file:///home/jl/programming/rust/musicplayer/test.mp3");
            let tx = play(&cmd, true).await;
            playas.push(tx);
            println!("done with: {:?}", &cmd);
        }
    }
}

// async fn listen(queue: Sender<CMD>, listener: TcpListener) {
async fn listen(queue: Sender<CMD>, addr: &str) {
    let listener = TcpListener::bind(addr).await.unwrap();
    println!("listening on: {}", listener.local_addr().unwrap());
    // while let Ok((mut socket, _addr)) = listener.accept().await {
    while let Ok((mut socket, _addr)) = listener.accept().await {
        println!("new connection");
        let mut buf = String::new();
        match socket.read_to_string(&mut buf).await {
            Ok(n) => {
                println!("read {n}, {buf}");
                if let Err(msg) = queue.send(buf).await {
                    eprintln!("receiver dropped: {}", msg);
                }
                println!("sent");
            }
            Err(e) => print!("Err: {}", e),
        }
    }
    println!("loop ended");
}
