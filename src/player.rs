use gst::prelude::*;
use gstreamer as gst;
use tokio_stream::StreamExt;
// use std::futures::StreamExt;
use std::time::Duration;
use tokio::{select, sync::mpsc::Sender, time};

struct Player {
    /// Our one and only element
    playbin: gst::Element,
    /// Are we in the PLAYING state?
    playing: bool,
    // Should we terminate execution?
    // terminated : bool,
    // observe: bool,
    // thread: Option<JoinHandle<()>>,
    /// Is seeking enabled for this media?
    seek_enabled: bool,
    /// Have we performed the seek already?
    duration: Option<gst::ClockTime>,
}

// struct Observer{
impl Player {
    fn new() -> Self {
        let playbin = gst::ElementFactory::make("playbin", Some("playbin"))
            .expect("Failed to create playbin element");

        Player {
            playbin,
            playing: false,
            // terminated: false,
            // observe: false,
            // thread: None,
            // observer:
            seek_enabled: false,
            duration: gst::ClockTime::NONE,
        }
    }

    fn queue(&self, uri: &str) {
        self.playbin.set_property("uri", uri);
    }

    fn play(&mut self) {
        if let Err(err) = self.playbin.set_state(gst::State::Playing) {
            eprintln!("Unable to set the playbin to the `Playing` state: {err}");
        }
    }
}

#[derive(Debug)]
pub enum Cmd {
    Play,
    Queue(String),
    Shutdown,
}

fn handle_cmd(cmd: Cmd, player: &mut Player) -> bool {
    // if let Some(cmd) = cmd {
    match cmd {
        Cmd::Play => {
            player.play();
        }
        Cmd::Queue(uri) => player.queue(&uri),
        Cmd::Shutdown => {
            // Shutdown pipeline
            player
                .playbin
                .set_state(gst::State::Null)
                .expect("Unable to set the pipeline to the `Null` state");
            return false;
        }
    }
    return true;
}

async fn new() -> Sender<Cmd> {
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);
    tokio::spawn(async move {
        let mut player = Player::new();
        let bus = player.playbin.bus().unwrap();
        let mut bus_stream = bus.stream();
        let mut interval = time::interval(Duration::from_millis(100));

        'exit: loop {
            select! {
                Some(cmd) = rx.recv() => {
                    println!("new cmd: {cmd:?}");
                    if !handle_cmd(cmd, &mut player)  {
                        break 'exit
                    }
                }
                msg = bus_stream.next() => {
                    if let Some(msg) = msg {
                        handle_message(&mut player, &msg)
                    }
                }
                _ = interval.tick() => {
                    if player.playing {
                        if player.duration.is_none() {
                            player.duration = player.playbin.query_duration();
                        }

                        let position = player
                            .playbin
                            .query_position::<gst::ClockTime>()
                            .expect("Could not query current position.");
                        print!("\r{position} / {}", player.duration.display());
                    }
                }

            }
        }
    });

    tx
}

pub async fn play(input: &str, block: bool) -> Sender<Cmd> {
    println!("{input:?}");

    let tx = new().await;
    if let Err(err) = tx.send(Cmd::Queue(input.to_string())).await {
        eprintln!("queue {err}");
    }

    if let Err(err) = tx.send(Cmd::Play).await {
        eprintln!("play {err}");
    }
    tx
}

fn handle_message(player: &mut Player, msg: &gst::Message) {
    use gst::MessageView;

    match msg.view() {
        MessageView::Error(err) => {
            println!(
                "Error received from element {:?}: {} ({:?})",
                err.src().map(|s| s.path_string()),
                err.error(),
                err.debug()
            );
        }
        MessageView::Eos(..) => {
            println!("End-Of-Stream reached.");
        }
        MessageView::DurationChanged(_) => {
            // The duration has changed, mark the current one as invalid
            player.duration = gst::ClockTime::NONE;
        }
        MessageView::StateChanged(state_changed) => {
            if state_changed
                .src()
                .map(|s| s == player.playbin)
                .unwrap_or(false)
            {
                let new_state = state_changed.current();
                let old_state = state_changed.old();

                println!(
                    "Pipeline state changed from {:?} to {:?}",
                    old_state, new_state
                );

                player.playing = new_state == gst::State::Playing;

                if player.playing {
                    let mut seeking = gst::query::Seeking::new(gst::Format::Time);
                    if player.playbin.query(&mut seeking) {
                        let (seekable, start, end) = seeking.result();
                        player.seek_enabled = seekable;
                        if seekable {
                            println!("Seeking is ENABLED from {} to {}", start, end)
                        } else {
                            println!("Seeking is DISABLED for this stream.")
                        }
                    } else {
                        eprintln!("Seeking query failed.")
                    }
                }
            }
        }
        _ => (),
    }
}
