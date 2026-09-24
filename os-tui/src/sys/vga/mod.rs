mod color;
mod font;
mod buffer;
mod palette;
mod screen;
mod writer;

pub use font::VgaFont;
pub use screen::VgaMode;
pub use palette::Palette as VgaPalette;
pub use buffer::Buffer as VgaBuffer;

use writer::WRITER;

use crate::sys::x86::int;
use crate::sys::x86::port::*;

use bit_field::BitField;
use core::cmp;
use core::fmt;
use core::fmt::Write;

const ATTR_ADDR_REG:           u16 = 0x3C0;
const ATTR_WRITE_REG:          u16 = 0x3C0;
const ATTR_READ_REG:           u16 = 0x3C1;
const MISC_WRITE_REG:          u16 = 0x3C2;
const SEQUENCER_ADDR_REG:      u16 = 0x3C4;
const SEQUENCER_DATA_REG:      u16 = 0x3C5;
const DAC_ADDR_READ_MODE_REG:  u16 = 0x3C7;
const DAC_ADDR_WRITE_MODE_REG: u16 = 0x3C8;
const DAC_DATA_REG:            u16 = 0x3C9;
const GRAPHICS_ADDR_REG:       u16 = 0x3CE;
const GRAPHICS_DATA_REG:       u16 = 0x3CF;
const CRTC_ADDR_REG:           u16 = 0x3D4;
const CRTC_DATA_REG:           u16 = 0x3D5;
const INPUT_STATUS_REG:        u16 = 0x3DA;
const INSTAT_READ_REG:         u16 = 0x3DA;

// ASCII Printable
// Backspace
// New Line
// Carriage Return
// Extended ASCII Printable
pub fn is_printable(c: u8) -> bool {
    matches!(c, 0x20..=0x7E | 0x08 | 0x0A | 0x0D | 0x80..=0xFF)
}

// 0x00 -> top
// 0x0F -> bottom
// 0x1F -> max (invisible)
fn set_underline_location(location: u8) {
    int::without_interrupts(|| {
        unsafe {
            outb(CRTC_ADDR_REG, 0x14); // Underline Location Register
            outb(CRTC_DATA_REG, location);
        }
    })
}

fn disable_underline() {
    set_underline_location(0x1F);
}

fn disable_blinking() {
    int::without_interrupts(|| {
        let reg = 0x10; // Attribute Mode Control Register
        let mut attr = get_attr_ctrl_reg(reg);
        attr.set_bit(3, false); // Clear "Blinking Enable" bit
        set_attr_ctrl_reg(reg, attr);
    })
}

fn set_attr_ctrl_reg(index: u8, value: u8) {
    int::without_interrupts(|| {
        unsafe {
            inb(INPUT_STATUS_REG); // Reset to address mode
            let tmp = inb(ATTR_ADDR_REG);
            outb(ATTR_ADDR_REG, index);
            outb(ATTR_ADDR_REG, value);
            outb(ATTR_ADDR_REG, tmp);
        }
    })
}

fn get_attr_ctrl_reg(index: u8) -> u8 {
    int::without_interrupts(|| {
        let index = index | 0x20; // Set "Palette Address Source" bit
        unsafe {
            inb(INPUT_STATUS_REG); // Reset to address mode
            let tmp = inb(ATTR_ADDR_REG);
            outb(ATTR_ADDR_REG, index);
            let res = inb(ATTR_READ_REG);
            outb(ATTR_ADDR_REG, tmp);
            res
        }
    })
}

pub fn init() {
    // Map palette registers to color registers
    set_attr_ctrl_reg(0x0, 0x00);
    set_attr_ctrl_reg(0x1, 0x01);
    set_attr_ctrl_reg(0x2, 0x02);
    set_attr_ctrl_reg(0x3, 0x03);
    set_attr_ctrl_reg(0x4, 0x04);
    set_attr_ctrl_reg(0x5, 0x05);
    set_attr_ctrl_reg(0x6, 0x14);
    set_attr_ctrl_reg(0x7, 0x07);
    set_attr_ctrl_reg(0x8, 0x38);
    set_attr_ctrl_reg(0x9, 0x39);
    set_attr_ctrl_reg(0xA, 0x3A);
    set_attr_ctrl_reg(0xB, 0x3B);
    set_attr_ctrl_reg(0xC, 0x3C);
    set_attr_ctrl_reg(0xD, 0x3D);
    set_attr_ctrl_reg(0xE, 0x3E);
    set_attr_ctrl_reg(0xF, 0x3F);

    screen::set_text_mode();
    WRITER.lock().clear_screen();
    // Load the TUI-OS "Ink & Brass" brand palette (reprograms the DAC).
    // Heap-free: this runs before the kernel allocator is ready.
    palette::set_brand_palette();
}

// --- TUI backend support (used by the ratatui VGA backend) ---------------
//
// These helpers give the TUI renderer direct access to the text-mode
// screen buffer (0xB8000) so it can draw arbitrary cells with the Ratatui
// diffing engine, bypassing the ANSI/vte console writer.

pub const TUI_WIDTH: usize = 80;
pub const TUI_HEIGHT: usize = 25;

/// Prepare the screen for TUI rendering: ensure text mode, disable
/// blinking (so bright backgrounds work) and clear the screen.
pub fn tui_init() {
    disable_blinking();
    screen::set_text_mode();
    WRITER.lock().clear_screen();
    // Re-entering text mode may restore the BIOS palette: reload the brand
    // palette so the TUI always renders with the "Ink & Brass" colors.
    palette::set_brand_palette();
}

/// Draw one cell at `(x, y)`. `fg`/`bg` are VGA color indexes (0-15).
pub fn tui_set_cell(x: usize, y: usize, c: u8, fg: u8, bg: u8) -> bool {
    WRITER.lock().set_cell(
        x,
        y,
        c,
        color::Color::from_vga_index(fg as usize),
        color::Color::from_vga_index(bg as usize),
    )
}

pub fn tui_fill_screen(c: u8, fg: u8, bg: u8) {
    WRITER.lock().fill_screen(
        c,
        color::Color::from_vga_index(fg as usize),
        color::Color::from_vga_index(bg as usize),
    );
}

pub fn tui_clear_screen() {
    WRITER.lock().clear_screen();
}

pub fn tui_hide_cursor() {
    WRITER.lock().tui_disable_cursor();
}

pub fn tui_show_cursor() {
    WRITER.lock().tui_enable_cursor();
}

pub fn tui_set_cursor_position(x: usize, y: usize) {
    WRITER.lock().tui_set_cursor_position(x, y);
}

pub fn tui_cursor_position() -> (usize, usize) {
    WRITER.lock().tui_cursor_position()
}

#[doc(hidden)]
pub fn print_fmt(args: fmt::Arguments) {
    int::without_interrupts(||
        WRITER.lock().write_fmt(args).expect("Could not print to VGA")
    )
}
