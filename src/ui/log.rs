use std::{collections::VecDeque, fmt, fs::File, io::Write, sync::Mutex};

use once_cell::sync::Lazy;
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

use super::interface::UiState;

pub static LOG: Lazy<Mutex<VecDeque<String>>> = Lazy::new(|| {
    let msgs = VecDeque::with_capacity(200);
    Mutex::new(msgs)
});

pub fn _log(msg: fmt::Arguments) {
    let time = chrono::Local::now();
    let mut log_file = std::env::var("LOG_FILE")
        .ok()
        .and_then(|name| File::options().create(true).append(true).open(name).ok());
    if let Ok(mut log) = LOG.lock() {
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

pub fn draw_event_log_tab<B: Backend>(f: &mut Frame<B>, ui_state: &mut UiState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(0)
        .constraints([Constraint::Length(2), Constraint::Min(2)].as_ref())
        .split(f.size());
    ui_state.vscroll = chunks[1].height.saturating_sub(2);

    if let Ok(log) = LOG.lock() {
        let offset = (chunks[1].height - 2).saturating_sub(log.len() as u16);
        let half_height = ((chunks[1].height - 2) / 2) as usize;
        let skip = ui_state
            .get_cursor_pos()
            .saturating_sub(half_height)
            .saturating_sub(offset.into());
        let events: Vec<ListItem> = log
            .iter()
            .enumerate()
            .skip(skip)
            .map(|(i, m)| {
                let content = vec![Spans::from(Span::raw(format!("{}: {}", i, m)))];
                let item = ListItem::new(content);
                if ui_state.get_cursor_pos() == i {
                    item.style(Style::default().fg(Color::Black).bg(Color::White))
                } else {
                    item
                }
            })
            .collect();
        let events = List::new(events).block(Block::default().borders(Borders::ALL).title("Log"));
        f.render_widget(events, chunks[1]);
    }
}

pub fn get_cursor_bound() -> usize {
    if let Ok(guard) = LOG.lock() {
        guard.len()
    } else {
        usize::MAX
    }
}
