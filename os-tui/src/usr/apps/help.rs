//! The help app: the command catalogue and the keyboard shortcuts.

use crate::usr::apps::terminal::COMMANDS;
use crate::usr::apps::AppAction;

use alloc::format;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;

use pc_keyboard::{DecodedKey, KeyCode};

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

/// The lines below the catalogue: a blank separator and the three key hints.
const FOOTER_LINES: usize = 4;

/// Every line the help view draws: one per command, plus the footer.
fn line_count() -> usize {
    COMMANDS.len() + FOOTER_LINES
}

/// The furthest down the view may scroll, in lines: the offset that brings the
/// last line up to the bottom of the box. Zero when everything already fits, and
/// saturating rather than wrapping, so a box taller than the list is not a
/// negative scroll.
fn max_scroll(visible: u16) -> u16 {
    (line_count() as u16).saturating_sub(visible)
}

/// Hold a requested offset inside the list. The app cannot clamp as the offset
/// moves, because the box height is only known once the widget is being drawn,
/// so the cap is applied there instead.
fn clamp_scroll(requested: u16, visible: u16) -> u16 {
    requested.min(max_scroll(visible))
}

/// A read-only view of the command list.
///
/// It holds exactly one piece of state: how far down the reader has scrolled.
/// That number is deliberately *not* clamped as it moves, because the height of
/// the box is unknown until the widget is being drawn; `render` applies the cap
/// through `clamp_scroll`.
pub struct HelpApp {
    requested: u16,
}

impl HelpApp {
    pub fn new() -> Self {
        HelpApp { requested: 0 }
    }

    /// The offset the reader has asked for, in lines. The number that actually
    /// gets drawn is this one after `clamp_scroll`, which needs a box height.
    pub fn scroll(&self) -> u16 {
        self.requested
    }

    /// The arrows scroll the catalogue. Everything else is left alone: the
    /// desktop already claimed the keys that leave the app, and a printable
    /// character reaching here is not an instruction to move the view.
    pub fn handle_key(&mut self, key: DecodedKey) -> AppAction {
        match key {
            DecodedKey::RawKey(KeyCode::ArrowDown) => {
                self.requested = self.requested.saturating_add(1)
            }
            DecodedKey::RawKey(KeyCode::ArrowUp) => self.requested = self.requested.saturating_sub(1),
            _ => {}
        }
        AppAction::Keep
    }

    pub fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        // The block draws a border on all four sides, so the text gets two rows
        // fewer than the box. `saturating_sub` because a box too small to hold
        // its own border is a zero-height paragraph, not a panic.
        let visible = area.height.saturating_sub(2) as u16;
        let offset = clamp_scroll(self.requested, visible);

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
        let paragraph = Paragraph::new(Text::from(lines))
            .block(block)
            .scroll((offset, 0));
        frame.render_widget(paragraph, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The property that must hold for *every* box height, not just today's:
    /// scrolling all the way down always lands the last line at the bottom. A
    /// cap built with wrapping arithmetic instead of `saturating_sub` breaks
    /// this at exactly one height, which is why the loop covers all of them
    /// rather than the one the desktop happens to use.
    #[test_case]
    fn help_can_always_reach_the_last_line() {
        for visible in 1..=line_count() as u16 {
            assert!(
                max_scroll(visible) as usize + visible as usize >= line_count(),
                "a box {visible} rows tall cannot reach the end of a {}-line list",
                line_count()
            );
        }
    }

    /// Today's numbers, so the regression stays legible: the catalogue used to
    /// be drawn once from the top with no offset, and the commands past the fold
    /// could never be seen. 27 commands plus a blank plus three notes is 31
    /// lines; the desktop gives an app rows 1..=22 and the border takes two, so
    /// 20 rows cannot show 31 lines and the view has to be able to move 11.
    #[test_case]
    fn help_scrolls_far_enough_to_reach_the_last_command() {
        assert_eq!(line_count(), COMMANDS.len() + 4);
        assert_eq!(line_count(), 31, "27 commands, a blank, and three notes");
        assert_eq!(max_scroll(20), 11, "the app area is 22 rows, less a 2-row border");
    }

    /// Holding the arrow down must not walk the view off the end into blank
    /// space. A cap that is one too high is the classic way to ship this, so the
    /// request is deliberately absurd: u16::MAX.
    #[test_case]
    fn help_scroll_is_capped_at_the_end_of_the_list() {
        assert_eq!(clamp_scroll(u16::MAX, 20), max_scroll(20));
        assert_eq!(clamp_scroll(max_scroll(20) + 1, 20), max_scroll(20));
    }

    /// When the box is tall enough for the whole list there is nothing to
    /// scroll, and the view must stay pinned to the top rather than drifting.
    /// This is the case the 1920x1080 work will actually land in.
    #[test_case]
    fn help_does_not_scroll_when_everything_fits() {
        let visible = line_count() as u16;
        assert_eq!(max_scroll(visible), 0);
        assert_eq!(clamp_scroll(7, visible), 0);
    }

    /// And symmetrically at the top: scrolling up from the first line stays
    /// there rather than wrapping round to the bottom.
    #[test_case]
    fn help_scroll_stays_at_the_top() {
        assert_eq!(clamp_scroll(0, 20), 0);
    }

    /// A view that opens at the top: the fix must not make the catalogue appear
    /// somewhere arbitrary, and the only safe place is the first command.
    #[test_case]
    fn help_starts_scrolled_to_the_top() {
        assert_eq!(HelpApp::new().scroll(), 0);
    }

    /// One line per press, and three presses are three lines -- not a page, and
    /// not a repeat. The arrows are the desktop's dock keys, so the help view
    /// only ever gets one press at a time.
    #[test_case]
    fn help_down_arrow_scrolls_one_line_per_press() {
        let mut app = HelpApp::new();
        for expected in 1..=3 {
            let action = app.handle_key(DecodedKey::RawKey(KeyCode::ArrowDown));
            assert_eq!(action, AppAction::Keep, "scrolling must not close the app");
            assert_eq!(app.scroll(), expected);
        }
    }

    /// Back up again, which is the half that a one-way offset gets wrong.
    #[test_case]
    fn help_up_arrow_scrolls_back_up() {
        let mut app = HelpApp::new();
        for _ in 0..4 {
            app.handle_key(DecodedKey::RawKey(KeyCode::ArrowDown));
        }
        for _ in 0..2 {
            app.handle_key(DecodedKey::RawKey(KeyCode::ArrowUp));
        }
        assert_eq!(app.scroll(), 2);
    }

    /// Up at the very top must stay there. A `u16` underflow in the wrong
    /// direction wraps the view to the far end of the list, which is the one
    /// place the reader never asked to be.
    #[test_case]
    fn help_up_arrow_at_the_top_stays_at_the_top() {
        let mut app = HelpApp::new();
        app.handle_key(DecodedKey::RawKey(KeyCode::ArrowUp));
        assert_eq!(app.scroll(), 0);
    }

    /// Everything that is not an arrow is still inert. The desktop hands the
    /// open app whatever it did not claim itself, so a stray letter or an F-key
    /// that slipped through must not scroll the catalogue.
    #[test_case]
    fn help_ignores_keys_that_are_not_the_arrows() {
        let mut app = HelpApp::new();
        let keys = [
            DecodedKey::Unicode('a'),
            DecodedKey::Unicode('\n'),
            DecodedKey::RawKey(KeyCode::Return),
            DecodedKey::RawKey(KeyCode::F7),
        ];
        for key in keys {
            assert_eq!(app.handle_key(key), AppAction::Keep);
        }
        assert_eq!(app.scroll(), 0);
    }
}
