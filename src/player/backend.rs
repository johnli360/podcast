use std::{
    error::Error,
    time::Duration,
};
use tokio::{
    select,
    sync::mpsc::{Receiver, Sender},
    time,
};
use tokio_stream::StreamExt;
use tui::{backend::CrosstermBackend, Terminal};

use crate::{
    player::state::get_time,
    ui::{draw_ui, UiState, UiUpdate, _log},
};

use super::{
    state::{start_refresh_thread, Playable, RssFeed, State},
    Cmd,
};

pub async fn new(mut ui_rx: Receiver<UiUpdate>, ploop_tx: Sender<Cmd>) -> Sender<Cmd> {
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);
    let ui_cmd_tx = tx.clone();
    tokio::spawn(async move {
        let mut ui_state = UiState::new(ui_cmd_tx);
        let mut player = match Player::new() {
            Ok(player) => player,
            Err(err) => {
                logln!("failed to initialize player: {err}");
                return;
            }
        };
        start_refresh_thread(player.state.rss_feeds.clone(), ui_state.episodes.clone());
        let bus = player.playbin.bus().unwrap();
        let mut bus_stream = bus.stream();
        let mut ui_interval = time::interval(Duration::from_millis(100));

        let stdout = std::io::stdout();
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = match Terminal::new(backend) {
            Err(err) => {
                logln!("failed to initialize terminal: {err}");
                return;
            }
            Ok(t) => t,
        };

        let mut tick_count : u32 = 0;
        loop {
            select! {
            Some(ui_update) = ui_rx.recv() => {
                ui_state.update(ui_update, &mut player).await;
                draw_ui(&mut terminal, &mut player, &mut ui_state);
            }
            Some(cmd) = rx.recv() => {
                if let Cmd::Shutdown = cmd {
                    logln!("quitting");
                    ploop_tx.send(cmd).await.unwrap();
                    run_cmd(Cmd::Shutdown, &mut player).await;
                    return
                } else { run_cmd(cmd, &mut player).await};
            }
            msg = bus_stream.next() => {
                if let Some(msg) = msg {
                    handle_message(&mut player, &msg)
                }
            }
            _ = ui_interval.tick() => {
                if player.duration.is_none() {
                    player.duration = player.playbin.query_duration();
                }
                draw_ui(&mut terminal, &mut player, &mut ui_state);
                tick_count += 1;
                // 100 ms * 1200 = 120 seconds
                if tick_count >= 1200 {
                    tick_count = 0;
                    player.update_state();
                    if let Err(err) = player.state.to_disc() {
                        logln!("error while saving state: {err}");
                    }
                }

            }
            }
        }
    });
    tx
}

async fn run_cmd(cmd: Cmd, player: &mut Player) {
    match cmd {
        Cmd::Play => player.play(),
        Cmd::Pause => player.pause(),
        Cmd::PlayPause => player.play_pause(),
        Cmd::Queue(uri) => player.queue(&uri),
        Cmd::Seek(pos) => player.seek(pos),
        Cmd::SeekRelative(delta) => player.seek_relative(delta),

        Cmd::Subscribe(url) => {
            let x = RssFeed {
                uri: url,
                channel: None,
            };
            if let Ok(mut feeds) = player.state.rss_feeds.lock() {
                feeds.push(x);
            }
        }
        Cmd::Shutdown => {
            player.update_state();
            if let Some(uri) = &player.current_uri {
                player.state.queue_front(uri);
            }
            player.set_null();
            if let Err(err) = player.state.to_disc() {
                logln!("{err}");
            }
        }
        Cmd::Next => {
            player.update_state();
            player.next();
        }
        Cmd::Prev => {
            player.update_state();
            player.prev();
        }
        Cmd::DeleteQueue(index) => {
            let uri = player.state.queue.remove(index);
            log_delete(index, uri);
        }
        Cmd::DeleteRecent(index) => {
            let uri = player.state.recent.remove(index);
            log_delete(index, uri);
        }
        Cmd::Update(args) => {
            let uri = args.0;
            logln!(
                "received cmd to upadte {uri} to {} @ {}",
                args.1.progress.0,
                args.1.progress.1
            );
            player.state.update_playable(uri, args.1);
        }
    }
}

fn log_delete(index: usize, uri: Option<String>) {
    if let Some(uri) = uri {
        logln!("Deleting {index}: {uri}");
    } else {
        logln!("Deleting {index}: no such element");
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
            logln!(
                "Error received from element {:?}: {} ({:?})",
                err.src().map(|s| s.path_string()),
                err.error(),
                err.debug()
            );
        }
        MessageView::Eos(..) => {
            logln!("End-Of-Stream reached.");
            if let Some(uri) = &player.current_uri {
                logln!("finished {uri}");
                player.state.reset_pos(uri);
            }
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
                if new_state == gst::State::Paused {
                    player.update_state();
                };

                logln!("Pipeline state: {:?} -> {:?}", old_state, new_state);

                player.playing = new_state == gst::State::Playing;
                player.play_state = new_state;

                if player.playing {
                    let mut seeking = gst::query::Seeking::new(gst::Format::Time);
                    if player.playbin.query(&mut seeking) {
                        let (seekable, _start, _end) = seeking.result();
                        player.seek_enabled = seekable;
                        if seekable {
                            // ui.log_event(format!("Seeking is ENABLED from {} to {}", start, end));
                            if let Some(pos) = player.pending_seek.take() {
                                // println_raw!("seeking to pending: {pos}");
                                logln!("seeking to pending: {pos}");
                                player.seek(pos);
                            }
                        } else {
                            logln!("Seeking is DISABLED for this stream.");
                        }
                    } else {
                        logln!("Seeking query failed.")
                    }
                }
            }
        }

        MessageView::Tag(tag) => {
            if let Some(uri) = player.current_uri.as_ref() {
                if let Some(state) = player.state.uris.get_mut(uri) {
                    let tags = tag.tags();
                    if let Some(artist) = tags.get::<gst::tags::Artist>() {
                        logln!("  Artist: {}", artist.get());
                    }

                    if let Some(title) = tags.get::<gst::tags::Title>() {
                        logln!("  Title: {}", title.get());
                        if state.title.is_none() {
                            state.title = Some(title.get().to_string());
                        }
                    }

                    if let Some(album) = tags.get::<gst::tags::Album>() {
                        logln!("  Album: {}", album.get());
                        if state.album.is_none() {
                            state.album = Some(album.get().to_string());
                        }
                    }
                }
            }
        }
        _ => (),
    }
}

use gst::{prelude::*, ClockTime};
use gstreamer as gst;

pub struct Player {
    pub state: State,
    pub duration: Option<gst::ClockTime>,
    pub current_uri: Option<String>,
    playbin: gst::Element,
    playing: bool,
    pub play_state: gst::State,
    seek_enabled: bool,
    pending_seek: Option<u64>,
}

impl Player {
    fn new() -> Result<Self, Box<dyn Error>> {
        let playbin = gst::ElementFactory::make("playbin", Some("playbin"))?;
        let state = State::from_disc()?;

        Ok(Player {
            play_state: gst::State::Null,
            state,
            pending_seek: None,
            playbin,
            playing: false,
            seek_enabled: false,
            duration: gst::ClockTime::NONE,
            current_uri: None,
        })
    }

    fn set_uri(&mut self, uri: &str) {
        self.current_uri = Some(uri.to_string());
        self.playbin.set_property("uri", uri);
    }

    fn queue(&mut self, uri: &str) {
        self.state.queue(uri);
    }

    fn play(&mut self) {
        if self.playing {
            return;
        }

        if self.current_uri.is_none() {
            if let Some(new) = self.state.pop_queue() {
                self.set_uri(&new);
            } else {
                return;
            }
        }

        let curi = self.current_uri.as_ref().unwrap();
        self.pending_seek = self.state.get_pos(curi);
        if let Err(err) = self.playbin.set_state(gst::State::Playing) {
            logln!("Unable to set the playbin to the `Playing` state: {err}");
        }
    }

    fn next(&mut self) -> bool {
        if let Some(next) = self.state.pop_queue() {
            self.set_null();
            if let Some(uri) = &self.current_uri {
                self.state.push_recent(uri);
            }
            self.duration = gst::ClockTime::NONE;
            self.set_uri(&next);
            if self.playing {
                self.playing = false;
                self.play();
            }
            return true;
        }
        false
    }

    fn prev(&mut self) -> bool {
        if let Some(next) = self.state.pop_recent() {
            self.set_null();
            if let Some(uri) = &self.current_uri {
                self.state.queue_front(uri);
            }
            self.duration = gst::ClockTime::NONE;
            self.set_uri(&next);
            if self.playing {
                self.playing = false;
                self.play();
            }
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
            if let Some(pos) = self.query_position().map(ClockTime::seconds) {
                let pos = pos + self.pending_seek.unwrap_or(0);
                logln!("updating: {uri} to {pos}");
                let t = get_time();

                if let Some(playable) = self.state.uris.get_mut(uri) {
                    playable.progress = (t, pos);
                } else {
                    let playable = Playable {
                        title: None,
                        album: None,
                        progress: (t, pos),
                    };
                    self.state.insert_playable(uri.to_string(), playable);
                };
            }
        }
    }

    fn pause(&mut self) {
        if self.playing {
            if let Err(err) = self.playbin.set_state(gst::State::Paused) {
                logln!("Failed to set pipeline state to `Paused`: {err}");
            }
            self.update_state();
        }
    }

    fn set_null(&mut self) {
        if let Err(err) = self.playbin.set_state(gst::State::Null) {
            logln!("Failed to set pipeline state to `Null`: {err}");
        }
    }

    fn seek(&mut self, pos: u64) {
        if let Err(err) = self.playbin.seek_simple(
            gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
            gst::ClockTime::from_seconds(pos),
        ) {
            logln!("failed to seek: {err}");
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
            logln!("failed seek_relative (query_position())");
        }
    }

    pub fn query_position(&self) -> Option<gst::ClockTime> {
        self.playbin.query_position::<gst::ClockTime>()
    }
}
