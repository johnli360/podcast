use std::io::Stdout;

use gstreamer::prelude::Displayable;
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Style, Modifier},
    text::{Span, Spans},
    widgets::{Block, Borders, List, ListItem, Paragraph, Tabs},
    Frame, Terminal,
};

use crate::player::Player;
const TAB_TITLES: &[&str] = &["Player", "tab2"];

pub struct UiState {
    pub tab_index: usize,
}
impl UiState {
    pub fn update(&mut self, event: UiUpdate) {
        match event {
            UiUpdate::Tab => {
                let new_index = (self.tab_index + 1) % TAB_TITLES.len();
                self.tab_index = new_index;
            }
        }
    }
}

pub enum UiUpdate {
    Tab,
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
            0 => draw_tab1(f, player, ui_state),
            _ => (),
        }
    });
}

fn draw_tab1<B: Backend>(f: &mut Frame<B>, player: &Player, _ui_state: &UiState) {
    let position = player
        .query_position()
        .map(|time| time.to_string())
        .unwrap_or_else(|| "n\\a".to_string());
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
}

// use crate::player::backend::Player;
// pub fn draw_ui(terminal: &mut Terminal<CrosstermBackend<Stdout>>, player: &Player) {
// terminal.clear().expect("hmm clear failed");
// if let Some(position) = player.query_position() {
// print_raw!("\r{position} / {}", player.duration.display());
// draw_ui(&mut player.terminal, player);
// }
// else {
// eprintln_raw!("Could not query current position.")
// }

/*     let _ = terminal.draw(|f| { */
/* let chunks = Layout::default() */
/* .direction(Direction::Vertical) */
/* .margin(0) */
/* .constraints( */
/* [ */
/* Constraint::Percentage(10), */
/* Constraint::Percentage(80), */
/* Constraint::Percentage(10), */
/* ] */
/* .as_ref(), */
/* ) */
/* .split(f.size()); */
// let block = Block::default().title("Block").borders(Borders::ALL);

// let position = player.query_position().unwrap_or("n\\a");
// let text = format!("\r{position} / {}", player.duration.display());
// let p = Paragraph::new(text).block(
// Block::default()
// .borders(Borders::ALL)
// .style(Style::default().fg(Color::White)),
// );
// block.paragraph(p);
// f.render_widget(p, chunks[0]);
// f.render_widget(p, chunks[0]);
// });
// }
