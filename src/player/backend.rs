use std::{io::Stdout, time::Duration};
use tokio::{select, sync::mpsc::Sender, time};
use tokio_stream::StreamExt;
use tui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, List, ListItem, Paragraph, Tabs},
    Terminal,
};

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
            _ = interval.tick() => {
                if player.duration.is_none() {
                    player.duration = player.playbin.query_duration();
                }
                draw_ui(&mut player);
            }
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
            player.update_state();
            if let Some(uri) = &player.current_uri {
                player.state.queue_front(uri);
            }
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

pub fn draw_ui(player: &mut Player) {
    // let mut terminal = (*player).terminal;
    let position = player
        .query_position()
        .map(|time| time.to_string())
        .unwrap_or_else(|| "n\\a".to_string());

    let Player { terminal, .. } = player;
    let _ = terminal.draw(|f| {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints(
                [
                    Constraint::Length(2),
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Min(1),
                ]
                .as_ref(),
            )
            .split(f.size());

        let titles = vec!["tab1", "tab2"]
            .iter()
            .map(|t| {
                let (first, rest) = t.split_at(1);
                Spans::from(vec![
                    Span::styled(first, Style::default().fg(Color::Yellow)),
                    Span::styled(rest, Style::default().fg(Color::Green)),
                ])
            })
            .collect();
        let tabs = Tabs::new(titles)
            .block(Block::default().borders(Borders::BOTTOM))
            .select(0)
            .style(Style::default().fg(Color::Cyan))
            .highlight_style(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .bg(Color::Black),
            );
        f.render_widget(tabs, chunks[0]);

        let input = Paragraph::new("test".to_string())
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL).title("Input"));
        f.render_widget(input, chunks[1]);

        // .style(match app.input_mode {
        // InputMode::Normal => Style::default(),
        // InputMode::Editing => Style::default().fg(Color::Yellow),
        let text = format!("\r{position} / {}", player.duration.display());
        let progress = Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::White)),
        );
        // block.paragraph(p);
        f.render_widget(progress, chunks[2]);

        let messages: Vec<ListItem> = player
            .state
            .queue
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let content = vec![Spans::from(Span::raw(format!("{}: {}", i, m)))];
                ListItem::new(content)
            })
            .collect();
        let messages =
            List::new(messages).block(Block::default().borders(Borders::ALL).title("Messages"));
        f.render_widget(messages, chunks[3]);
    });
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
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl Player {
    fn new() -> Self {
        let playbin = gst::ElementFactory::make("playbin", Some("playbin"))
            .expect("Failed to create playbin element");

        let state = State::from_disc().expect("failed to read state");
        state.print_queue();

        let stdout = std::io::stdout();
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend).expect("hmm2");
        Player {
            state,
            pending_seek: None,
            playbin,
            playing: false,
            seek_enabled: false,
            duration: gst::ClockTime::NONE,
            current_uri: None,
            terminal,
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
                self.playing = false;
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
        if self.playing {
            self.playbin
                .set_state(gst::State::Paused)
                .expect("Unable to set the pipeline to the `Paused` state");
            self.update_state();
        }
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
