//! The desktop: wallpaper, dock, and the fullscreen app switcher.
//!
//! The desktop owns the screen. It draws the title bar, the hint bar and the
//! status bar, decides whether a key belongs to the dock or to the open app,
//! and runs exactly one fullscreen app at a time.

use crate::api;
use crate::sys;
use crate::usr::apps::files::FilesApp;
use crate::usr::apps::help::HelpApp;
use crate::usr::apps::sysinfo::SysInfoApp;
use crate::usr::apps::terminal::TerminalApp;
use crate::usr::apps::{AppAction, AppKind, APPS};
use crate::usr::tui::VgaBackend;
use crate::usr::util::{format_uptime, version};

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use pc_keyboard::{DecodedKey, KeyCode};

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::{Frame, Terminal};

/// The dock index of the shutdown entry: one past the last app.
const SHUTDOWN_INDEX: usize = APPS.len();

/// How many entries the dock has, apps plus shutdown.
const DOCK_LEN: usize = APPS.len() + 1;

/// What the desktop should do with a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopCmd {
    /// Nothing: the key belongs to the open app, or nobody.
    None,
    /// Launch the app at the highlighted dock index.
    Open(AppKind),
    /// Shut the machine down.
    Halt,
    /// Close the open app and return to the desktop.
    Close,
    /// Bring another app to the front.
    Switch(AppKind),
}

/// Undo [`as_route_code`], so a key the desktop did not claim reaches the app
/// in the shape the app expects: a `KeyCode` stays a `KeyCode`, and the Enter
/// and Escape control characters become the characters the terminal reads.
fn decode_from(code: KeyCode, key: DecodedKey) -> DecodedKey {
    match (code, key) {
        (KeyCode::Return, _) => DecodedKey::Unicode('\n'),
        (KeyCode::Escape, _) => DecodedKey::Unicode('\x1b'),
        (_, other) => other,
    }
}

/// Map a decoded key onto the `KeyCode` the routing table speaks, or `None`
/// when the key is not one of the desktop's and belongs to the open app.
///
/// This exists because the keyboard driver never hands us `KeyCode::Return` or
/// `KeyCode::Escape`: every layout table decodes those two as the control
/// characters `'\n'` and 0x1B. Translating them here keeps [`route_key`] a
/// pure function over `KeyCode` and keeps the driver quirk in the I/O layer,
/// where it belongs.
fn as_route_code(key: DecodedKey) -> Option<KeyCode> {
    match key {
        DecodedKey::RawKey(code) => Some(code),
        DecodedKey::Unicode('\r' | '\n') => Some(KeyCode::Return),
        DecodedKey::Unicode('\x1b') => Some(KeyCode::Escape),
        DecodedKey::Unicode(_) => None,
    }
}

/// Decide what a key means, given whether an app is open and which dock entry
/// is highlighted. Returns the command and the new dock index.
///
/// Pure and total, so the routing rules can be tested without a machine: the
/// desktop loop does the I/O, this does the thinking.
///
/// `app` is `None` when the desktop itself has focus. That distinction is the
/// whole point of the function — while an app is open the arrow keys belong to
/// that app, and only the desktop focused state moves the dock.
fn route_key(
    app: Option<AppKind>,
    dock_index: usize,
    key: KeyCode,
) -> (DesktopCmd, usize) {
    // The F-keys mean the same thing from anywhere: they name an app.
    let switched = match key {
        KeyCode::F1 => Some(AppKind::Help),
        KeyCode::F2 => Some(AppKind::SysInfo),
        KeyCode::F3 => Some(AppKind::Terminal),
        KeyCode::F4 => Some(AppKind::Files),
        _ => None,
    };
    if let Some(kind) = switched {
        return (DesktopCmd::Switch(kind), dock_index);
    }

    match app {
        // Desktop focused: the arrows walk the dock, Enter launches.
        None => match key {
            KeyCode::ArrowRight => (DesktopCmd::None, (dock_index + 1) % DOCK_LEN),
            KeyCode::ArrowLeft => (DesktopCmd::None, (dock_index + DOCK_LEN - 1) % DOCK_LEN),
            KeyCode::Return => {
                if dock_index == SHUTDOWN_INDEX {
                    (DesktopCmd::Halt, dock_index)
                } else {
                    (DesktopCmd::Open(APPS[dock_index].0), dock_index)
                }
            }
            _ => (DesktopCmd::None, dock_index),
        },
        // An app is open: Esc and F5 go back to the desktop, and everything
        // else is the app's business.
        Some(_) => match key {
            KeyCode::Escape | KeyCode::F5 => (DesktopCmd::Close, dock_index),
            _ => (DesktopCmd::None, dock_index),
        },
    }
}

/// The desktop: the open app, the dock selection, and one instance of each app.
pub struct Desktop {
    app: Option<AppKind>,
    dock_index: usize,
    terminal: TerminalApp,
    files: FilesApp,
    sysinfo: SysInfoApp,
    help: HelpApp,
}

impl Desktop {
    pub fn new() -> Self {
        Desktop {
            // Boot into the terminal, so the machine comes up on something
            // useful rather than on an empty desktop.
            app: Some(AppKind::Terminal),
            dock_index: AppKind::Terminal.index(),
            terminal: TerminalApp::new(),
            files: FilesApp::new(),
            sysinfo: SysInfoApp::new(),
            help: HelpApp::new(),
        }
    }

    // ------------------------------------------------------------------
    // Key handling
    // ------------------------------------------------------------------

    /// Route one decoded key, then act on the result.
    fn handle_key(&mut self, key: DecodedKey) {
        let Some(code) = as_route_code(key) else {
            // A printable character is never a desktop shortcut.
            self.send_to_app(key);
            return;
        };

        let (cmd, next) = route_key(self.app, self.dock_index, code);
        self.dock_index = next;

        match cmd {
            // Nobody claimed it, so give the app its chance. Rebuild the
            // original `DecodedKey`: a character that reached `route_key` as a
            // control code has to go back to the app in the form the app reads.
            DesktopCmd::None => self.send_to_app(decode_from(code, key)),
            DesktopCmd::Open(kind) => {
                self.app = Some(kind);
                self.dock_index = kind.index();
            }
            DesktopCmd::Switch(kind) => {
                self.app = Some(kind);
                self.dock_index = kind.index();
            }
            DesktopCmd::Close => {
                self.app = None;
            }
            DesktopCmd::Halt => {
                sys::acpi::shutdown();
            }
        }
    }

    /// Offer a key to the open app and honour what it wants in return.
    fn send_to_app(&mut self, key: DecodedKey) {
        let Some(kind) = self.app else {
            return;
        };
        let action = match kind {
            AppKind::Terminal => self.terminal.handle_key(key),
            AppKind::Files => self.files.handle_key(key),
            AppKind::SysInfo => self.sysinfo.handle_key(key),
            AppKind::Help => self.help.handle_key(key),
        };
        match action {
            AppAction::Keep => {}
            AppAction::Close => self.app = None,
            AppAction::Switch(next) => {
                self.app = Some(next);
                self.dock_index = next.index();
            }
        }
    }

    // ------------------------------------------------------------------
    // Main loop
    // ------------------------------------------------------------------

    fn run(&mut self, terminal: &mut Terminal<VgaBackend>) {
        let mut last_draw: usize = 0;
        terminal.clear().ok();
        loop {
            let mut redraw = false;
            while let Some(key) = sys::keyboard::try_pop_decoded_key() {
                self.handle_key(key);
                redraw = true;
            }
            let ticks = sys::clk::ticks();
            if redraw || ticks.wrapping_sub(last_draw) >= 50 {
                terminal.draw(|frame| self.render(frame)).ok();
                last_draw = ticks;
            }
            sys::x86::hlt();
        }
    }

    // ------------------------------------------------------------------
    // Rendering
    // ------------------------------------------------------------------

    fn render(&mut self, frame: &mut Frame<'_>) {
        let area = frame.area();
        let chunks = Layout::vertical([
            Constraint::Length(1), // title bar
            Constraint::Min(1),    // the open app, or the wallpaper
            Constraint::Length(1), // hint bar
            Constraint::Length(1), // status bar
        ])
        .split(area);

        self.render_title(frame, chunks[0]);
        match self.app {
            Some(AppKind::Terminal) => self.terminal.render(frame, chunks[1]),
            Some(AppKind::Files) => self.files.render(frame, chunks[1]),
            Some(AppKind::SysInfo) => self.sysinfo.render(frame, chunks[1]),
            Some(AppKind::Help) => self.help.render(frame, chunks[1]),
            None => render_wallpaper(frame, chunks[1]),
        }
        self.render_hint(frame, chunks[2]);
        self.render_status(frame, chunks[3]);
    }

    fn render_title(&self, frame: &mut Frame<'_>, area: Rect) {
        let left = format!(" TUI-OS v{} ", version());
        let right = format!(" {} ", sys::clk::date());
        let width = area.width as usize;
        let gap = width.saturating_sub(left.len() + right.len());
        let line = Line::from(vec![
            Span::styled(
                left,
                Style::new()
                    .fg(Color::White)
                    .bg(Color::Blue)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" ".repeat(gap)),
            Span::styled(right, Style::new().fg(Color::White).bg(Color::Blue)),
        ]);
        frame.render_widget(Paragraph::new(line).style(Style::new().bg(Color::Blue)), area);
    }

    fn render_hint(&self, frame: &mut Frame<'_>, area: Rect) {
        // Only glyphs that `tui::char_to_cp437` actually maps may appear here:
        // it has the triangles but not ← → ↑ ↓, and an unmapped glyph is drawn
        // as a blank cell, which silently eats the arrow out of the hint.
        let text: String = match self.app {
            None => {
                let what = if self.dock_index == SHUTDOWN_INDEX {
                    "Apagar el equipo"
                } else {
                    APPS[self.dock_index].2
                };
                format!(" ◀/▶ elegir   Enter abrir   {}", what)
            }
            Some(_) => {
                " F1 Ayuda  F2 Sistema  F3 Terminal  F4 Archivos  F5/Esc Escritorio".to_string()
            }
        };
        let paragraph = Paragraph::new(Line::styled(text, Style::new().fg(Color::DarkGray)));
        frame.render_widget(paragraph, area);
    }

    fn render_status(&self, frame: &mut Frame<'_>, area: Rect) {
        let unit = api::unit::SizeUnit::Binary;
        let left = format!(
            " MEM {} / {} ",
            unit.format(sys::mem::memory_used()),
            unit.format(sys::mem::memory_size())
        );
        let right = format!(
            " TIEMPO {}   {} ",
            format_uptime(sys::clk::boot_time()),
            sys::clk::date()
        );
        let width = area.width as usize;
        let gap = width.saturating_sub(left.len() + right.len() + 1);
        let line = Line::from(vec![
            Span::styled(left, Style::new().fg(Color::DarkGray)),
            Span::raw(" ".repeat(gap)),
            Span::styled(right, Style::new().fg(Color::DarkGray)),
        ]);
        frame.render_widget(Paragraph::new(line), area);
    }
}

/// The wallpaper shown when no app is open: wordmark, tagline, hairline.
fn render_wallpaper(frame: &mut Frame<'_>, area: Rect) {
    let mut lines: Vec<Line<'static>> = vec![Line::raw("")];

    let wordmark = "TUI-OS";
    let pad = (area.width as usize).saturating_sub(wordmark.len()) / 2;
    lines.push(Line::from(vec![
        Span::raw(" ".repeat(pad)),
        Span::styled(
            wordmark,
            Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("v{}", version()),
            Style::new()
                .fg(Color::LightCyan)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    let tagline = "Escritorio 100% Rust sobre VGA 80x25";
    let pad = (area.width as usize).saturating_sub(tagline.chars().count()) / 2;
    lines.push(Line::styled(
        format!("{}{}", " ".repeat(pad), tagline),
        Style::new().fg(Color::DarkGray),
    ));

    let divider = "────── · ──────";
    // Count characters, not UTF-8 bytes: '─' and '·' are multibyte.
    let pad = (area.width as usize).saturating_sub(divider.chars().count()) / 2;
    lines.push(Line::styled(
        format!("{}{}", " ".repeat(pad), divider),
        Style::new().fg(Color::DarkGray),
    ));

    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(Color::DarkGray))
        .title(" Escritorio ")
        .title_alignment(Alignment::Center);
    frame.render_widget(
        Paragraph::new(Text::from(lines)).wrap(ratatui::widgets::Wrap { trim: false }).block(block),
        area,
    );
}

/// Launch the desktop. This function never returns.
pub fn main() -> ! {
    sys::vga::tui_init();
    sys::console::disable_echo();
    sys::console::enable_raw();
    sys::keyboard::drain_decoded_keys();

    let backend = VgaBackend::new();
    let mut terminal = match Terminal::new(backend) {
        Ok(t) => t,
        Err(_) => {
            // Errors surface as a Spanish message; they never panic. The VGA
            // text buffer is already live at this point, so printk reaches the
            // screen the user is looking at.
            printk!("No se pudo inicializar el terminal VGA.\r\n");
            printk!("El escritorio no puede arrancar. Apagando el equipo.\r\n");
            sys::acpi::shutdown();
            loop {
                sys::x86::hlt();
            }
        }
    };
    terminal.clear().ok();

    let mut desktop = Desktop::new();
    desktop.run(&mut terminal);

    // Only reached if the desktop is stopped without a reboot; park the CPU.
    sys::console::disable_raw();
    sys::console::enable_echo();
    loop {
        sys::x86::hlt();
    }
}

#[cfg(test)]
mod tests {
    use super::{as_route_code, route_key, AppKind, DesktopCmd};
    use pc_keyboard::{DecodedKey, KeyCode};

    #[test_case]
    fn enter_and_escape_arrive_as_control_characters() {
        // Every pc_keyboard layout -- en-US included -- decodes Return as
        // `'\n'` and Escape as 0x1B, so `KeyCode::Return` and
        // `KeyCode::Escape` are never handed to us. The routing table speaks
        // `KeyCode`, so the loop has to recognise these two itself; otherwise
        // Enter launches nothing from the dock and Esc never closes an app.
        assert_eq!(
            as_route_code(DecodedKey::Unicode('\n')),
            Some(KeyCode::Return)
        );
        assert_eq!(
            as_route_code(DecodedKey::Unicode('\r')),
            Some(KeyCode::Return)
        );
        assert_eq!(
            as_route_code(DecodedKey::Unicode('\x1b')),
            Some(KeyCode::Escape)
        );

        // F-keys and the arrows do arrive as raw codes and pass straight
        // through, while a printable character is none of the desktop's
        // business and must be handed to the app instead.
        assert_eq!(
            as_route_code(DecodedKey::RawKey(KeyCode::F2)),
            Some(KeyCode::F2)
        );
        assert_eq!(
            as_route_code(DecodedKey::RawKey(KeyCode::ArrowLeft)),
            Some(KeyCode::ArrowLeft)
        );
        assert_eq!(as_route_code(DecodedKey::Unicode('a')), None);
    }

    #[test_case]
    fn route_key_desktop_navigation() {
        assert_eq!(
            route_key(None, 0, KeyCode::ArrowRight),
            (DesktopCmd::None, 1)
        );
        assert_eq!(
            route_key(None, 1, KeyCode::ArrowLeft),
            (DesktopCmd::None, 0)
        );
        assert_eq!(
            route_key(None, 4, KeyCode::ArrowRight),
            (DesktopCmd::None, 0)
        ); // wrap
        assert_eq!(
            route_key(None, 2, KeyCode::Return),
            (DesktopCmd::Open(AppKind::SysInfo), 2)
        );
        assert_eq!(route_key(None, 4, KeyCode::Return), (DesktopCmd::Halt, 4));
        assert_eq!(
            route_key(None, 0, KeyCode::F3),
            (DesktopCmd::Switch(AppKind::Terminal), 0)
        );
        assert_eq!(
            route_key(Some(AppKind::Terminal), 2, KeyCode::F2),
            (DesktopCmd::Switch(AppKind::SysInfo), 2)
        );
        assert_eq!(
            route_key(Some(AppKind::Terminal), 2, KeyCode::Escape),
            (DesktopCmd::Close, 2)
        );
        // App focused: Left/Right belong to the app, not the dock.
        assert_eq!(
            route_key(Some(AppKind::Files), 2, KeyCode::ArrowRight),
            (DesktopCmd::None, 2)
        );
    }
}
