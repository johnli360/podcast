use std::{collections::VecDeque, io::Stdout, path::PathBuf};

use crossterm::event::{KeyCode, KeyEvent};
use gstreamer::State;
use tokio::sync::mpsc::Sender;
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Corner, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, List, ListItem, Paragraph, Tabs},
    Frame, Terminal,
};

use crate::{
    dir::children,
    player::{Cmd, Player},
};
const TAB_TITLES: &[&str] = &["Player", "Log"];

pub struct UiState {
    pub tab_index: usize,
    cursor_position: usize,
    log: VecDeque<String>,
    file_prompt: Option<(String, bool, Option<usize>, Vec<String>)>,
    tx: Sender<Cmd>,
}
impl UiState {
    pub fn new(tx: Sender<Cmd>) -> UiState {
        Self {
            tab_index: 0,
            cursor_position: 0,
            log: VecDeque::new(),
            file_prompt: None,
            tx,
        }
    }
    pub async fn update(&mut self, event: UiUpdate) {
        match event {
            UiUpdate::Tab => {
                let new_index = (self.tab_index + 1) % TAB_TITLES.len();
                self.tab_index = new_index;
            }
            UiUpdate::Log(msg) => {
                self.log_event(msg);
            }
            UiUpdate::BrowseFile => {
                if self.file_prompt.is_none() {
                    self.file_prompt = Some((String::from("/home/jl"), true, None, Vec::new()));
                } else {
                    self.file_prompt = None;
                }
            }
            UiUpdate::KeyEvent(KeyEvent { code, .. }) => match code {
                KeyCode::Char('d') => {
                    let cmd = if self.cursor_position < RECENT_SIZE {
                        Cmd::DeleteRecent(self.cursor_position)
                    } else {
                        Cmd::DeleteQueue(self.cursor_position - RECENT_SIZE)
                    };
                    self.tx.send(cmd).await.expect("Failed to send delete");
                }

                KeyCode::Down | KeyCode::Char('j') => {
                    //TODO: don't increment out of bounds of playlist
                    self.cursor_position = self.cursor_position.saturating_add(1);
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.cursor_position = self.cursor_position.saturating_sub(1);
                }
                KeyCode::Char(c) => {
                    if let Some((ref mut s, ref mut dirty, ref mut index, ref mut cmp)) =
                        self.file_prompt
                    {
                        if let Some(i) = index {
                            if let Some(cmp_alt) = cmp.get_mut(*i) {
                                std::mem::swap(s, cmp_alt);
                            }
                            *index = None;
                        }
                        s.push(c);
                        *dirty = true;
                    }
                }
                KeyCode::Esc => {
                    self.file_prompt = None;
                }
                KeyCode::Enter => {
                    if let Some((s, _, _, _)) = self.file_prompt.take() {
                        let uri = if s.contains("://") {
                            s
                        } else {
                            let mut uri = String::from("file://");
                            uri.push_str(&s);
                            uri
                        };
                        if let Err(err) = self.tx.send(Cmd::Queue(uri)).await {
                            self.log_event(format!("{err}"));
                        }
                    }
                }
                KeyCode::Backspace => {
                    if let Some((ref mut s, ref mut dirty, _, _)) = self.file_prompt {
                        s.pop();
                        *dirty = true;
                    }
                }
                KeyCode::Tab => {
                    if let Some((_, _, ref mut index, cmpl)) = &mut self.file_prompt {
                        if let Some(ref mut index_inner) = index {
                            *index_inner += 1;
                            if *index_inner >= cmpl.len() {
                                *index = None;
                            }
                        } else {
                            *index = Some(0);
                        }
                    }
                }
                _ => {}
            },
        }
    }

    pub fn log_event(&mut self, msg: String) {
        self.log.push_front(msg);
        if self.log.len() > 40 {
            self.log.pop_back();
        }
    }
}

pub enum UiUpdate {
    Tab,
    Log(String),
    BrowseFile,
    KeyEvent(KeyEvent),
}

pub fn draw_ui(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    player: &mut Player,
    ui_state: &mut UiState,
) {
    let _ = terminal.draw(|f| {
        let chunks = Layout::default()
            .margin(0)
            .constraints([Constraint::Length(2), Constraint::Min(1)].as_ref())
            .split(f.size());
        let titles = TAB_TITLES
            .iter()
            .map(|t| Spans::from(vec![Span::styled(*t, Style::default().fg(Color::White))]))
            .collect();
        let tabs = Tabs::new(titles)
            .block(Block::default().borders(Borders::BOTTOM))
            .select(ui_state.tab_index)
            .style(Style::default().fg(Color::White))
            .highlight_style(Style::default().fg(Color::Yellow));
        f.render_widget(tabs, chunks[0]);

        match ui_state.tab_index {
            0 => draw_player_tab(f, player, ui_state),
            1 => draw_event_log_tab(f, ui_state),
            _ => (),
        }
    });
}

fn last_n(s: &str, n: impl Into<usize>) -> &str {
    let n = n.into();
    if n >= s.len() {
        s
    } else {
        &s[s.len() - n..]
    }
}

fn draw_player_tab<B: Backend>(f: &mut Frame<B>, player: &Player, ui_state: &mut UiState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(0)
        .constraints(
            [
                Constraint::Length(2),
                // Constraint::Length(3),
                Constraint::Length(5),
                Constraint::Length(3),
                // Constraint::Min(1),
                Constraint::Min(1),
            ]
            .as_ref(),
        )
        .split(f.size());

    draw_recents(f, chunks[1], ui_state, player);

    draw_current_info(f, chunks[2], player);

    if let Some(_prompt) = &ui_state.file_prompt {
        draw_file_prompt(f, chunks[3], ui_state);
    } else {
        draw_playlist(f, chunks[3], ui_state, player);
    }
}

const RECENT_SIZE: usize = 3;
fn draw_recents<B: Backend>(f: &mut Frame<B>, chunk: Rect, ui_state: &UiState, player: &Player) {
    let recent: Vec<ListItem> = player
        .state
        .recent
        .iter()
        .take(RECENT_SIZE)
        .enumerate()
        .map(|(i, m)| {
            let content = vec![Spans::from(Span::raw(format!(
                "{}: {}",
                i,
                last_n(m, chunk.width.saturating_sub(5))
            )))];
            let item = ListItem::new(content);
            if ui_state.cursor_position == RECENT_SIZE - i - 1 {
                item.style(Style::default()
                    .fg(Color::Black)
                    .bg(Color::White))
            } else {
                item
            }
        })
        .collect();
    let recent = List::new(recent)
        .block(Block::default().borders(Borders::ALL).title("Recent"))
        .start_corner(Corner::BottomLeft);
    f.render_widget(recent, chunk);
}

fn draw_file_prompt<B: Backend>(f: &mut Frame<B>, chunk: Rect, ui_state: &mut UiState) {
    if let Some((current_input, dirty, cmpl_ind, ref mut cmpl)) = ui_state.file_prompt.as_mut() {
        let chunks = Layout::default()
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(chunk);

        let prompt = if let Some(index) = cmpl_ind {
            (*cmpl).get(*index).unwrap_or(current_input)
        } else {
            &current_input
        };
        let input = Paragraph::new(&prompt[..])
            .style(Style::default())
            .block(Block::default().borders(Borders::ALL).title("Input"));
        f.render_widget(input, chunks[0]);

        let path = PathBuf::from(&current_input);

        if *dirty {
            *dirty = false;
            *cmpl = children(path);
        }

        let cmpl: Vec<ListItem> = cmpl
            .iter()
            .map(|m| {
                let content = vec![Spans::from(Span::raw(m))];
                ListItem::new(content)
            })
            .collect();
        let cmpl = List::new(cmpl).block(Block::default().borders(Borders::ALL)); //.title("Log"));
        f.render_widget(cmpl, chunks[1]);
    }
}

fn draw_playlist<B: Backend>(f: &mut Frame<B>, chunk: Rect, ui_state: &UiState, player: &Player) {
    let playlist: Vec<ListItem> = player
        .state
        .queue
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let content = vec![Spans::from(Span::raw(format!(
                "{}: {}",
                i,
                last_n(m, chunk.width.saturating_sub(5))
            )))];
            let item = ListItem::new(content);
            if ui_state.cursor_position >= RECENT_SIZE
                && i == ui_state.cursor_position - RECENT_SIZE
            {
                item.style(Style::default()
                    .fg(Color::Black)
                    .bg(Color::White))
            } else {
                item
            }
        })
        .collect();
    let playlist =
        List::new(playlist).block(Block::default().borders(Borders::ALL).title("Playlist"));
    f.render_widget(playlist, chunk);
}

fn draw_current_info<B: Backend>(f: &mut Frame<B>, chunk: Rect, player: &Player) {
    let position = player
        .query_position()
        .map(|time| format!("{:.0}", time))
        .unwrap_or_else(|| "n\\a".to_string());

    let duration = player
        .duration
        .map(|time| format!("{:.0}", time))
        .unwrap_or_else(|| "n\\a".to_string());

    let p_length = position.len() + duration.len() + 6;
    let space = if p_length >= chunk.width as usize {
        0
    } else {
        chunk.width as usize - p_length
    };
    let uri = if let Some(uri) = &player.current_uri {
        last_n(uri, space)
    } else {
        ""
    };

    let text = format!("{uri} {position} / {duration}");
    let progress = Paragraph::new(text).block(
        Block::default()
            .title(state_to_str(player.play_state))
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::White)),
    );
    f.render_widget(progress, chunk);
}

const fn state_to_str(state: State) -> &'static str {
    return match state {
        State::VoidPending => "Void",
        State::Null => "Null",
        State::Ready => "Ready",
        State::Paused => "Paused",
        State::Playing => "Playing",
        _ => "Unknown",
    }
}

fn draw_event_log_tab<B: Backend>(f: &mut Frame<B>, ui_state: &UiState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(0)
        .constraints([Constraint::Length(2), Constraint::Min(2)].as_ref())
        .split(f.size());

    let events: Vec<ListItem> = ui_state
        .log
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let content = vec![Spans::from(Span::raw(format!("{}: {}", i, m)))];
            ListItem::new(content)
        })
        .collect();
    let events = List::new(events).block(Block::default().borders(Borders::ALL).title("Log"));
    f.render_widget(events, chunks[1]);
}
