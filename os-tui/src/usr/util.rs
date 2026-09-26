//! Small helpers shared by the desktop and the apps it runs.

use alloc::format;
use alloc::string::String;
use alloc::string::ToString;

use ratatui::style::{Color, Style};
use ratatui::text::Span;

/// The version of the whole operating system, gathered at compile time.
pub fn version() -> String {
    option_env!("TUIOS_VERSION")
        .unwrap_or(env!("CARGO_PKG_VERSION"))
        .to_string()
}

/// A label span with a fixed width, used by the system information screen.
pub fn label_span(label: &str, width: usize) -> Span<'static> {
    Span::styled(
        format!("{:<width$}", label, width = width),
        Style::new().fg(Color::LightBlue),
    )
}

/// Format a number of seconds as a short human-readable uptime.
pub fn format_uptime(seconds: f64) -> String {
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
