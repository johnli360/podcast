// use podaemon::dir::children;
use chrono::DateTime;
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
    Frame,
};

use crate::player::{state::Playable, Player};

use super::interface::UiState;

pub fn draw_episodes_tab<B: Backend>(f: &mut Frame<B>, player: &Player, ui_state: &mut UiState) {
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
    //                                 2 for border, 1 for header
    ui_state.vscroll = tbl_height.saturating_sub(2 + 1);
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
                let pod_title = item.title.as_ref().unwrap_or(&asd);
                let date = item
                    .pub_date()
                    .map(DateTime::parse_from_rfc2822)
                    .and_then(Result::ok)
                    .map(|dt| dt.date_naive().to_string());

                let progress = item
                    .enclosure()
                    .and_then(|e| player.state.uris.get(&e.url))
                    .map(Playable::progress_string);

                let item = Row::new(vec![
                    // Cell::from(i.to_string()),
                    Cell::from(date.unwrap_or_default()),
                    Cell::from(progress.unwrap_or_default()),
                    Cell::from(chan_title.to_string()),
                    Cell::from(pod_title.to_string()),
                ]);
                if ui_state.get_cursor_pos() == i {
                    item.style(Style::default().fg(Color::Black).bg(Color::White))
                } else {
                    item
                }
            })
            .collect();
        let constraints = [
            // Constraint::Length(3),
            Constraint::Length(10),
            Constraint::Length(5),
            Constraint::Length(18),
            Constraint::Length(chunks[2].width),
        ];
        let tbl = Table::new(episodes)
            .block(Block::default().borders(Borders::ALL).title("Episodes"))
            .header(
                // Row::new(vec!["i", " State ", "Date", "Podcast", "Episode"])
                Row::new(vec!["Date", "State", "Podcast", "Episode"])
                    .style(Style::default().fg(Color::Yellow)), // .bottom_margin(1),
            )
            .widths(&constraints)
            .column_spacing(1);

        f.render_widget(tbl, chunks[1]);

        if let Some(search) = &ui_state.prompt {
            let input = Paragraph::new(format!(": {}", search.as_str())).style(Style::default());
            f.render_widget(input, chunks[2]);
        };
    }
}
