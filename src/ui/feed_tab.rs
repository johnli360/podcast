use super::interface::{last_n, UiState};
use crate::player::Player;
use ratatui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Span, Line},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

pub fn draw_feed_tab<B: Backend>(f: &mut Frame<B>, player: &Player, ui_state: &mut UiState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(0)
        .constraints(
            [
                Constraint::Length(2),
                Constraint::Min(5),
                Constraint::Length(1),
            ]
            .as_ref(),
        )
        .split(f.size());
    ui_state.vscroll = chunks[1].height.saturating_sub(2);
    let half_height = (chunks[1].height - 2) / 2;
    let first = ui_state.get_cursor_pos().saturating_sub(half_height.into());

    if let Ok(rss_feeds) = player.state.rss_feeds.lock() {
        let feeds: Vec<ListItem> = rss_feeds
            .iter()
            .enumerate()
            .skip(first)
            .take(chunks[1].height as usize)
            .map(|(i, feed)| {
                let channel_guard = feed.channel.read();
                let text = if let Ok(Some(channel)) = channel_guard.as_deref() {
                    &channel.title
                } else {
                    &feed.uri
                };

                let content = vec![Line::from(Span::raw(format!(
                    "{}: {}",
                    i,
                    last_n(text, chunks[1].width.saturating_sub(5))
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
        f.render_widget(feeds, chunks[1]);

        if let Some((prompt, _, _, _)) = &ui_state.file_prompt {
            let input = Paragraph::new(format!(": {prompt}"))
                .style(Style::default())
                .block(Block::default());
            f.render_widget(input, chunks[2]);
        }
    }
}
