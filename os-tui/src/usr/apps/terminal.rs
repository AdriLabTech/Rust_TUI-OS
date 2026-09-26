//! The terminal app: a command line over a scrolling output log.

use crate::api;
use crate::sys;
use crate::usr::apps::{AppAction, AppKind};
use crate::usr::util::{format_uptime, version};

use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;

use pc_keyboard::{DecodedKey, KeyCode};

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};
use ratatui::Frame;

/// Maximum number of characters in the command input line.
const MAX_INPUT: usize = 78;

/// Maximum number of lines kept in the on-screen log.
const MAX_LOG: usize = 256;

/// The built-in commands, with a one-line description for the help screen.
///
/// Task 5 rebuilds this catalogue, so the descriptions are still the old
/// English ones here; only the removed `widgets` entry is gone.
pub static COMMANDS: &[(&str, &str)] = &[
    ("help", "Show this help screen"),
    ("sysinfo", "Show live system information"),
    ("echo", "Print the given arguments"),
    ("clear", "Clear the command log"),
    ("date", "Show the current date and time"),
    ("uptime", "Show how long the system has been running"),
    ("version", "Show the OS version"),
    ("mem", "Show memory usage"),
    ("halt", "Shut down the system (ACPI)"),
    ("reboot", "Reboot the system"),
    ("quit", "Shut down the system (alias of halt)"),
];

/// One line of the terminal output log.
struct LogEntry {
    text: String,
    prompt: bool,
}

/// The terminal's state, mutated by key presses and redrawn on each frame.
pub struct TerminalApp {
    input: String,
    cursor: usize,
    history: Vec<String>,
    history_pos: Option<usize>,
    log: Vec<LogEntry>,
}

impl TerminalApp {
    pub fn new() -> Self {
        let mut app = TerminalApp {
            input: String::new(),
            cursor: 0,
            history: Vec::new(),
            history_pos: None,
            log: Vec::new(),
        };
        app.push_log(
            format!("TUI-OS v{} · 100% Rust: kernel + bootloader + shell", version()),
            false,
        );
        app.push_log(
            "El escritorio se dibuja con ratatui 0.30 en el búfer de texto VGA 80x25".to_string(),
            false,
        );
        app.push_log(
            "Sin espacio de usuario: todo vive en el crate del kernel".to_string(),
            false,
        );
        app.push_log(
            "Escriba 'help' (o pulse F1) para listar los comandos disponibles.".to_string(),
            false,
        );
        app
    }

    fn push_log(&mut self, text: String, prompt: bool) {
        if self.log.len() >= MAX_LOG {
            self.log.remove(0);
        }
        self.log.push(LogEntry { text, prompt });
    }

    // ------------------------------------------------------------------
    // Key handling
    // ------------------------------------------------------------------

    /// Handle one decoded key.
    ///
    /// The F-keys are not handled here: the desktop owns them, so that they
    /// mean the same thing no matter which app is open.
    pub fn handle_key(&mut self, key: DecodedKey) -> AppAction {
        match key {
            DecodedKey::Unicode(c) => match c {
                '\n' => return self.submit(),
                '\t' => self.tab(),
                '\x08' => self.backspace(),
                '\x7f' => self.delete(),
                '\x03' => self.cancel_input(), // Ctrl+C
                '\x0c' => self.clear_screen(),  // Ctrl+L
                '\x1b' => {}                    // Escape
                c if c.is_control() => {}
                c => self.insert_char(c),
            },
            DecodedKey::RawKey(code) => match code {
                KeyCode::ArrowUp => self.history_prev(),
                KeyCode::ArrowDown => self.history_next(),
                KeyCode::ArrowLeft => self.left(),
                KeyCode::ArrowRight => self.right(),
                KeyCode::Home => self.cursor = 0,
                KeyCode::End => self.cursor = self.input.chars().count(),
                _ => {}
            },
        }
        AppAction::Keep
    }

    fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    fn right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.input.chars().count());
    }

    fn tab(&mut self) {
        self.complete();
    }

    fn set_input(&mut self, s: String) {
        self.input = s;
        self.cursor = self.input.chars().count();
    }

    fn insert_char(&mut self, c: char) {
        let mut chars: Vec<char> = self.input.chars().collect();
        if chars.len() >= MAX_INPUT {
            return;
        }
        self.cursor = self.cursor.min(chars.len());
        chars.insert(self.cursor, c);
        self.input = chars.into_iter().collect();
        self.cursor += 1;
    }

    fn backspace(&mut self) {
        let mut chars: Vec<char> = self.input.chars().collect();
        self.cursor = self.cursor.min(chars.len());
        if self.cursor == 0 {
            return;
        }
        chars.remove(self.cursor - 1);
        self.input = chars.into_iter().collect();
        self.cursor -= 1;
    }

    fn delete(&mut self) {
        let mut chars: Vec<char> = self.input.chars().collect();
        self.cursor = self.cursor.min(chars.len());
        if self.cursor >= chars.len() {
            return;
        }
        chars.remove(self.cursor);
        self.input = chars.into_iter().collect();
    }

    fn cancel_input(&mut self) {
        self.input.clear();
        self.cursor = 0;
    }

    fn clear_screen(&mut self) {
        self.log.clear();
    }

    fn complete(&mut self) {
        if self.input.is_empty() || self.input.contains(' ') {
            return;
        }
        let word = self.input.clone();
        let matches: Vec<&'static str> = COMMANDS
            .iter()
            .map(|(n, _)| *n)
            .filter(|n| n.starts_with(&word))
            .collect();
        match matches.len() {
            0 => {}
            1 => self.set_input(matches[0].to_string()),
            _ => self.push_log(matches.join("  "), false),
        }
    }

    fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let pos = match self.history_pos {
            Some(p) if p > 0 => p - 1,
            _ => self.history.len() - 1,
        };
        self.history_pos = Some(pos);
        self.set_input(self.history[pos].clone());
    }

    fn history_next(&mut self) {
        match self.history_pos {
            Some(p) if p + 1 < self.history.len() => {
                self.history_pos = Some(p + 1);
                self.set_input(self.history[p + 1].clone());
            }
            _ => {
                self.history_pos = None;
                self.input.clear();
                self.cursor = 0;
            }
        }
    }

    fn submit(&mut self) -> AppAction {
        let cmd = self.input.trim().to_string();
        let mut action = AppAction::Keep;
        if !cmd.is_empty() {
            self.history.push(cmd.clone());
            self.history_pos = None;
            self.push_log(format!("> {}", cmd), true);
            let (lines, act) = self.exec(&cmd);
            for line in lines {
                self.push_log(line, false);
            }
            action = act;
        }
        self.input.clear();
        self.cursor = 0;
        action
    }

    /// Run one command line, returning its output and the app switch it asks
    /// for. `Switch` is how `help` and `sysinfo` reach the screen they name.
    fn exec(&mut self, cmdline: &str) -> (Vec<String>, AppAction) {
        let mut out = Vec::new();
        let mut parts = cmdline.split_whitespace();
        let cmd = match parts.next() {
            Some(c) => c,
            None => return (out, AppAction::Keep),
        };
        let args: Vec<&str> = parts.collect();
        let mut action = AppAction::Keep;
        match cmd {
            "help" => {
                out.push("Abriendo la ayuda, pulse F5 para volver.".to_string());
                action = AppAction::Switch(AppKind::Help);
            }
            "sysinfo" => {
                out.push(
                    "Abriendo la informacion del sistema, pulse F5 para volver.".to_string(),
                );
                action = AppAction::Switch(AppKind::SysInfo);
            }
            "clear" => {
                self.log.clear();
            }
            "echo" => out.push(args.join(" ")),
            "date" => out.push(sys::clk::date()),
            "uptime" => {
                out.push(format!("Tiempo activo: {}", format_uptime(sys::clk::boot_time())))
            }
            "version" => out.push(version()),
            "mem" => {
                let unit = api::unit::SizeUnit::Binary;
                out.push(format!(
                    "RAM total:    {}",
                    unit.format(sys::mem::memory_size())
                ));
                out.push(format!(
                    "RAM en uso:   {}",
                    unit.format(sys::mem::memory_used())
                ));
                out.push(format!(
                    "RAM libre:    {}",
                    unit.format(sys::mem::memory_free())
                ));
            }
            "halt" | "quit" => {
                out.push("Apagando el equipo…".to_string());
                sys::acpi::shutdown();
            }
            "reboot" => {
                sys::idt::reset(); // Never returns
            }
            _ => out.push(format!("comando no encontrado: '{}' · escriba 'help'", cmd)),
        }
        (out, action)
    }

    // ------------------------------------------------------------------
    // Rendering
    // ------------------------------------------------------------------

    /// Draw the log and the input line into the area the desktop hands us.
    pub fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(3)]).split(area);
        self.render_log(frame, chunks[0]);
        self.render_input(frame, chunks[1]);
    }

    fn render_log(&self, frame: &mut Frame<'_>, area: Rect) {
        // Bottom-anchored, the way a real terminal grows: blank space on top,
        // newest line at the bottom.
        let n = area.height as usize;
        let taken = self.log.len().min(n);
        let mut lines: Vec<Line<'static>> = vec![Line::raw(""); n - taken];
        for entry in &self.log[self.log.len() - taken..] {
            let style = if entry.prompt {
                Style::new().fg(Color::LightGreen)
            } else {
                Style::new().fg(Color::Gray)
            };
            lines.push(Line::styled(entry.text.clone(), style));
        }
        let paragraph = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false });
        frame.render_widget(paragraph, area);
    }

    fn render_input(&self, frame: &mut Frame<'_>, area: Rect) {
        let block = Block::new()
            .borders(Borders::TOP | Borders::BOTTOM)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(Color::DarkGray));
        let inner = block.inner(area);

        let max_w = (inner.width as usize).saturating_sub(2);
        let chars: Vec<char> = self.input.chars().collect();
        let (start, shown): (usize, String) = if chars.len() > max_w {
            let start = if self.cursor < max_w {
                0
            } else {
                self.cursor - max_w
            };
            let end = (start + max_w).min(chars.len());
            (start, chars[start..end].iter().collect())
        } else {
            (0, self.input.clone())
        };

        let line = Line::from(vec![
            Span::styled(
                "> ",
                Style::new()
                    .fg(Color::LightGreen)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(shown.clone(), Style::new().fg(Color::LightCyan)),
        ]);
        frame.render_widget(Paragraph::new(line).block(block), area);

        // Keep the hardware cursor on the input line.
        let cx = inner.x + 2 + (self.cursor.saturating_sub(start)) as u16;
        frame.set_cursor_position((cx, inner.y));
    }
}
