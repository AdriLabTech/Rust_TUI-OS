//! The VGA text-mode backend for Ratatui.
//!
//! This module implements [`ratatui::backend::Backend`] over the 80x25 VGA
//! text buffer, translating the Unicode symbols produced by Ratatui into
//! Code page 437 characters and the 16 VGA palette colors.

use ratatui::backend::{Backend, ClearType, WindowSize};
use ratatui::buffer::Cell;
use ratatui::layout::{Position, Rect, Size};
use ratatui::style::Color as RColor;

use crate::sys::vga;

/// VGA text mode width in columns.
pub const WIDTH: u16 = 80;
/// VGA text mode height in rows.
pub const HEIGHT: u16 = 25;

/// The error type of [`VgaBackend`].
///
/// It is uninhabited because writing to VGA memory can never fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VgaError {}

impl core::fmt::Display for VgaError {
    fn fmt(&self, _f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {}
    }
}

impl core::error::Error for VgaError {}

/// Translate a Ratatui color into a VGA color index (0-15).
pub fn color_to_vga_index(color: RColor) -> u8 {
    match color {
        RColor::Reset => 7,
        RColor::Black => 0,
        RColor::Red => 4,
        RColor::Green => 2,
        RColor::Yellow => 6,
        RColor::Blue => 1,
        RColor::Magenta => 5,
        RColor::Cyan => 3,
        RColor::Gray => 7,
        RColor::DarkGray => 8,
        RColor::LightRed => 12,
        RColor::LightGreen => 10,
        RColor::LightYellow => 14,
        RColor::LightBlue => 9,
        RColor::LightMagenta => 13,
        RColor::LightCyan => 11,
        RColor::White => 15,
        RColor::Indexed(n) => n % 16,
        // True color has no match in VGA text mode; default to light gray.
        RColor::Rgb(_, _, _) => 7,
    }
}

/// Like [`color_to_vga_index`], but for cell *backgrounds*: a terminal's
/// default (reset) background is black — with the Ink & Brass palette that is
/// index 0 (ink), not the LightGray used for the default foreground.
pub fn bg_to_vga_index(color: RColor) -> u8 {
    match color {
        RColor::Reset => 0,
        c => color_to_vga_index(c),
    }
}

/// Translate a Unicode character into the Code page 437 byte that the VGA
/// text buffer actually stores.
///
/// Symbols that do not exist in CP437 (and are not part of the glyphs used
/// by the widgets below) render as a blank rather than as garbage.
pub fn char_to_cp437(c: char) -> u8 {
    match c {
        // Box drawing
        '─' | '━' => 0xC4,
        '│' | '┃' => 0xB3,
        '┌' => 0xDA,
        '┐' => 0xBF,
        '└' => 0xC0,
        '┘' => 0xD9,
        '├' => 0xC3,
        '┤' => 0xB4,
        '┬' => 0xC2,
        '┴' => 0xC1,
        '┼' => 0xC5,
        // Rounded corners (used by BorderType::Rounded)
        '╭' => 0xDA,
        '╮' => 0xBF,
        '╰' => 0xC0,
        '╯' => 0xD9,
        // Double lines
        '═' => 0xCD,
        '║' => 0xBA,
        '╔' => 0xC9,
        '╗' => 0xBB,
        '╚' => 0xC8,
        '╝' => 0xBC,
        '╠' => 0xCC,
        '╣' => 0xB9,
        '╦' => 0xCB,
        '╩' => 0xCA,
        '╬' => 0xCE,
        // Blocks
        '█' => 0xDB,
        '▀' => 0xDF,
        '▄' => 0xDC,
        '▌' | '▐' => 0xDB,
        '░' => 0xB0,
        '▒' => 0xB1,
        '▓' => 0xB2,
        // Sparkline bars mapped onto the closest CP437 density
        '▁' | '▂' => 0xB0,
        '▃' => 0xB1,
        '▅' => 0xB1,
        '▆' | '▇' => 0xB2,
        // Arrows
        '▲' => 0x1E,
        '▼' => 0x1F,
        '◀' | '◄' => 0x11,
        '▶' | '►' => 0x10,
        // The dock's selection marker and the title bar's breadcrumb. CP437
        // carries neither a small triangle nor an angle quote, so both borrow
        // the big triangle. Unmapped they draw as blank cells, and a blank
        // where the marker belongs makes the dock look like it has no
        // selection at all.
        '▸' => 0x10,
        '‹' => 0x11,
        // Prompts and bullets
        '❯' | '»' => 0xAF,
        '•' | '●' => 0x07,
        '·' => 0xFA,
        '…' => 0x2E,
        // The Latin-1 letters CP437 carries. The interface is in Spanish, so
        // these are load-bearing rather than decorative: an unmapped letter is
        // written as a blank cell, which reads as a typo instead of as a
        // missing glyph ("Información" came out as "Informaci n").
        //
        // CP437 has no uppercase accented letters and no inverted question or
        // exclamation marks, so `Á`, `Ó`, `¿` and `¡` cannot be drawn at all
        // and must be kept out of UI strings. Accents belong inside words.
        'Ç' => 0x80,
        'ü' => 0x81,
        'é' => 0x82,
        'â' => 0x83,
        'ä' => 0x84,
        'à' => 0x85,
        'å' => 0x86,
        'ç' => 0x87,
        'ê' => 0x88,
        'ë' => 0x89,
        'è' => 0x8A,
        'ï' => 0x8B,
        'î' => 0x8C,
        'ì' => 0x8D,
        'Ä' => 0x8E,
        'Å' => 0x8F,
        'É' => 0x90,
        'æ' => 0x91,
        'Æ' => 0x92,
        'ô' => 0x93,
        'ö' => 0x94,
        'ò' => 0x95,
        'û' => 0x96,
        'ù' => 0x97,
        'ÿ' => 0x98,
        'Ö' => 0x99,
        'Ü' => 0x9A,
        'á' => 0xA0,
        'í' => 0xA1,
        'ó' => 0xA2,
        'ú' => 0xA3,
        'ñ' => 0xA4,
        'Ñ' => 0xA5,
        'ß' => 0xE1,
        '°' => 0xF8,
        // Everything else that CP437 shares with ASCII
        c if c.is_ascii() => c as u8,
        _ => 0x00,
    }
}

/// A [`Backend`] that renders Ratatui frames directly into the VGA text-mode
/// buffer (0xB8000), one cell at a time.
#[derive(Debug, Clone, Copy, Default)]
pub struct VgaBackend {
    cursor: Position,
}

impl VgaBackend {
    pub fn new() -> Self {
        Self {
            cursor: Position::new(0, 0),
        }
    }

    fn clear_area(&mut self, area: Rect) {
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                vga::tui_set_cell(x as usize, y as usize, b' ', 7, 0);
            }
        }
    }
}

impl Backend for VgaBackend {
    type Error = VgaError;

    fn draw<'a, I>(&mut self, content: I) -> Result<(), Self::Error>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        for (x, y, cell) in content {
            let symbol = cell.symbol();
            let c = symbol.chars().next().unwrap_or(' ');
            let fg = color_to_vga_index(cell.fg);
            let bg = bg_to_vga_index(cell.bg);
            vga::tui_set_cell(x as usize, y as usize, char_to_cp437(c), fg, bg);
        }
        Ok(())
    }

    fn hide_cursor(&mut self) -> Result<(), Self::Error> {
        vga::tui_hide_cursor();
        Ok(())
    }

    fn show_cursor(&mut self) -> Result<(), Self::Error> {
        vga::tui_show_cursor();
        Ok(())
    }

    fn get_cursor_position(&mut self) -> Result<Position, Self::Error> {
        Ok(self.cursor)
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> Result<(), Self::Error> {
        let pos = position.into();
        self.cursor = pos;
        vga::tui_set_cursor_position(pos.x as usize, pos.y as usize);
        Ok(())
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        vga::tui_clear_screen();
        Ok(())
    }

    fn clear_region(&mut self, clear_type: ClearType) -> Result<(), Self::Error> {
        let (x, y) = (self.cursor.x, self.cursor.y);
        match clear_type {
            ClearType::All => vga::tui_clear_screen(),
            ClearType::AfterCursor => {
                self.clear_area(Rect::new(x, y, WIDTH - x, HEIGHT - y));
            }
            ClearType::BeforeCursor => {
                self.clear_area(Rect::new(0, 0, (x + 1).min(WIDTH), (y + 1).min(HEIGHT)));
            }
            ClearType::CurrentLine => self.clear_area(Rect::new(0, y, WIDTH, 1)),
            ClearType::UntilNewLine => self.clear_area(Rect::new(x, y, WIDTH - x, 1)),
        }
        Ok(())
    }

    fn size(&self) -> Result<Size, Self::Error> {
        Ok(Size::new(WIDTH, HEIGHT))
    }

    fn window_size(&mut self) -> Result<WindowSize, Self::Error> {
        // Standard VGA 80x25 text mode: 9x16 pixel character cells.
        Ok(WindowSize {
            columns_rows: Size::new(WIDTH, HEIGHT),
            pixels: Size::new(720, 400),
        })
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[test_case]
fn test_char_to_cp437_ascii() {
    assert_eq!(char_to_cp437('A'), b'A');
    assert_eq!(char_to_cp437(' '), b' ');
    assert_eq!(char_to_cp437('~'), b'~');
}

#[test_case]
fn test_char_to_cp437_box() {
    assert_eq!(char_to_cp437('┌'), 0xDA);
    assert_eq!(char_to_cp437('─'), 0xC4);
    assert_eq!(char_to_cp437('│'), 0xB3);
    assert_eq!(char_to_cp437('█'), 0xDB);
    assert_eq!(char_to_cp437('❯'), 0xAF);
}

#[test_case]
fn test_char_to_cp437_spanish() {
    // The interface is in Spanish, so these letters are load-bearing. CP437
    // carries every one of them; without the mapping each accented letter is
    // drawn as a blank cell, and "Información" reads as "Informaci n".
    assert_eq!(char_to_cp437('á'), 0xA0);
    assert_eq!(char_to_cp437('é'), 0x82);
    assert_eq!(char_to_cp437('í'), 0xA1);
    assert_eq!(char_to_cp437('ó'), 0xA2);
    assert_eq!(char_to_cp437('ú'), 0xA3);
    assert_eq!(char_to_cp437('ü'), 0x81);
    assert_eq!(char_to_cp437('ñ'), 0xA4);
    assert_eq!(char_to_cp437('Ñ'), 0xA5);
    assert_eq!(char_to_cp437('ç'), 0x87);
    assert_eq!(char_to_cp437('°'), 0xF8);

    // CP437 has no uppercase accented letters, so those cannot be drawn at
    // all and must never appear in a UI string. This asserts the limitation
    // is real, so nobody "fixes" a mangled word by reaching for `Á`.
    assert_eq!(char_to_cp437('Á'), 0);
    assert_eq!(char_to_cp437('Ó'), 0);
    assert_eq!(char_to_cp437('¿'), 0);
    assert_eq!(char_to_cp437('¡'), 0);
}

#[test_case]
fn test_char_to_cp437_arrows() {
    // The triangles are the arrows the UI can actually draw. They must never
    // fall through to the blank fallback, because the desktop hint bar uses
    // them and a blank cell there silently deletes the arrow from the text.
    assert_eq!(char_to_cp437('◀'), 0x11);
    assert_eq!(char_to_cp437('▶'), 0x10);
    assert_eq!(char_to_cp437('▲'), 0x1E);
    assert_eq!(char_to_cp437('▼'), 0x1F);

    // The Unicode arrows are deliberately unmapped: CP437 does draw them, at
    // 0x18-0x1B, but this table does not, so they render as blanks. Use the
    // triangles instead.
    assert_eq!(char_to_cp437('←'), 0);
    assert_eq!(char_to_cp437('→'), 0);
    assert_eq!(char_to_cp437('↑'), 0);
    assert_eq!(char_to_cp437('↓'), 0);
}

#[test_case]
fn test_char_to_cp437_dock_glyphs() {
    // The dock marks the selected entry with `▸` and the title bar opens with
    // `‹`. CP437 has neither a small triangle nor a single angle quote, so both
    // have to land on the big triangle it does carry. Left unmapped they draw
    // as blank cells, and a blank cell where the selection marker should be
    // makes the dock look like it has no selection at all.
    assert_eq!(char_to_cp437('▸'), 0x10);
    assert_eq!(char_to_cp437('‹'), 0x11);
}

#[test_case]
fn test_color_to_vga_index() {
    assert_eq!(color_to_vga_index(RColor::Black), 0);
    assert_eq!(color_to_vga_index(RColor::White), 15);
    assert_eq!(color_to_vga_index(RColor::LightCyan), 11);
    assert_eq!(color_to_vga_index(RColor::Indexed(20)), 4);
    assert_eq!(color_to_vga_index(RColor::Rgb(1, 2, 3)), 7);
}

#[test_case]
fn test_bg_to_vga_index() {
    // Reset backgrounds must be ink (0), never the LightGray foreground (7).
    assert_eq!(bg_to_vga_index(RColor::Reset), 0);
    assert_eq!(bg_to_vga_index(RColor::Black), 0);
    assert_eq!(bg_to_vga_index(RColor::Blue), 1);
    assert_eq!(bg_to_vga_index(RColor::LightCyan), 11);
    assert_eq!(bg_to_vga_index(RColor::White), 15);
}