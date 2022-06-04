use std::{collections::VecDeque, io::Stdout};

use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, List, ListItem, Paragraph, Tabs},
    Frame, Terminal,
};

use crate::player::Player;
const TAB_TITLES: &[&str] = &["Player", "Log"];

pub struct UiState {
    pub tab_index: usize,
    log: VecDeque<String>,
}
impl UiState {
    pub fn new() -> UiState {
        Self {
            tab_index: 0,
            log: VecDeque::new(),
        }
    }
    pub fn update(&mut self, event: UiUpdate) {
        match event {
            UiUpdate::Tab => {
                let new_index = (self.tab_index + 1) % TAB_TITLES.len();
                self.tab_index = new_index;
            }
            UiUpdate::Log(msg) => {
                self.log_event(msg);
            }
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
}

pub fn draw_ui(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    player: &mut Player,
    ui_state: &UiState,
) {
    let _ = terminal.draw(|f| {
        let chunks = Layout::default()
            .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
            .split(f.size());
        let titles = TAB_TITLES
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
            .select(ui_state.tab_index)
            .style(Style::default().fg(Color::Cyan))
            .highlight_style(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .bg(Color::Black),
            );
        f.render_widget(tabs, chunks[0]);

        match ui_state.tab_index {
            0 => draw_player_tab(f, player, ui_state),
            1 => draw_event_log_tab(f, ui_state),
            _ => (),
        }
    });
}

fn draw_player_tab<B: Backend>(f: &mut Frame<B>, player: &Player, _ui_state: &UiState) {
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

    let input = Paragraph::new("test".to_string())
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::ALL).title("Input"));
    f.render_widget(input, chunks[1]);

    draw_current_info(f, chunks[2], player);

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
        List::new(messages).block(Block::default().borders(Borders::ALL).title("Playlist"));
    f.render_widget(messages, chunks[3]);
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

    let p_length = position.len() + duration.len() + 5;
    let space = if p_length >= chunk.width as usize {
        0
    } else {
        chunk.width as usize - p_length
    };
    let uri = if let Some(uri) = &player.current_uri {
        if uri.len() > space {
            &uri[uri.len() - space..]
        } else {
            uri
        }
    } else {
        ""
    };

    let text = format!("{uri} {position} / {duration}");
    let progress = Paragraph::new(text).block(
        Block::default()
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::White)),
    );
    f.render_widget(progress, chunk);
}

fn draw_event_log_tab<B: Backend>(f: &mut Frame<B>, ui_state: &UiState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(3)
        .constraints([Constraint::Min(2)].as_ref())
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
    f.render_widget(events, chunks[0]);
}
