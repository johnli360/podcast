use crossterm::event::{read, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{LeaveAlternateScreen, EnterAlternateScreen};
use crossterm::{terminal, execute};

#[macro_use]
mod macros;

use podaemon::player::{Cmd, self};
// use rss::Channel;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
use tokio::sync::mpsc::{self, Receiver, Sender};

use podaemon::dir::get_file;

use std::io::stdout;

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

    let key_thread = start_key_thread(tx3);
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
                for line in buf.lines() {
                    if let Some(cmd) = player::parse_cmd(line) {
                        if let Err(msg) = queue.send(cmd).await {
                            eprintln_raw!("receiver dropped: {}", msg);
                        }
                    }
                }
            }
            Err(e) => print!("Err: {}", e),
        }
        buf.clear();
    }
    println_raw!("loop ended");
}

fn start_key_thread(tx3: Sender<Cmd>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
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
                    Char('o') => get_file()
                        .map(|p| Cmd::Queue(p))
                        .map(|c| tx3.blocking_send(c)),
                    Char('h') | Char('H') | KeyCode::Left => {
                        let cmd = if modifiers.intersects(KeyModifiers::SHIFT) {
                            Cmd::Prev
                        } else {
                            Cmd::SeekRelative(-10)
                        };
                        Some(tx3.blocking_send(cmd))
                    }
                    Char('l') | Char('L') | KeyCode::Right => {
                        let cmd = if modifiers.intersects(KeyModifiers::SHIFT) {
                            Cmd::Next
                        } else {
                            Cmd::SeekRelative(10)
                        };
                        Some(tx3.blocking_send(cmd))
                    }
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
    })
}
