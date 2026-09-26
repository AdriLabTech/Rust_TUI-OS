//! The files app.
//!
//! A scaffold: Task 6 replaces the body with the full port. It already
//! honours the one key the desktop does not own — `q` closes the app from
//! normal mode, because Esc is intercepted by the desktop first.

use crate::usr::apps::AppAction;

use alloc::string::ToString;
use alloc::vec;

use pc_keyboard::DecodedKey;

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

pub struct FilesApp;

impl FilesApp {
    pub fn new() -> Self {
        FilesApp
    }

    pub fn handle_key(&mut self, key: DecodedKey) -> AppAction {
        match key {
            DecodedKey::Unicode('q') | DecodedKey::Unicode('Q') => AppAction::Close,
            _ => AppAction::Keep,
        }
    }

    pub fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        let lines = vec![
            Line::raw(""),
            Line::styled(
                "Archivos: (puerto pendiente)".to_string(),
                Style::new().fg(Color::LightYellow),
            ),
            Line::raw(""),
            Line::styled(
                "q o Esc para volver al escritorio".to_string(),
                Style::new().fg(Color::DarkGray),
            ),
        ];
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(Color::DarkGray))
            .title(" Archivos ")
            .title_alignment(Alignment::Center);
        let paragraph = Paragraph::new(Text::from(lines)).block(block);
        frame.render_widget(paragraph, area);
    }
}
