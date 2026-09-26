//! The system information app: CPU, memory, uptime, live.

use crate::api;
use crate::sys;
use crate::usr::apps::terminal::COMMANDS;
use crate::usr::apps::AppAction;
use crate::usr::util::{format_uptime, label_span, version};

use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;

use pc_keyboard::DecodedKey;

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Gauge, Paragraph};
use ratatui::Frame;

/// A read-only view of the machine. It holds no state of its own.
pub struct SysInfoApp;

impl SysInfoApp {
    pub fn new() -> Self {
        SysInfoApp
    }

    /// Every key is inert here: the desktop owns the keys that leave the app.
    pub fn handle_key(&mut self, _key: DecodedKey) -> AppAction {
        AppAction::Keep
    }

    pub fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        let chunks = Layout::vertical([Constraint::Min(4), Constraint::Length(1)]).split(area);

        let cpuid = sys::cpu::cpuid();
        let vendor = cpuid
            .get_vendor_info()
            .map_or("desconocido".to_string(), |v| format!("{}", v));
        let brand = cpuid
            .get_processor_brand_string()
            .map_or("desconocido".to_string(), |b| b.as_str().trim().to_string());
        let freq = cpuid
            .get_processor_frequency_info()
            .map_or(0, |f| f.processor_base_frequency());
        // A zero here means CPUID did not report a base frequency, not that the
        // processor runs at 0 MHz. Say so rather than print a wrong number.
        let freq = if freq == 0 {
            "no disponible".to_string()
        } else {
            format!("{} MHz", freq)
        };

        let unit = api::unit::SizeUnit::Binary;
        let total = sys::mem::memory_size();
        let used = sys::mem::memory_used();
        let free = sys::mem::memory_free();

        let info = |label: &str, value: String| {
            Line::from(vec![
                // Wide enough for the longest Spanish label ("Tiempo activo"),
                // otherwise the value butts straight up against it.
                label_span(label, 14),
                Span::styled(value, Style::new().fg(Color::White)),
            ])
        };
        let lines: Vec<Line<'static>> = vec![
            info("Fabricante", vendor),
            info("Modelo", brand),
            info("Frecuencia", freq),
            info("Kernel", format!("TUI-OS v{} (amd64)", version())),
            info("RAM total", unit.format(total)),
            info("RAM en uso", unit.format(used)),
            info("RAM libre", unit.format(free)),
            info("Tiempo activo", format_uptime(sys::clk::boot_time())),
            info("Fecha", sys::clk::date()),
            info("Video", "VGA texto 80x25 @ 720x400".to_string()),
            info("Renderizador", "ratatui 0.30 (VgaBackend)".to_string()),
            info("Comandos", format!("{} integrados", COMMANDS.len())),
        ];

        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(Color::DarkGray))
            .title(" Sistema ")
            .title_alignment(Alignment::Center);
        frame.render_widget(Paragraph::new(Text::from(lines)).block(block), chunks[0]);

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
}
