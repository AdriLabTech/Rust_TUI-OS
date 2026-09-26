//! The help app: the command catalogue and the keyboard shortcuts.

use crate::usr::apps::terminal::COMMANDS;
use crate::usr::apps::AppAction;

use alloc::format;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;

use pc_keyboard::DecodedKey;

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

/// A read-only view of the command list. It holds no state of its own.
pub struct HelpApp;

impl HelpApp {
    pub fn new() -> Self {
        HelpApp
    }

    /// Every key is inert here: the desktop owns the keys that leave the app.
    pub fn handle_key(&mut self, _key: DecodedKey) -> AppAction {
        AppAction::Keep
    }

    pub fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        let mut lines: Vec<Line<'static>> = Vec::new();
        for (name, desc) in COMMANDS {
            lines.push(Line::from(vec![
                Span::styled(
                    format!(" {:<10}", name),
                    Style::new().fg(Color::LightYellow),
                ),
                Span::raw("  "),
                Span::styled(desc.to_string(), Style::new().fg(Color::Gray)),
            ]));
        }
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            "F1-F4 abren una aplicacion, F5 o Esc vuelve al escritorio.".to_string(),
            Style::new().fg(Color::DarkGray),
        ));
        lines.push(Line::styled(
            "Tab completa nombres de comandos, las flechas recuperan el historial,".to_string(),
            Style::new().fg(Color::DarkGray),
        ));
        lines.push(Line::styled(
            "Ctrl+L limpia la pantalla, Ctrl+C cancela la linea de entrada.".to_string(),
            Style::new().fg(Color::DarkGray),
        ));
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(Color::DarkGray))
            .title(" Ayuda ")
            .title_alignment(Alignment::Center);
        let paragraph = Paragraph::new(Text::from(lines)).block(block);
        frame.render_widget(paragraph, area);
    }
}
