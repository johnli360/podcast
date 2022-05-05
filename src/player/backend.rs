use std::{io::Write, time::Duration};
use tokio::{select, sync::mpsc::Sender, time};
use tokio_stream::StreamExt;

use super::{state::State, Cmd};

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
            if let Err(err) = player.state.to_disc() {
                eprintln_raw!("{err}");
            }
            return false;
        }
        Cmd::Next => {
            player.next();
        }
        Cmd::Prev => {
            player.prev();
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
            if err
                .src()
                .map(|src| src.path_string().to_string().contains("uridecodebin"))
                .unwrap_or(false)
            {
                player.current_uri = None;
            }
            println_raw!(
                "Error received from element {:?}: {} ({:?})",
                err.src().map(|s| s.path_string()),
                err.error(),
                err.debug()
            );
        }
        MessageView::Eos(..) => {
            println_raw!("End-Of-Stream reached.");
            if !player.next() {
                player.set_null();
            }
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
                            println_raw!("Seeking is ENABLED from {} to {}", start, end);
                            if let Some(pos) = player.pending_seek.take() {
                                println_raw!("seeking to pending: {pos}");
                                player.seek(pos);
                            }
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
    state: State,
    playbin: gst::Element,
    playing: bool,
    seek_enabled: bool,
    pending_seek: Option<u64>,
    duration: Option<gst::ClockTime>,
    current_uri: Option<String>,
}

impl Player {
    fn new() -> Self {
        let playbin = gst::ElementFactory::make("playbin", Some("playbin"))
            .expect("Failed to create playbin element");

        let state = State::from_disc().expect("failed to read state");
        state.print_queue();
        Player {
            state,
            pending_seek: None,
            playbin,
            playing: false,
            seek_enabled: false,
            duration: gst::ClockTime::NONE,
            current_uri: None,
        }
    }

    fn set_uri(&mut self, uri: &str) {
        self.current_uri = Some(uri.to_string());
        self.playbin.set_property("uri", uri);
    }

    fn queue(&mut self, uri: &str) {
        self.state.queue(uri);
        self.state.print_queue();
    }

    fn play(&mut self) {
        if self.playing {
            return;
        }

        if self.current_uri.is_none() {
            if let Some(new) = self.state.pop_queue() {
                self.set_uri(&new);
            } else {
                println_raw!("nothing to play");
                return;
            }
        }

        let curi = self.current_uri.take().unwrap();
        self.pending_seek = self.state.get_pos(&curi);
        self.current_uri.replace(curi);

        if let Err(err) = self.playbin.set_state(gst::State::Playing) {
            eprintln_raw!("Unable to set the playbin to the `Playing` state: {err}");
        }
    }

    fn report_playlist(&self) {
        self.state.print_recent();
        println_raw!("Current: {:?}", self.current_uri);
        self.state.print_queue();
    }

    fn next(&mut self) -> bool {
        if let Some(next) = self.state.pop_queue() {
            self.pause();
            self.set_null();
            if let Some(uri) = &self.current_uri {
                self.state.push_recent(uri);
            }
            self.duration = gst::ClockTime::NONE;
            self.set_uri(&next);
            if self.playing {
                self.play();
            }
            self.report_playlist();
            return true;
        }
        false
    }

    fn prev(&mut self) -> bool {
        if let Some(next) = self.state.pop_recent() {
            self.pause();
            self.set_null();
            if let Some(uri) = &self.current_uri {
                self.state.queue_front(uri);
            }
            self.duration = gst::ClockTime::NONE;
            self.set_uri(&next);
            if self.playing {
                self.play();
            }
            self.report_playlist();
            return true;
        }
        false
    }

    fn play_pause(&mut self) {
        if self.playing {
            self.pause();
        } else {
            self.play();
        };
    }

    fn update_state(&mut self) {
        if let Some(uri) = &self.current_uri {
            if let Some(pos) = self.query_position() {
                self.state.insert_playable(uri.to_string(), pos.seconds());
            }
        }
    }

    fn pause(&mut self) {
        self.playbin
            .set_state(gst::State::Paused)
            .expect("Unable to set the pipeline to the `Paused` state");
        self.update_state();
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
    }
}
