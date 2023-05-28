use std::path::PathBuf;

use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table},
    Frame,
};

use crate::{dir::children, player::Player};

use super::interface::{last_n, UiState};
use gstreamer::State;

pub fn draw_player_tab<B: Backend>(f: &mut Frame<B>, player: &Player, ui_state: &mut UiState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(0)
        .constraints(
            [
                Constraint::Length(2),
                Constraint::Length((RECENT_SIZE + 2) as u16),
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

const RECENT_SIZE: usize = 10;
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
        Constraint::Length(RECENT_SIZE as u16),
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

fn draw_playlist<B: Backend>(
    f: &mut Frame<B>,
    chunk: Rect,
    ui_state: &mut UiState,
    player: &Player,
) {
    //                                   2 for border, 1 for header
    ui_state.vscroll = chunk.height.saturating_sub(2 + 1);
    let half_height = chunk.height.saturating_sub(2) / 2;
    let first = ui_state.get_cursor_pos().saturating_sub(half_height.into());
    let to_skip = if first < (half_height * 2) as usize {
        0
    } else {
        first
    };
    let playlist: Vec<Row> = player
        .state
        .queue
        .iter()
        .enumerate()
        .skip(to_skip)
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
            Row::new(vec!["i", "Channel Title", "Title"]).style(Style::default().fg(Color::Yellow)),
        )
        .widths(&constraints)
        .column_spacing(1);

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
