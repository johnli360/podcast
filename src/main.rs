use std::io::{Write, stdout};

use crossterm::cursor::MoveTo;
use crossterm::event::{self, read, Event, KeyCode, KeyEvent};
use crossterm::style::Print;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{terminal, execute};
use podaemon::read_lines;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc::{self, Receiver, Sender};
// use tokio::stream::StreamExt;
// use std::sync::mpsc::{Receiver, Sender};

#[macro_use]
mod macros;
mod player;
use player::*;

type CMD = String;

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    gstreamer::init().unwrap();
    execute!(stdout(), EnterAlternateScreen)?;

    let (tx, rx) = mpsc::channel::<CMD>(64);
    let tx2 = tx.clone();
    let tx3 = tx.clone();

    tokio::spawn(async {
        listen(tx, "192.168.0.108:51234").await;
    });
    tokio::spawn(async {
        listen(tx2, "127.0.0.1:51234").await;
    });
    if let Err(err) = terminal::enable_raw_mode() {
        eprintln_raw!("{err}");
    };

    let key_thread = std::thread::spawn(move || {
        while let Ok(c) = read() {
            if let Event::Key(KeyEvent { code, .. }) = c {
                match code {
                    KeyCode::Char('q') => {
                        tx3.blocking_send("quit".into()).expect("bad send :(");
                        break;
                    }
                    KeyCode::Char(c) => {
                        println_raw!("pressed: {c:?}");
                    }
                    _ => {}
                }
            }
        }
    });

    ploop(rx).await;

    key_thread.join().unwrap();

    if let Err(err) = terminal::disable_raw_mode() {
        eprintln_raw!("{err}");
    };

    execute!(stdout(), LeaveAlternateScreen)?;
    Ok(())
}

async fn ploop(mut queue: Receiver<CMD>) {
    let mut playas: Vec<Sender<Cmd>> = Vec::new();
    while let Some(cmd) = queue.recv().await {
        if cmd.starts_with("stop") {
            for p in &playas {
                if let Err(err) = p.send(player::Cmd::Shutdown).await {
                    eprintln_raw!("{err}");
                }
            }
        } else if cmd == "quit" {
            for p in &playas {
                if let Err(err) = p.send(player::Cmd::Shutdown).await {
                    eprintln_raw!("{err}");
                }
                p.closed().await;
            }

            return;
        } else {
            let cmd = String::from("file:///home/jl/programming/rust/musicplayer/test.mp3");
            let tx = play(&cmd, true).await;
            playas.push(tx);
            println_raw!("done with: {:?}", &cmd);
        }
    }
}

async fn listen(queue: Sender<CMD>, addr: &str) {
    let listener = TcpListener::bind(addr).await.unwrap();
    println_raw!("listening on: {}", listener.local_addr().unwrap());

    while let Ok((mut socket, _addr)) = listener.accept().await {
        println_raw!("new connection");
        let mut buf = String::new();
        match socket.read_to_string(&mut buf).await {
            Ok(n) => {
                println_raw!("read {n}, {buf}");
                if let Err(msg) = queue.send(buf).await {
                    eprintln_raw!("receiver dropped: {}", msg);
                }
                println_raw!("sent");
            }
            Err(e) => print!("Err: {}", e),
        }
    }
    println_raw!("loop ended");
}
