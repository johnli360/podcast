use std::io::{stdout, Write};

use crossterm::event::{self, read, Event, KeyCode, KeyEvent};

use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{execute, terminal};

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc::{self, Receiver, Sender};
// use tokio::stream::StreamExt;
// use std::sync::mpsc::{Receiver, Sender};

#[macro_use]
mod macros;
mod player;
use player::*;

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    gstreamer::init().unwrap();
    execute!(stdout(), EnterAlternateScreen)?;

    let (tx, rx) = mpsc::channel::<Cmd>(64);
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
        let mut done = false;
        while let Ok(c) = read() {
            if let Event::Key(KeyEvent { code, modifiers }) = c {
                use KeyCode::Char;
                let res = match code {
                    Char('q') => {
                        done = true;
                        Some(tx3.blocking_send(Cmd::Quit))
                    }
                    Char(' ') => Some(tx3.blocking_send(Cmd::PlayPause)),
                    Char('h') | KeyCode::Left => Some(tx3.blocking_send(Cmd::SeekRelative(-10))),
                    Char('l') | KeyCode::Right => Some(tx3.blocking_send(Cmd::SeekRelative(10))),
                    c => {
                        println_raw!("pressed: {c:?}, mods: {modifiers:?}");
                        None
                    }
                };
                if let Some(Err(err)) = res {
                    eprintln_raw!("key error: {err}");
                }
            }
            if done {
                break;
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

async fn ploop(mut queue: Receiver<Cmd>) {
    let p: Sender<Cmd> = player::new().await;
    while let Some(cmd) = queue.recv().await {
        if cmd == Cmd::Quit {
            if let Err(err) = p.send(player::Cmd::Shutdown).await {
                eprintln_raw!("{err}");
            }
            p.closed().await;
            return;
        }

        if let Err(err) = p.send(cmd).await {
            eprintln_raw!("{err}");
        }
    }
}

async fn listen(queue: Sender<Cmd>, addr: &str) {
    let listener = TcpListener::bind(addr).await.unwrap();
    println_raw!("listening on: {}", listener.local_addr().unwrap());

    let mut buf = String::new();
    while let Ok((mut socket, _addr)) = listener.accept().await {
        println_raw!("new connection");
        match socket.read_to_string(&mut buf).await {
            Ok(n) => {
                println_raw!("read {n} from socket");
                if let Some(cmd) = player::parse_cmd(&buf) {
                    if let Err(msg) = queue.send(cmd).await {
                        eprintln_raw!("receiver dropped: {}", msg);
                    }
                }
            }
            Err(e) => print!("Err: {}", e),
        }
        buf.clear();
    }
    println_raw!("loop ended");
}
