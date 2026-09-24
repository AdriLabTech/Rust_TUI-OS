//! The built-in TUI shell.
//!
//! This is the whole "userspace" of the OS: a Ratatui-rendered shell that
//! draws windows on the VGA 80x25 text buffer and ships its own commands
//! (help, sysinfo, widgets, echo, clear, date, uptime, version, mem, halt,
//! reboot, quit).

use crate::api;
use crate::sys;
use crate::usr::tui::VgaBackend;

use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;

use pc_keyboard::{DecodedKey, KeyCode};

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, BorderType, Borders, Gauge, List, ListItem, ListState, Paragraph, Sparkline, Tabs,
    Wrap,
};
use ratatui::{Frame, Terminal};

/// Maximum number of characters in the command input line.
const MAX_INPUT: usize = 78;

/// Maximum number of lines kept in the on-screen log.
const MAX_LOG: usize = 256;

/// The number of tabs in the widgets demo.
const TAB_COUNT: usize = 4;

/// The version of the whole operating system, gathered at compile time.
pub fn version() -> String {
    option_env!("TUIOS_VERSION")
        .unwrap_or(env!("CARGO_PKG_VERSION"))
        .to_string()
}

/// The built-in commands, with a one-line description for the Help screen.
const COMMANDS: &[(&str, &str)] = &[
    ("help", "Show this help screen"),
    ("sysinfo", "Show live system information"),
    ("widgets", "Open the ratatui widgets demo"),
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

/// The main areas of the 25-row screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    Home,
    Help,
    SysInfo,
    Widgets,
}

/// One line of the shell output log.
struct LogEntry {
    text: String,
    prompt: bool,
}

/// The state of the shell, mutated by key presses and redrawn on each frame.
struct Shell {
    screen: Screen,
    input: String,
    cursor: usize,
    history: Vec<String>,
    history_pos: Option<usize>,
    log: Vec<LogEntry>,
    widgets_list: Vec<String>,
    list_state: ListState,
    tab_index: usize,
    running: bool,
}

impl Shell {
    fn new() -> Self {
        let mut shell = Shell {
            screen: Screen::Home,
            input: String::new(),
            cursor: 0,
            history: Vec::new(),
            history_pos: None,
            log: Vec::new(),
            widgets_list: vec![
                "Calculator".to_string(),
                "Editor".to_string(),
                "Files".to_string(),
                "Games".to_string(),
                "Network".to_string(),
                "Settings".to_string(),
                "System".to_string(),
            ],
            list_state: ListState::default(),
            tab_index: 0,
            running: true,
        };
        shell.push_log(
            format!("TUI-OS v{} — 100% Rust kernel + bootloader + shell", version()),
            false,
        );
        shell.push_log(
            "Rendered by ratatui 0.30 directly into the VGA 80x25 text buffer".to_string(),
            false,
        );
        shell.push_log("No userspace: everything lives in the kernel crate".to_string(), false);
        shell.push_log("Type 'help' (or press F1) to list the built-in commands.".to_string(), false);
        shell
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

    fn handle_key(&mut self, key: DecodedKey) {
        match key {
            DecodedKey::Unicode(c) => match c {
                '\n' => self.submit(),
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
                KeyCode::ArrowUp => self.up(),
                KeyCode::ArrowDown => self.down(),
                KeyCode::ArrowLeft => self.left(),
                KeyCode::ArrowRight => self.right(),
                KeyCode::Home => self.cursor = 0,
                KeyCode::End => self.cursor = self.input.chars().count(),
                KeyCode::F1 => self.screen = Screen::Help,
                KeyCode::F2 => self.screen = Screen::SysInfo,
                KeyCode::F3 => self.screen = Screen::Widgets,
                KeyCode::F4 => self.screen = Screen::Home,
                _ => {}
            },
        }
    }

    fn up(&mut self) {
        match self.screen {
            Screen::Widgets => self.widgets_up(),
            _ => self.history_prev(),
        }
    }

    fn down(&mut self) {
        match self.screen {
            Screen::Widgets => self.widgets_down(),
            _ => self.history_next(),
        }
    }

    fn left(&mut self) {
        if self.screen == Screen::Widgets {
            self.tab_index = (self.tab_index + TAB_COUNT - 1) % TAB_COUNT;
        } else {
            self.cursor = self.cursor.saturating_sub(1);
        }
    }

    fn right(&mut self) {
        if self.screen == Screen::Widgets {
            self.tab_index = (self.tab_index + 1) % TAB_COUNT;
        } else {
            self.cursor = (self.cursor + 1).min(self.input.chars().count());
        }
    }

    fn tab(&mut self) {
        if self.screen == Screen::Widgets {
            self.tab_index = (self.tab_index + 1) % TAB_COUNT;
        } else {
            self.complete();
        }
    }

    fn widgets_up(&mut self) {
        let n = self.widgets_list.len();
        let sel = self.list_state.selected().unwrap_or(0);
        self.list_state.select(Some((sel + n - 1) % n));
    }

    fn widgets_down(&mut self) {
        let n = self.widgets_list.len();
        let sel = self.list_state.selected().unwrap_or(0);
        self.list_state.select(Some((sel + 1) % n));
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
        self.screen = Screen::Home;
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

    fn submit(&mut self) {
        let cmd = self.input.trim().to_string();
        if !cmd.is_empty() {
            self.history.push(cmd.clone());
            self.history_pos = None;
            self.push_log(format!("> {}", cmd), true);
            for line in self.exec(&cmd) {
                self.push_log(line, false);
            }
        }
        self.input.clear();
        self.cursor = 0;
    }

    fn exec(&mut self, cmdline: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut parts = cmdline.split_whitespace();
        let cmd = match parts.next() {
            Some(c) => c,
            None => return out,
        };
        let args: Vec<&str> = parts.collect();
        match cmd {
            "help" => {
                self.screen = Screen::Help;
                out.push("Showing the command list, press F4 to go home.".to_string());
            }
            "sysinfo" => {
                self.screen = Screen::SysInfo;
                out.push("Showing live system information, press F4 to go home.".to_string());
            }
            "widgets" => {
                self.screen = Screen::Widgets;
                out.push("Showing the widgets demo, press F4 to go home.".to_string());
            }
            "clear" => {
                self.log.clear();
                self.screen = Screen::Home;
            }
            "echo" => out.push(args.join(" ")),
            "date" => out.push(sys::clk::date()),
            "uptime" => out.push(format!("Uptime: {}", format_uptime(sys::clk::boot_time()))),
            "version" => out.push(version()),
            "mem" => {
                let unit = api::unit::SizeUnit::Binary;
                let total = sys::mem::memory_size();
                let used = sys::mem::memory_used();
                let free = sys::mem::memory_free();
                out.push(format!("Memory total: {}", unit.format(total)));
                out.push(format!("Memory used:  {}", unit.format(used)));
                out.push(format!("Memory free:  {}", unit.format(free)));
            }
            "halt" | "quit" => {
                out.push("Halting…".to_string());
                self.running = false;
                sys::acpi::shutdown();
            }
            "reboot" => {
                sys::idt::reset(); // Never returns
            }
            _ => out.push(format!("command not found: '{}' — type 'help'", cmd)),
        }
        out
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
            if !self.running {
                return;
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
            Constraint::Min(8),    // main content
            Constraint::Length(1), // hint bar
            Constraint::Length(3), // input box
            Constraint::Length(1), // status bar
        ])
        .split(area);

        self.render_title(frame, chunks[0]);
        match self.screen {
            Screen::Home => self.render_home(frame, chunks[1]),
            Screen::Help => self.render_help(frame, chunks[1]),
            Screen::SysInfo => self.render_sysinfo(frame, chunks[1]),
            Screen::Widgets => self.render_widgets(frame, chunks[1]),
        }
        self.render_hint(frame, chunks[2]);
        self.render_input(frame, chunks[3]);
        self.render_status(frame, chunks[4]);
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

    fn render_home(&self, frame: &mut Frame<'_>, area: Rect) {
        // Refined, quiet branding: two-tone logotype + tagline + hairline.
        let banner_h = 5; // blank + wordmark + tagline + hairline + blank
        let n = (area.height as usize).saturating_sub(banner_h);
        let mut lines: Vec<Line<'static>> = Vec::new();

        lines.push(Line::raw(""));
        let wordmark = format!("TUI-OS  v{}", version());
        let pad = (area.width as usize).saturating_sub(wordmark.len()) / 2;
        lines.push(Line::from(vec![
            Span::raw(" ".repeat(pad)),
            Span::styled(
                "TUI-OS",
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
        lines.push(Line::raw(""));

        // Bottom-aligned log tail: blank space on top like a real terminal.
        let taken = self.log.len().min(n);
        for _ in 0..(n - taken) {
            lines.push(Line::raw(""));
        }
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

    fn render_help(&self, frame: &mut Frame<'_>, area: Rect) {
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
            "F1-F4 switch screens, Tab completes command names, arrows recall".to_string(),
            Style::new().fg(Color::DarkGray),
        ));
        lines.push(Line::styled(
            "history, Ctrl+L clears the screen, Ctrl+C cancels the input line.".to_string(),
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

    fn render_sysinfo(&self, frame: &mut Frame<'_>, area: Rect) {
        let chunks = Layout::vertical([Constraint::Min(4), Constraint::Length(1)]).split(area);

        let cpuid = sys::cpu::cpuid();
        let vendor = cpuid
            .get_vendor_info()
            .map_or("unknown".to_string(), |v| format!("{}", v));
        let brand = cpuid
            .get_processor_brand_string()
            .map_or("unknown".to_string(), |b| b.as_str().trim().to_string());
        let freq = cpuid
            .get_processor_frequency_info()
            .map_or(0, |f| f.processor_base_frequency());

        let unit = api::unit::SizeUnit::Binary;
        let total = sys::mem::memory_size();
        let used = sys::mem::memory_used();
        let free = sys::mem::memory_free();

        let info = |label: &str, value: String| {
            Line::from(vec![
                label_span(label, 12),
                Span::styled(value, Style::new().fg(Color::White)),
            ])
        };
        let lines: Vec<Line<'static>> = vec![
            info("CPU vendor", vendor),
            info("CPU model", brand),
            info("CPU speed", format!("{} MHz", freq)),
            info("Kernel", format!("TUI-OS v{} (amd64)", version())),
            info("RAM total", unit.format(total)),
            info("RAM used", unit.format(used)),
            info("RAM free", unit.format(free)),
            info("Uptime", format_uptime(sys::clk::boot_time())),
            info("Date", sys::clk::date()),
            info("Video", "VGA text 80x25 @ 720x400".to_string()),
            info("Renderer", "ratatui 0.30 (VgaBackend)".to_string()),
            info("Shell", format!("{} built-in commands", COMMANDS.len())),
        ];

        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(Color::DarkGray))
            .title(" Sistema ")
            .title_alignment(Alignment::Center);
        frame.render_widget(Paragraph::new(Text::from(lines)).block(block), chunks[0]);

        // Memory usage gauge
        let ratio = if total > 0 {
            (used as f64) / (total as f64)
        } else {
            0.0
        };
        let gauge = Gauge::default()
            .gauge_style(Style::new().fg(Color::LightGreen))
            .ratio(ratio)
            .label(format!(" RAM {:.0}% ", ratio * 100.0));
        frame.render_widget(gauge, chunks[1]);
    }

    fn render_widgets(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let chunks = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(area);

        // Tab bar on top, as a window with tabs.
        let titles = ["Gauge", "Sparkline", "List", "Notes"];
        let tabs = Tabs::new(titles.to_vec())
            .select(self.tab_index % TAB_COUNT)
            .highlight_style(
                Style::new()
                    .fg(Color::Black)
                    .bg(Color::LightCyan)
                    .add_modifier(Modifier::BOLD),
            )
            .divider("│");
        let tabs_block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(Color::DarkGray));
        frame.render_widget(tabs_block, chunks[0]);
        let tabs_inner = Rect::new(chunks[0].x + 1, chunks[0].y, chunks[0].width - 2, 1);
        frame.render_widget(tabs, tabs_inner);

        // Two windows side by side.
        let body = chunks[1];
        let cols = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(body);
        let left = Layout::vertical([Constraint::Length(4), Constraint::Min(1)]).split(cols[0]);
        let right = Layout::vertical([Constraint::Length(6), Constraint::Min(1)]).split(cols[1]);

        // Window #1: memory gauge
        let total = sys::mem::memory_size();
        let used = sys::mem::memory_used();
        let ratio = if total > 0 {
            (used as f64) / (total as f64)
        } else {
            0.0
        };
        let gauge = Gauge::default()
            .block(
                Block::new()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::new().fg(Color::LightGreen))
                    .title(" Memory ")
                    .title_alignment(Alignment::Center),
            )
            .gauge_style(Style::new().fg(Color::LightGreen))
            .ratio(ratio)
            .label(format!(
                " {:.0}% of {} ",
                ratio * 100.0,
                api::unit::SizeUnit::Binary.format(total)
            ));
        frame.render_widget(gauge, left[0]);

        // Window #2: an animated sparkline
        let t = sys::clk::ticks() as u32;
        let data: Vec<u64> = (0..32)
            .map(|i| {
                let wave = (i * 8 + t / 30) % 25;
                let base = ((i * 7 + t / 10) % 40) / 2;
                5 + ((wave + base) % 41) as u64
            })
            .collect();
        let sparkline = Sparkline::default()
            .block(
                Block::new()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::new().fg(Color::LightCyan))
                    .title(" Signal ")
                    .title_alignment(Alignment::Center),
            )
            .style(Style::new().fg(Color::LightCyan))
            .data(data);
        frame.render_widget(sparkline, left[1]);

        // Window #3: a selectable list
        let items: Vec<ListItem<'static>> = self
            .widgets_list
            .iter()
            .map(|name| ListItem::new(name.clone()))
            .collect();
        let list = List::new(items)
            .block(
                Block::new()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::new().fg(Color::LightYellow))
                    .title(" Dock ")
                    .title_alignment(Alignment::Center),
            )
            .highlight_style(Style::new().fg(Color::Black).bg(Color::LightYellow))
            .highlight_symbol("> ");
        frame.render_stateful_widget(list, right[0], &mut self.list_state);

        // Window #4: an "about" paragraph
        let text = Text::from(vec![
            Line::styled(
                "100% Rust",
                Style::new()
                    .fg(Color::LightGreen)
                    .add_modifier(Modifier::BOLD),
            ),
            Line::raw("TUI-OS is a UNIX-like OS: the kernel,"),
            Line::raw("bootloader and this shell are one crate."),
            Line::raw("This very UI is rendered by ratatui 0.30"),
            Line::raw("straight into the VGA text buffer."),
            Line::styled(
                "Press F1-F4 to switch screens.",
                Style::new().fg(Color::LightYellow),
            ),
        ]);
        let paragraph = Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .block(
                Block::new()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::new().fg(Color::LightMagenta))
                    .title(" About ")
                    .title_alignment(Alignment::Center),
            );
        frame.render_widget(paragraph, right[1]);
    }

    fn render_hint(&self, frame: &mut Frame<'_>, area: Rect) {
        let text = match self.screen {
            Screen::Home => {
                " F1 help  F2 sysinfo  F3 widgets  F4 home  |  Tab complete  \u{2191}/\u{2193} history  Ctrl+L clear  Enter run"
            }
            Screen::Help => " F1 help  F2 sysinfo  F3 widgets  F4 home  |  Enter run  Tab: complete  Ctrl+L: clear",
            Screen::SysInfo => " F1 help  F2 sysinfo  F3 widgets  F4 home  |  Live data, refreshed a few times per second",
            Screen::Widgets => {
                " \u{2191}/\u{2193}: select item   \u{2190}/\u{2192}: tab   Tab: next tab   F1-F4: other screens"
            }
        };
        let paragraph =
            Paragraph::new(Line::styled(text.to_string(), Style::new().fg(Color::DarkGray)));
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

    fn render_status(&self, frame: &mut Frame<'_>, area: Rect) {
        let unit = api::unit::SizeUnit::Binary;
        let left = format!(
            " MEM {} / {} ",
            unit.format(sys::mem::memory_used()),
            unit.format(sys::mem::memory_size())
        );
        let right = format!(
            " UP {}   {} ",
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

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

/// A label span with a fixed width, used by the sysinfo screen.
fn label_span(label: &str, width: usize) -> Span<'static> {
    Span::styled(
        format!("{:<width$}", label, width = width),
        Style::new().fg(Color::LightBlue),
    )
}

/// Format a number of seconds as a short human-readable uptime.
fn format_uptime(seconds: f64) -> String {
    let secs = seconds as u64;
    let d = secs / 86_400;
    let h = (secs % 86_400) / 3_600;
    let m = (secs % 3_600) / 60;
    let s = secs % 60;
    if d > 0 {
        format!("{}d {}h {}m", d, h, m)
    } else if h > 0 {
        format!("{}h {}m {}s", h, m, s)
    } else if m > 0 {
        format!("{}m {}s", m, s)
    } else {
        format!("{}s", s)
    }
}

// ---------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------

/// Launch the TUI shell. This function never returns.
pub fn main(_args: &[&str]) -> ! {
    sys::vga::tui_init();
    sys::console::disable_echo();
    sys::console::enable_raw();
    sys::keyboard::drain_decoded_keys();

    let backend = VgaBackend::new();
    let mut terminal = Terminal::new(backend).expect("could not initialize the VGA terminal");
    terminal.clear().ok();

    let mut shell = Shell::new();
    shell.run(&mut terminal);

    // Only reached if the shell is stopped without a reboot; park the CPU.
    sys::console::disable_raw();
    sys::console::enable_echo();
    loop {
        sys::x86::hlt();
    }
}