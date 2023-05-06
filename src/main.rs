use crossterm::event::{read, Event, KeyEvent};
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{execute, terminal};

// #[macro_use]
// mod macros;

use podaemon::logln;
use podaemon::player::{self, Cmd};
use podaemon::ui::interface::UiUpdate;
// use rss::Channel;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
use tokio::select;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::mpsc::{self, Receiver, Sender};

use std::env;
use std::io::stdout;

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    // console_subscriber::init();
    logln!("init");
    gstreamer::init().unwrap();
    execute!(stdout(), EnterAlternateScreen)?;

    let (tx, rx) = mpsc::channel::<Cmd>(64);
    let tx2 = tx.clone();
    let tx3 = tx2.clone();
    let (ui_tx, ui_rx) = mpsc::channel::<UiUpdate>(64);

    tokio::spawn(async move {
        sig_handler(tx2).await.unwrap();
    });

    if let Ok(port) = env::var("PORT") {
        let addr = format!("0.0.0.0:{port}");
        tokio::spawn(async move {
            listen(tx, &addr).await;
        });
    }
    if let Err(err) = terminal::enable_raw_mode() {
        logln!("{err}");
    };

    let _key_thread_handle = start_key_thread(ui_tx);
    ploop(rx, tx3, ui_rx).await;

    if let Err(err) = terminal::disable_raw_mode() {
        logln!("{err}");
    };

    execute!(stdout(), LeaveAlternateScreen)?;
    Ok(())
}

async fn ploop(mut queue: Receiver<Cmd>, tx: Sender<Cmd>, ui_rx: Receiver<UiUpdate>) {
    let p: Sender<Cmd> = player::new(ui_rx, tx).await;
    while let Some(cmd) = queue.recv().await {
        if cmd == Cmd::Shutdown {
            if let Err(err) = p.send(cmd).await {
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

fn start_key_thread(ui_tx: Sender<UiUpdate>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        while let Ok(c) = read() {
            if let Event::Key(
                event @ KeyEvent {
                    code: _,
                    modifiers: _,
                    kind: _,
                    state: _,
                },
            ) = c
            {
                if let Err(err) = ui_tx.blocking_send(UiUpdate::KeyEvent(event)) {
                    logln!("key error: {err}");
                    break;
                };
            }
        }
    })
}

async fn sig_handler(cmd: Sender<Cmd>) -> Result<(), Box<dyn std::error::Error>> {
    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sigquit = signal(SignalKind::quit())?;
    let mut sigterm = signal(SignalKind::terminate())?;
    loop {
        select! {
            Some(()) = sigterm.recv() => {
                cmd.send(Cmd::Shutdown).await?;
                break;
            }
            Some(()) = sigquit.recv() => {
                cmd.send(Cmd::Shutdown).await?;
                break;
            }
            Some(()) = sigint.recv() => {
                cmd.send(Cmd::Shutdown).await?;
                break;
            }
        }
    }
    Ok(())
}
