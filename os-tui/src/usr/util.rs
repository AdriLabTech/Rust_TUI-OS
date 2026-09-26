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

/// Format a byte count with binary (1024-based) units, the way the memory
/// figures in the status bar read. Truncates rather than rounds, so a figure
/// never claims more memory than is actually there.
pub fn format_size(bytes: usize) -> String {
    const K: usize = 1024;
    const M: usize = K * 1024;
    const G: usize = M * 1024;
    if bytes >= G {
        format!("{}G", bytes / G)
    } else if bytes >= M {
        format!("{}M", bytes / M)
    } else if bytes >= K {
        format!("{}K", bytes / K)
    } else {
        format!("{}B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn format_size_uses_binary_units() {
        assert_eq!(format_size(1024), "1K");
        assert_eq!(format_size(17 * 1024 * 1024), "17M");
    }

    /// The unit boundary cases, because picking the wrong divisor is the whole
    /// way this function can be wrong: 1023 is not 1K, and 1048576 is 1M and
    /// not 1024K.
    #[test_case]
    fn format_size_switches_unit_on_the_right_boundary() {
        assert_eq!(format_size(0), "0B");
        assert_eq!(format_size(512), "512B");
        assert_eq!(format_size(1023), "1023B");
        assert_eq!(format_size(1024), "1K");
        assert_eq!(format_size(1024 * 1024 - 1), "1023K");
        assert_eq!(format_size(1024 * 1024), "1M");
        assert_eq!(format_size(1024 * 1024 * 1024), "1G");
    }
}
