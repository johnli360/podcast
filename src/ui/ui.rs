// use podaemon::dir::children;
use std::{
    cmp,
    io::Stdout,
    mem,
    sync::{Arc, Mutex},
};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rss::Item;
use tokio::sync::mpsc::Sender;
use tui::{
    backend::CrosstermBackend,
    layout::{Constraint, Layout},
    style::{Color, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, Tabs},
    Terminal,
};

use crate::player::{
    state::{get_time, Playable},
    Cmd, Player,
};

use super::{
    episodes_tab::draw_episodes_tab,
    feed_tab::draw_feed_tab,
    log::{self, draw_event_log_tab},
    player_tab::draw_player_tab,
};
const TAB_TITLES: &[&str] = &["Player", "Episodes", "Feeds", "Log"];

pub struct UiState {
    pub tab_index: usize,
    cursor_position: [usize; TAB_TITLES.len()],
    pub file_prompt: Option<(String, bool, Option<usize>, Vec<String>)>,
    pub prompt: Option<String>,
    hit_number: isize,
    pub vscroll: u16,
    key_hist: Vec<KeyEvent>,
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
            vscroll: 0,
            key_hist: Vec::new(),
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
        let bound = match self.tab_index {
            0 => player.state.recent.len() + player.state.queue.len(),
            1 => {
                if let Ok(eps) = self.episodes.lock() {
                    eps.len()
                } else {
                    usize::MAX
                }
            }
            2 => player
                .state
                .rss_feeds
                .lock()
                .map(|v| v.len())
                .unwrap_or(usize::MAX),
            3 => log::get_cursor_bound(),
            _ => usize::MAX,
        };
        bound - 1
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

    async fn file_prompt_update(&mut self, code: KeyCode) {
        if let Some((ref mut s, ref mut dirty, ref mut index, ref mut cmp)) = self.file_prompt {
            match code {
                KeyCode::Char(c) => {
                    if let Some(i) = index {
                        if let Some(cmp_alt) = cmp.get_mut(*i) {
                            std::mem::swap(s, cmp_alt);
                        }
                        *index = None;
                    }
                    s.push(c);
                    *dirty = true;
                }
                KeyCode::Esc => {
                    self.file_prompt = None;
                }
                KeyCode::Backspace => {
                    if let Some(i) = index {
                        if let Some(cmp_alt) = cmp.get_mut(*i) {
                            std::mem::swap(s, cmp_alt);
                        }
                        *index = None;
                    }

                    s.pop();
                    *dirty = true;
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

                KeyCode::Enter => {
                    if self.tab_index == 0 {
                        if let Some(i) = index {
                            if let Some(cmp_alt) = cmp.get_mut(*i) {
                                std::mem::swap(s, cmp_alt);
                            };
                        };

                        if !s.contains("://") {
                            let mut uri = String::from("file://");
                            uri.push_str(s);
                            mem::swap(s, &mut uri);
                        };
                        if let Err(err) = self.tx.send(Cmd::Queue(mem::take(s))).await {
                            logln!("{err}");
                        }
                        self.file_prompt = None;
                    } else if self.tab_index == 2 {
                        if let Err(err) = self
                            .tx
                            .send(Cmd::Subscribe(mem::replace(s, "".to_string())))
                            .await
                        {
                            logln!("Subscribe error: {err}");
                        }
                    }
                }

                _ => {}
            }
        }
    }

    pub async fn update(&mut self, event: UiUpdate, player: &mut Player) {
        match event {
            UiUpdate::KeyEvent(
                event @ KeyEvent {
                    code, modifiers, ..
                },
            ) => {
                if self.file_prompt.is_some() {
                    self.file_prompt_update(code).await;
                } else if self.prompt.is_some() {
                    self.search_update(code);
                } else {
                    use KeyCode::Char;
                    match code {
                        Char('G') => {
                            let bound = self.get_cursor_bound(player);
                            self.cursor_position[self.tab_index] = bound;
                        }
                        Char('g') => {
                            if let Some(KeyEvent { code, .. }) = self.key_hist.last() {
                                if let KeyCode::Char('g') = code {
                                    self.cursor_position[self.tab_index] = 0;
                                    self.key_hist.clear();
                                }
                            } else {
                                self.key_hist.push(event);
                            }
                        }
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

                        KeyCode::Char('/') => {
                            if self.tab_index == 1 {
                                self.prompt = Some("".to_string());
                            }
                        }

                        KeyCode::Char('d') => {
                            if KeyModifiers::CONTROL == modifiers {
                                let new = cmp::min(
                                    self.get_cursor_bound(player),
                                    self.vscroll as usize + self.get_cursor_pos(),
                                );
                                self.cursor_position[self.tab_index] = new;
                            } else if self.tab_index == 0 {
                                let cpos = self.get_cursor_pos();
                                let recent_size = player.state.recent.len();
                                let cmd = if cpos < recent_size {
                                    Cmd::DeleteRecent(recent_size - cpos - 1)
                                } else {
                                    Cmd::DeleteQueue(cpos - recent_size)
                                };
                                if let Err(err) = self.tx.send(cmd).await {
                                    logln!("Failed to send delete: {err}");
                                }
                            }
                        }

                        Char('u') => {
                            if KeyModifiers::CONTROL == modifiers {
                                self.cursor_position[self.tab_index] =
                                    self.get_cursor_pos().saturating_sub(self.vscroll as usize);
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

                        KeyCode::Backspace => {
                            if let Some((ref mut s, ref mut dirty, _, _)) = self.file_prompt {
                                s.pop();
                                *dirty = true;
                            }
                        }
                        KeyCode::Tab => {
                            let new_index = (self.tab_index + 1) % TAB_TITLES.len();
                            self.tab_index = new_index;
                        }

                        KeyCode::Enter => {
                            if self.tab_index == 1 {
                                let info = self.episodes.lock().ok().and_then(|eps| {
                                    eps.get(self.get_cursor_pos())
                                        .and_then(|(chan_title, item)| {
                                            let x = item.enclosure().map(|enclosure| {
                                                (
                                                    chan_title.clone(),
                                                    item.title().map(str::to_string),
                                                    enclosure.url.clone(),
                                                    item.source.clone().map(|s| s.url),
                                                )
                                            });
                                            x
                                        })
                                });

                                if let Some((chan_title, title, url, source)) = info {
                                    let url2 = url.clone();
                                    let pos = player.state.uris.get(&url2);
                                    let playable = Playable {
                                        title,
                                        album: Some(chan_title),
                                        progress: pos
                                            .map(|x| x.progress)
                                            .unwrap_or((get_time(), 0)),
                                        source,
                                    };
                                    player.state.insert_playable(url, playable);

                                    if let Err(err) = self.tx.send(Cmd::Queue(url2)).await {
                                        logln!("failed to queue: {err}");
                                    }
                                };
                            };
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
            3 => draw_event_log_tab(f, ui_state),
            _ => (),
        }
    });
}

pub fn last_n(s: &str, n: impl Into<usize>) -> &str {
    let n = n.into();
    if n >= s.len() {
        s
    } else {
        &s[s.len() - n..]
    }
}
