use super::dir::children;
use std::{
    collections::VecDeque,
    fs::File,
    io::Stdout,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use chrono::DateTime;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use gstreamer::State;
use rss::Item;
use tokio::sync::mpsc::Sender;
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table, Tabs},
    Frame, Terminal,
};

use crate::player::{state::Playable, Cmd, Player};
const TAB_TITLES: &[&str] = &["Player", "Episodes", "Feeds", "Log"];

use std::io::Write;
pub static mut LOG: Mutex<Option<VecDeque<String>>> = Mutex::new(None);
pub fn _log(msg: &str) {
    let time = chrono::Local::now();
    let mut log_file = std::env::var("LOG_FILE")
        .ok()
        .and_then(|name| File::options().create(true).append(true).open(name).ok());
    unsafe {
        if let Ok(mut log) = LOG.lock() {
            let log = log.as_mut().expect("log uninitialised");
            let msg = format!("{time}: {}", msg);
            if let Some(Err(err)) = log_file.as_mut().map(|log_file| {
                log_file
                    .write(msg.as_bytes())
                    .and(log_file.write("\n".as_bytes()))
            }) {
                log.push_front(err.to_string());
            }
            log.push_front(msg);
            if log.len() == log.capacity() {
                log.pop_back();
            }
        }
    }
}

pub struct UiState {
    pub tab_index: usize,
    cursor_position: [usize; TAB_TITLES.len()],
    file_prompt: Option<(String, bool, Option<usize>, Vec<String>)>,
    prompt: Option<String>,
    hit_number: isize,
    pub episodes: Arc<Mutex<Vec<(String, Item)>>>,
    tx: Sender<Cmd>,
}
impl UiState {
    async fn send_cmd(&self, cmd: Cmd) {
        if let Err(err) = self.tx.send(cmd).await {
            logln!("{err}");
        }
    }

    pub fn new(tx: Sender<Cmd>) -> UiState {
        Self {
            tab_index: 0,
            hit_number: 0,
            cursor_position: [0; TAB_TITLES.len()],
            file_prompt: None,
            prompt: None,
            episodes: Arc::new(Mutex::new(Vec::new())),
            tx,
        }
    }

    fn search_ep(&mut self) {
        if let Ok(eps) = self.episodes.lock() {
            if let Some(prompt) = &self.prompt.as_ref().map(|p| p.to_lowercase()) {
                let mut hit_count = 0;
                let i = eps
                    .iter()
                    .enumerate()
                    .find(|(_i, (_url, item))| {
                        item.title()
                            .map(|title| {
                                let is_match = title.to_lowercase().contains(prompt);
                                if is_match {
                                    hit_count += 1;
                                }
                                is_match && hit_count > self.hit_number
                            })
                            .unwrap_or(false)
                    })
                    .map(|(i, _)| i);

                if let Some(i) = i {
                    self.cursor_position[self.tab_index] = i;
                }
            }
        }
    }

    pub fn get_cursor_pos(&self) -> usize {
        self.cursor_position[self.tab_index]
    }

    fn get_cursor_bound(&self, player: &Player) -> usize {
        match self.tab_index {
            0 => player.state.recent.len() + player.state.queue.len() - 1,
            // 1 => EPISODES (lots, no point in calculating max?).
            2 => {
                player
                    .state
                    .rss_feeds
                    .lock()
                    .map(|v| v.len())
                    .unwrap_or(usize::MAX)
                    - 1
            }
            // 3 => LOG
            _ => usize::MAX,
        }
    }

    fn search_update(&mut self, code: KeyCode) {
        if let Some(prompt) = self.prompt.as_mut() {
            match code {
                KeyCode::Char('#') => {
                    self.hit_number += 1;
                    self.search_ep();
                }
                KeyCode::Char('*') => {
                    self.hit_number -= 1;
                    self.search_ep();
                }
                KeyCode::Char(c) => {
                    prompt.push(c);
                    self.search_ep();
                }
                KeyCode::Backspace => {
                    prompt.pop();
                    self.search_ep();
                }
                KeyCode::Esc => {
                    self.prompt = None;
                    self.hit_number = 0;
                }
                _ => {}
            };
        }
    }

    pub async fn update(&mut self, event: UiUpdate, player: &mut Player) {
        match event {
            UiUpdate::KeyEvent(KeyEvent {
                code, modifiers, ..
            }) => {
                if self.file_prompt.is_some() {
                    match code {
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
                        _ => {}
                    }
                } else if self.prompt.is_some() {
                    self.search_update(code);
                } else {
                    use KeyCode::Char;
                    match code {
                        Char('q') => {
                            self.send_cmd(Cmd::Shutdown).await;
                        }
                        Char(' ') => {
                            self.send_cmd(Cmd::PlayPause).await;
                        }
                        Char('h') | Char('H') | KeyCode::Left => {
                            let cmd = if modifiers.intersects(KeyModifiers::SHIFT) {
                                Cmd::Prev
                            } else {
                                Cmd::SeekRelative(-10)
                            };
                            self.send_cmd(cmd).await;
                        }
                        Char('l') | Char('L') | KeyCode::Right => {
                            let cmd = if modifiers.intersects(KeyModifiers::SHIFT) {
                                Cmd::Next
                            } else {
                                Cmd::SeekRelative(10)
                            };
                            self.send_cmd(cmd).await
                        }

                        KeyCode::Char('o') => {
                            if self.tab_index == 0 || self.tab_index == 2 {
                                let init = if self.tab_index == 0 {
                                    String::from("/home/jl")
                                } else {
                                    String::new()
                                };

                                if self.file_prompt.is_none() {
                                    self.file_prompt = Some((init, true, None, Vec::new()));
                                } else {
                                    self.file_prompt = None;
                                }
                            } else if self.tab_index == 1 {
                                self.prompt = Some("".to_string());
                            }
                        }

                        KeyCode::Char('d') => {
                            if self.tab_index == 0 {
                                let cpos = self.get_cursor_pos();
                                let recent_size = player.state.recent.len();
                                let cmd = if cpos < recent_size {
                                    Cmd::DeleteRecent(recent_size - cpos - 1)
                                } else {
                                    Cmd::DeleteQueue(cpos - recent_size)
                                };
                                self.tx.send(cmd).await.expect("Failed to send delete");
                            }
                        }

                        KeyCode::Down | KeyCode::Char('j') => {
                            let pos = self.get_cursor_pos();
                            let bound = self.get_cursor_bound(player);
                            if pos < bound {
                                self.cursor_position[self.tab_index] = pos.saturating_add(1);
                            }
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            self.cursor_position[self.tab_index] =
                                self.cursor_position[self.tab_index].saturating_sub(1);
                        }

                        KeyCode::Enter => {
                            if let Some((s, _, _, _)) = self.file_prompt.take() {
                                if self.tab_index == 0 {
                                    let uri = if s.contains("://") {
                                        s
                                    } else {
                                        let mut uri = String::from("file://");
                                        uri.push_str(&s);
                                        uri
                                    };
                                    if let Err(err) = self.tx.send(Cmd::Queue(uri)).await {
                                        logln!("{err}");
                                    }
                                } else if self.tab_index == 2 {
                                    if let Err(err) = self.tx.send(Cmd::Subscribe(s)).await {
                                        logln!("Subscribe error: {err}");
                                    }
                                }
                            } else if self.tab_index == 1 {
                                let url = if let Ok(eps) = self.episodes.lock() {
                                    eps.get(self.get_cursor_pos())
                                        .and_then(|(chan_title, item)| {
                                            if let Some(url) = item.enclosure().map(|enc| &enc.url)
                                            {
                                                let playable = Playable {
                                                    title: item.title.clone(),
                                                    album: Some(chan_title.clone()),
                                                    progress: 0,
                                                };
                                                player.state.insert_playable(url.clone(), playable);
                                                Some(url.clone())
                                            } else {
                                                None
                                            }
                                        })
                                } else {
                                    None
                                };

                                if let Err(err) = self.tx.send(Cmd::Queue(url.unwrap())).await {
                                    logln!("{err}");
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
                            } else {
                                let new_index = (self.tab_index + 1) % TAB_TITLES.len();
                                self.tab_index = new_index;
                            }
                        }

                        _ => {}
                    }
                };
            }
        }
    }
}

#[derive(Debug)]
pub enum UiUpdate {
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
            1 => draw_episodes_tab(f, ui_state),
            2 => draw_feed_tab(f, player, ui_state),
            3 => draw_event_log_tab(f),
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
                Constraint::Length(5),
                Constraint::Length(3),
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

fn draw_episodes_tab<B: Backend>(f: &mut Frame<B>, ui_state: &mut UiState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(0)
        .constraints(
            [
                Constraint::Length(2),
                // Constraint::Length(3),
                Constraint::Min(5),
                Constraint::Length(1),
            ]
            .as_ref(),
        )
        .split(f.size());

    let tbl_height = chunks[1].height;
    let half_height = (tbl_height - 2) / 2;
    let first = ui_state.get_cursor_pos().saturating_sub(half_height.into());

    if let Ok(episodes) = ui_state.episodes.lock() {
        let episodes: Vec<Row> = episodes
            .iter()
            .enumerate()
            .skip(first)
            .take(tbl_height.into())
            .map(|(i, (chan_title, item))| {
                let asd = String::from("n/a");
                let title = item.title.as_ref().unwrap_or(&asd);
                let x = item
                    .pub_date()
                    .map(DateTime::parse_from_rfc2822)
                    .and_then(Result::ok)
                    .map(|dt| dt.date().naive_utc().to_string());

                let item = Row::new(vec![
                    Cell::from(i.to_string()),
                    Cell::from(x.unwrap_or("".to_string())),
                    Cell::from(chan_title.to_string()),
                    Cell::from(title.to_string()),
                ]);
                if ui_state.get_cursor_pos() == i {
                    item.style(Style::default().fg(Color::Black).bg(Color::White))
                } else {
                    item
                }
            })
            .collect();
        let constraints = [
            Constraint::Length(3),
            Constraint::Length(10),
            Constraint::Length(18),
            Constraint::Length(chunks[2].width),
        ];
        let tbl = Table::new(episodes)
            .block(Block::default().borders(Borders::ALL).title("Episodes"))
            .header(
                Row::new(vec!["i", "Date", "Podcast Title", "Episode Title"])
                    .style(Style::default().fg(Color::Yellow)), // .bottom_margin(1),
            )
            .widths(&constraints)
            .column_spacing(1);

        f.render_widget(tbl, chunks[1]);

        if let Some(search) = &ui_state.prompt {
            let input = Paragraph::new(format!(": {}", search.as_str())).style(Style::default());
            f.render_widget(input, chunks[2]);
        };
        // .block(Block::default().borders(Borders::ALL).title("Search"));
    }
}

fn draw_feed_tab<B: Backend>(f: &mut Frame<B>, player: &Player, ui_state: &mut UiState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(0)
        .constraints(
            [
                Constraint::Length(2),
                Constraint::Length(3),
                Constraint::Min(5),
            ]
            .as_ref(),
        )
        .split(f.size());
    let input = Paragraph::new("...")
        .style(Style::default())
        .block(Block::default().borders(Borders::ALL).title("Search"));
    f.render_widget(input, chunks[1]);

    let half_height = (chunks[2].height - 2) / 2;
    let first = ui_state.get_cursor_pos().saturating_sub(half_height.into());

    if let Ok(rss_feeds) = player.state.rss_feeds.lock() {
        let feeds: Vec<ListItem> = rss_feeds
            .iter()
            .enumerate()
            .skip(first)
            .take(chunks[2].height as usize)
            .map(|(i, m)| {
                let text = if let Some(x) = &m.channel {
                    &x.title
                } else {
                    &m.uri
                };
                let content = vec![Spans::from(Span::raw(format!(
                    "{}: {}",
                    i,
                    last_n(text, chunks[2].width.saturating_sub(5))
                )))];
                let item = ListItem::new(content);
                if ui_state.get_cursor_pos() == i {
                    item.style(Style::default().fg(Color::Black).bg(Color::White))
                } else {
                    item
                }
            })
            .collect();
        let feeds = List::new(feeds).block(Block::default().borders(Borders::ALL).title("Feeds"));
        f.render_widget(feeds, chunks[2]);
    }
}

const RECENT_SIZE: usize = 3;
fn draw_recents<B: Backend>(f: &mut Frame<B>, chunk: Rect, ui_state: &UiState, player: &Player) {
    let recent_len = player.state.recent.len();
    let to_skip = recent_len
        .saturating_sub(RECENT_SIZE)
        .saturating_sub(ui_state.get_cursor_pos());
    let recent: Vec<Row> = player
        .state
        .recent
        .iter()
        .enumerate()
        .skip(to_skip)
        .take(RECENT_SIZE)
        .map(|(i, uri)| {
            let name = if let Some(name) = player.state.uris.get(uri).and_then(|p| p.title.as_ref())
            {
                name
            } else {
                uri
            };

            let chan_title = player
                .state
                .uris
                .get(uri)
                .and_then(|p| p.album.as_ref())
                .map(|s| s.as_str())
                .unwrap_or("");

            let item = Row::new(vec![
                Cell::from(i.to_string()),
                Cell::from(chan_title.to_string()),
                Cell::from(name.to_string()),
            ]);
            if ui_state.get_cursor_pos() == recent_len - 1 - i {
                item.style(Style::default().fg(Color::Black).bg(Color::White))
            } else {
                item
            }
        })
        .rev() //TODO: not perfect, List of rows instead?
        .collect();
    let constraints = [
        Constraint::Length(3),
        Constraint::Length(18),
        Constraint::Length(chunk.width),
    ];

    let recent = Table::new(recent)
        .block(Block::default().borders(Borders::ALL).title("Recent"))
        .widths(&constraints)
        .column_spacing(1);

    f.render_widget(recent, chunk);
}

fn draw_file_prompt<B: Backend>(f: &mut Frame<B>, chunk: Rect, ui_state: &mut UiState) {
    if let Some((ref current_input, dirty, cmpl_ind, ref mut cmpl)) = ui_state.file_prompt.as_mut()
    {
        let chunks = Layout::default()
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(chunk);

        let file_prompt = if let Some(index) = cmpl_ind {
            (*cmpl).get(*index).unwrap_or(current_input)
        } else {
            current_input
        };
        let input = Paragraph::new(&file_prompt[..])
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
        let cmpl = List::new(cmpl).block(Block::default().borders(Borders::ALL));
        f.render_widget(cmpl, chunks[1]);
    }
}

fn draw_playlist<B: Backend>(f: &mut Frame<B>, chunk: Rect, ui_state: &UiState, player: &Player) {
    //                - 2 for border
    // let uri_map = player.state.uris;
    let half_height = (chunk.height - 2) / 2;
    let first = ui_state.get_cursor_pos().saturating_sub(half_height.into());
    let playlist: Vec<Row> = player
        .state
        .queue
        .iter()
        .enumerate()
        .skip(first)
        .map(|(i, uri)| {
            let name = if let Some(name) = player.state.uris.get(uri).and_then(|p| p.title.as_ref())
            {
                name
            } else {
                uri
            };

            let chan_title = player
                .state
                .uris
                .get(uri)
                .and_then(|p| p.album.as_ref())
                .map(|s| s.as_str())
                .unwrap_or("");

            let r_len = player.state.recent.len();
            let item = Row::new(vec![
                Cell::from(i.to_string()),
                Cell::from(chan_title.to_string()),
                Cell::from(name.to_string()),
            ]);
            if ui_state.get_cursor_pos() >= r_len && i == ui_state.get_cursor_pos() - r_len {
                item.style(Style::default().fg(Color::Black).bg(Color::White))
            } else {
                item
            }
        })
        .collect();
    let constraints = [
        Constraint::Length(3),
        Constraint::Length(18),
        Constraint::Length(chunk.width),
    ];

    let playlist = Table::new(playlist)
        .block(Block::default().borders(Borders::ALL).title("Episodes"))
        .header(
            Row::new(vec!["i", "Channel Title", "Title"]).style(Style::default().fg(Color::Yellow)), // .bottom_margin(1),
        )
        .widths(&constraints)
        .column_spacing(1);

    // let playlist =
    // List::new(playlist).block(Block::default().borders(Borders::ALL).title("Playlist"));
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

    let name = if let Some(uri) = &player.current_uri {
        if let Some(name) = player
            .state
            .uris
            .get(uri)
            .and_then(|playable| playable.title.as_ref())
        {
            name
        } else {
            last_n(uri, space)
        }
    } else {
        ""
    };

    let text = format!("{name} {position} / {duration}");
    let progress = Paragraph::new(text).block(
        Block::default()
            .title(state_to_str(player.play_state))
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::White)),
    );
    f.render_widget(progress, chunk);
}

const fn state_to_str(state: State) -> &'static str {
    match state {
        State::VoidPending => "Void",
        State::Null => "Null",
        State::Ready => "Ready",
        State::Paused => "Paused",
        State::Playing => "Playing",
        _ => "Unknown",
    }
}

fn draw_event_log_tab<B: Backend>(f: &mut Frame<B>) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(0)
        .constraints([Constraint::Length(2), Constraint::Min(2)].as_ref())
        .split(f.size());

    if let Ok(log) = unsafe { LOG.lock() } {
        let log = log.as_ref().expect("");
        let events: Vec<ListItem> = log
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
}
