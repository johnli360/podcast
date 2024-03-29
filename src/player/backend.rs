use crate::logln;
use std::{
    error::Error,
    sync::{Arc, RwLock},
    time::Duration,
};
use tokio::{
    select,
    sync::mpsc::{Receiver, Sender},
    time,
};
use tokio_stream::StreamExt;
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::{
    player::state::get_time,
    ui::interface::{draw_ui, UiState, UiUpdate},
};

use super::{
    state::{start_refresh_thread, Playable, RssFeed, State},
    Cmd,
};

async fn start_observation(state: &State, feed_tx: Sender<Arc<RssFeed>>) {
    let mut feeds = Vec::new();
    if let Ok(rss_feeds) = state.rss_feeds.lock() {
        for feed in rss_feeds.iter() {
            feeds.push(feed.clone());
        }
    }

    for feed in feeds.iter() {
        if let Err(err) = feed_tx.send(feed.clone()).await {
            logln!("failed to send feed: {err}");
        }
    }
}

pub async fn new(mut ui_rx: Receiver<UiUpdate>, ploop_tx: Sender<Cmd>) -> Sender<Cmd> {
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);
    let ui_cmd_tx = tx.clone();
    tokio::spawn(async move {
        let mut ui_state = UiState::new(ui_cmd_tx);
        let feed_tx = start_refresh_thread(ui_state.episodes.clone());
        let mut player = match Player::new(feed_tx.clone()) {
            Ok(player) => player,
            Err(err) => {
                logln!("failed to initialize player: {err}");
                return;
            }
        };
        start_observation(&player.state, feed_tx.clone()).await;

        let mut bus_stream = player.playbin.bus().unwrap().stream();
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

        let mut tick_count: u32 = 0;
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
                    if !handle_message(&mut player, &msg) {
                        logln!("reseting playbin");
                        let mut playbin = gst::ElementFactory::make("playbin", Some("playbin"))
                            .expect("failed to initalise playbin");
                        let mut new_bus_stream = playbin.bus().unwrap().stream();
                        std::mem::swap(&mut player.playbin, &mut playbin);
                        std::mem::swap(&mut bus_stream, &mut new_bus_stream);
                        if let Some(uri) = player.current_uri.take() {
                            player.set_uri(&uri);
                        }
                        if let Err(err) = player.playbin.set_state(gst::State::Ready) {
                            logln!("failed to ready new playbing: {err}");
                        }
                    }
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
                    if player.playing {
                        player.update_state();
                        if let Err(err) = player.state.to_disc() {
                            logln!("error while saving state: {err}");
                        }
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
            logln!("cmd to subscribe to {url}");
            let new_feed = Arc::new(RssFeed {
                uri: url.clone(),
                channel: Arc::new(RwLock::new(None)),
            });
            if let Err(err) = player.feed_tx.send(new_feed.clone()).await {
                logln!("failed send new feed: {err}");
            }
            if let Ok(mut feeds) = player.state.rss_feeds.lock() {
                // TODO: better data structure for feeds?
                if !feeds.iter().any(|x| x.uri == url) {
                    feeds.push(new_feed);
                }
            }
            if let Err(err) = player.state.to_disc() {
                logln!("{err}");
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
            if let Some(uri) = uri.as_ref() {
                player.state.push_recent(uri);
            }
            log_delete(index, uri);
        }
        Cmd::DeleteRecent(index) => {
            let uri = player.state.recent.remove(index);
            log_delete(index, uri);
        }
        Cmd::Update(args) => {
            let uri = args.0;
            logln!(
                "cmd to update {uri} to {} @ {}",
                args.1.progress.unwrap_or_default(),
                args.1.length.unwrap_or_default(),
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

fn handle_message(player: &mut Player, msg: &gst::Message) -> bool {
    use gst::MessageView;

    match msg.view() {
        MessageView::Error(err) => {
            logln!(
                "Error received from element {:?}: {} ({:?})",
                err.src().map(|s| s.path_string()),
                err.error(),
                err.debug()
            );

            let err_str = err
                .src()
                .map(|src| src.path_string().to_string())
                .unwrap_or_default();
            if err_str.contains("uridecodebin") {
                player.current_uri = None;
            }

            if err.error().to_string().contains("Connection terminated") {
                logln!("pulse sink crashed :(");
                player.set_null();
                player.playing = false;

                return false;
            }
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
                            if let Some(pos) = player.pending_seek.take() {
                                let (hours, minutes, seconds) =
                                    clktime_to_hms(gst::ClockTime::from_seconds(pos));
                                logln!(
                                    "seeking to pending: {}:{:02}:{:02}",
                                    hours,
                                    minutes,
                                    seconds
                                );
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
    true
}

use gst::prelude::*;
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
    feed_tx: Sender<Arc<RssFeed>>,
}

impl Player {
    fn new(feed_tx: Sender<Arc<RssFeed>>) -> Result<Self, Box<dyn Error>> {
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
            feed_tx,
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
                // self.state.push_recent(uri);
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
            if let Some(pos) = self.query_position() {
                {
                    let (hours, minutes, seconds) = clktime_to_hms(pos);
                    logln!("{}:{:02}:{:02} ~ {uri}", hours, minutes, seconds);
                }

                let seconds = pos.seconds();
                let t = get_time();
                if let Some(playable) = self.state.uris.get_mut(uri) {
                    playable.progress = Some(seconds);
                    playable.updated = Some(t);
                    playable.length = self.duration.map(gst::ClockTime::seconds);
                } else {
                    let playable = Playable {
                        title: None,
                        album: None,
                        source: None,
                        progress: Some(seconds),
                        length: self.duration.map(gst::ClockTime::seconds),
                        updated: Some(t),
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
            } else {
                self.update_state();
            }
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

const fn clktime_to_hms(time: gst::ClockTime) -> (u64, u64, u64) {
    let seconds = time.seconds();
    let minutes = time.minutes();
    let hours = minutes / 60;
    (hours, minutes % 60, seconds % 60)
}
