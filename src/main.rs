use crossterm::event::{read, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{execute, terminal};

#[macro_use]
mod macros;

use podaemon::player::{self, Cmd};
use podaemon::ui::UiUpdate;
use podaemon::ui::_log;
// use rss::Channel;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
use tokio::sync::mpsc::{self, Receiver, Sender};

use std::collections::VecDeque;
use std::env;
use std::io::stdout;
use std::sync::Mutex;

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    unsafe {
        let msgs = VecDeque::with_capacity(50);
        podaemon::ui::LOG = Mutex::new(Some(msgs));
    }
    console_subscriber::init();
    logln!("init");
    gstreamer::init().unwrap();
    execute!(stdout(), EnterAlternateScreen)?;

    let (tx, rx) = mpsc::channel::<Cmd>(64);
    let tx2 = tx.clone();
    let tx3 = tx.clone();
    let (ui_tx, ui_rx) = mpsc::channel::<UiUpdate>(64);
    let ui_tx_key_thread = ui_tx.clone();

    if let Ok(port) = env::var("PORT") {
        {
            let addr = format!("192.168.10.3:{}", &port);
            tokio::spawn(async move {
                listen(tx, &addr).await;
            });
        }
        tokio::spawn(async move {
            let addr = format!("127.0.0.1:{port}");
            listen(tx2, &addr).await;
        });
    }
    if let Err(err) = terminal::enable_raw_mode() {
        logln!("{err}");
    };

    let key_thread = start_key_thread(tx3, ui_tx_key_thread);
    ploop(rx, ui_rx).await;

    key_thread.join().unwrap();

    if let Err(err) = terminal::disable_raw_mode() {
        logln!("{err}");
    };

    execute!(stdout(), LeaveAlternateScreen)?;
    Ok(())
}

async fn ploop(mut queue: Receiver<Cmd>, ui_rx: Receiver<UiUpdate>) {
    let p: Sender<Cmd> = player::new(ui_rx).await;
    while let Some(cmd) = queue.recv().await {
        if cmd == Cmd::Quit {
            if let Err(err) = p.send(player::Cmd::Shutdown).await {
                logln!("{err}");
            }
            p.closed().await;
            return;
        }

        if let Err(err) = p.send(cmd).await {
            logln!("{err}");
        }
    }
}

async fn listen(queue: Sender<Cmd>, addr: &str) {
    let listener = TcpListener::bind(addr).await.unwrap();
    logln!("listening on: {}", listener.local_addr().unwrap());

    let mut buf = String::new();
    while let Ok((mut socket, _addr)) = listener.accept().await {
        match socket.read_to_string(&mut buf).await {
            Ok(_n) => {
                for line in buf.lines() {
                    if let Some(cmd) = player::parse_cmd(line) {
                        if let Err(msg) = queue.send(cmd).await {
                            logln!("receiver dropped: {}", msg);
                        }
                    }
                }
            }
            Err(e) => print!("Err: {}", e),
        }
        buf.clear();
    }
    logln!("loop ended");
}

fn start_key_thread(tx3: Sender<Cmd>, ui_tx: Sender<UiUpdate>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut done = false;
        let mut editing = false;
        while let Ok(c) = read() {
            if let Event::Key(
                event @ KeyEvent {
                    code,
                    modifiers,
                    kind: _,
                    state: _,
                },
            ) = c
            {
                use KeyCode::Char;
                // ui_tx
                // .blocking_send(UiUpdate::Log(format!("Key: {c:?}, editing: {editing}")))
                // .unwrap();
                if editing {
                    if let Err(err) = ui_tx.blocking_send(UiUpdate::KeyEvent(event)) {
                        logln!("key error: {err}");
                    }
                    match code {
                        KeyCode::Enter | KeyCode::Esc => {
                            editing = false;
                        }
                        _ => {}
                    }
                    continue;
                }

                let res = match code {
                    Char('q') => {
                        done = true;
                        Some(tx3.blocking_send(Cmd::Quit))
                    }
                    Char(' ') => Some(tx3.blocking_send(Cmd::PlayPause)),
                    Char('o') => {
                        if let Err(err) = ui_tx.blocking_send(UiUpdate::BrowseFile) {
                            logln!("key error: {err}");
                        } else {
                            editing = true;
                        };
                        None
                    }
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
                    Char('d')
                    | Char('D')
                    | Char('j')
                    | Char('k')
                    | KeyCode::Up
                    | KeyCode::Down
                    | KeyCode::Enter => {
                        if let Err(err) = ui_tx.blocking_send(UiUpdate::KeyEvent(event)) {
                            logln!("key error: {err}");
                        };
                        None
                    }
                    KeyCode::Tab => {
                        if let Err(err) = ui_tx.blocking_send(UiUpdate::Tab) {
                            logln!("key error: {err}");
                        };
                        None
                    }
                    c => {
                        logln!("pressed: {c:?}, mods: {modifiers:?}");
                        None
                    }
                };
                if let Some(Err(err)) = res {
                    logln!("key error: {err}");
                }
            }
            if done {
                break;
            }
        }
    })
}
