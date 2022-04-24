use tokio_stream::StreamExt;
use std::{io::Write, time::Duration};
use tokio::{select, sync::mpsc::Sender, time};

use super::Cmd;

pub async fn new() -> Sender<Cmd> {
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);
    tokio::spawn(async move {
        let mut player = Player::new();
        let bus = player.playbin.bus().unwrap();
        let mut bus_stream = bus.stream();
        let mut interval = time::interval(Duration::from_millis(100));

        loop {
            select! {
            Some(cmd) = rx.recv() => {
                println_raw!("new cmd: {cmd:?}");
                if !run_cmd(cmd, &mut player)  {
                        return
                }
            }
            msg = bus_stream.next() => {
                if let Some(msg) = msg {
                    handle_message(&mut player, &msg)
                }
            }
            _ = interval.tick() => { report_pos(&mut player).await; }
            }
        }
    });

    tx
}

fn run_cmd(cmd: Cmd, player: &mut Player) -> bool {
    match cmd {
        Cmd::Play => player.play(),
        Cmd::Pause => player.pause(),
        Cmd::PlayPause => player.play_pause(),
        Cmd::Queue(uri) => player.queue(&uri),
        Cmd::Seek(pos) => player.seek(pos),
        Cmd::SeekRelative(delta) => player.seek_relative(delta),
        Cmd::Shutdown | Cmd::Quit => {
            player.set_null();
            return false;
        }
    }
    true
}

async fn report_pos(player: &mut Player) {
    if player.playing {
        if player.duration.is_none() {
            player.duration = player.playbin.query_duration();
        }

        if let Some(position) = player.query_position() {
            print_raw!("\r{position} / {}", player.duration.display());
        } else {
            eprintln_raw!("Could not query current position.")
        }

        if let Err(err) = std::io::stdout().flush() {
            eprintln_raw!("failed flush: {err}");
        }
    }
}

fn handle_message(player: &mut Player, msg: &gst::Message) {
    use gst::MessageView;

    match msg.view() {
        MessageView::Error(err) => {
            println_raw!(
                "Error received from element {:?}: {} ({:?})",
                err.src().map(|s| s.path_string()),
                err.error(),
                err.debug()
            );
        }
        MessageView::Eos(..) => {
            println_raw!("End-Of-Stream reached.");
            player.set_null();
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

                println_raw!(
                    "Pipeline state changed from {:?} to {:?}",
                    old_state,
                    new_state
                );

                player.playing = new_state == gst::State::Playing;

                if player.playing {
                    let mut seeking = gst::query::Seeking::new(gst::Format::Time);
                    if player.playbin.query(&mut seeking) {
                        let (seekable, start, end) = seeking.result();
                        player.seek_enabled = seekable;
                        if seekable {
                            println_raw!("Seeking is ENABLED from {} to {}", start, end)
                        } else {
                            println_raw!("Seeking is DISABLED for this stream.")
                        }
                    } else {
                        eprintln_raw!("Seeking query failed.")
                    }
                }
            }
        }
        _ => (),
    }
}

use gst::prelude::*;
use gstreamer as gst;

pub struct Player {
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
            eprintln_raw!("Unable to set the playbin to the `Playing` state: {err}");
        }
    }

    fn play_pause(&mut self) {
        let state = if self.playing {
            gst::State::Paused
        } else {
            gst::State::Playing
        };
        if let Err(err) = self.playbin.set_state(state) {
            eprintln_raw!("Unable to set the playbin to `{state:?}`: {err}");
        }
    }

    fn pause(&mut self) {
        self.playbin
            .set_state(gst::State::Paused)
            .expect("Unable to set the pipeline to the `Paused` state");
    }

    fn set_null(&mut self) {
        self.playbin
            .set_state(gst::State::Null)
            .expect("Unable to set the pipeline to the `Null` state");
    }

    fn seek(&mut self, pos: u64) {
        if let Err(err) = self.playbin.seek_simple(
            gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
            gst::ClockTime::from_seconds(pos),
        ) {
            eprintln_raw!("failed to seek: {err}");
        }
    }

    fn seek_relative(&mut self, delta: i64) {
        if let Some(current) = self.query_position() {
            let current = current.seconds();
            let new = if delta < 0 {
                current.saturating_sub(delta.unsigned_abs())
            } else {
                current
                    .checked_add(delta.unsigned_abs())
                    .unwrap_or(u64::MAX)
            };
            self.seek(new);
        } else {
            eprintln_raw!("failed seek_relative");
        }
    }
    fn query_position(&self) -> Option<gst::ClockTime> {
        self.playbin.query_position::<gst::ClockTime>()
        // .expect("Could not query current position.")
    }
}


