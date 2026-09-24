use super::*;

use color::Color;

use crate::sys::fs::{FileIO, IO};

use alloc::boxed::Box;
use core::convert::TryFrom;
use spin::Mutex;

static PALETTE: Mutex<Option<Palette>> = Mutex::new(None);

const DEFAULT_COLORS: [(u8, u8, u8); 16] = [
    (0x00, 0x00, 0x00), // DarkBlack
    (0x00, 0x00, 0x80), // DarkBlue
    (0x00, 0x80, 0x00), // DarkGreen
    (0x00, 0x80, 0x80), // DarkCyan
    (0x80, 0x00, 0x00), // DarkRed
    (0x80, 0x00, 0x80), // DarkMagenta
    (0x80, 0x80, 0x00), // DarkYellow
    (0xC0, 0xC0, 0xC0), // DarkWhite
    (0x80, 0x80, 0x80), // BrightBlack
    (0x00, 0x00, 0xFF), // BrightBlue
    (0x00, 0xFF, 0x00), // BrightGreen
    (0x00, 0xFF, 0xFF), // BrightCyan
    (0xFF, 0x00, 0x00), // BrightRed
    (0xFF, 0x00, 0xFF), // BrightMagenta
    (0xFF, 0xFF, 0x00), // BrightYellow
    (0xFF, 0xFF, 0xFF), // BrightWhite
];

/// The TUI-OS brand palette ("Ink & Brass"): a warm, low-contrast, editorial
/// theme. Each of the 16 semantic VGA indexes keeps its role — Black is the
/// ink background, Blue the charcoal panels, LightCyan (11) the global brass
/// accent, LightRed (12) reserved for errors — but renders with the RGB below
/// instead of the garish IBM defaults. Every channel is a multiple of 4 so the
/// 6-bit VGA DAC reproduces the exact color (no rounding on `r >> 2`).
const TUIOS_COLORS: [(u8, u8, u8); 16] = [
    (0x0C, 0x0C, 0x10), // 0  Black        -> ink (desktop / log background)
    (0x18, 0x1C, 0x24), // 1  Blue         -> charcoal (title bar, dock, panels)
    (0x6C, 0x88, 0x78), // 2  Green        -> sage
    (0x54, 0x90, 0x98), // 3  Cyan         -> muted teal
    (0x94, 0x44, 0x40), // 4  Red          -> brick
    (0x70, 0x54, 0x7C), // 5  Magenta      -> plum
    (0x78, 0x60, 0x38), // 6  Brown        -> dark bronze
    (0xBC, 0xC4, 0xCC), // 7  Light Gray   -> body text
    (0x3C, 0x44, 0x50), // 8  Dark Gray    -> muted / disabled
    (0x68, 0x80, 0x9C), // 9  Light Blue   -> dusty steel
    (0x94, 0xAC, 0x88), // 10 Light Green  -> light sage
    (0xC8, 0xA0, 0x5C), // 11 Light Cyan   -> brass (global accent)
    (0xC0, 0x60, 0x58), // 12 Light Red    -> sand red (errors only)
    (0xA0, 0x80, 0xA4), // 13 Light Magenta -> mauve
    (0xD4, 0xB4, 0x78), // 14 Light Yellow -> pale brass
    (0xE8, 0xE4, 0xDC), // 15 White        -> warm white (headings)
];

#[derive(Debug, Clone)]
pub struct Palette {
    pub colors: Box<[(u8, u8, u8); 256]>,
}

impl Palette {
    pub fn new() -> Self {
        Self { colors: Box::new([(0, 0, 0); 256]) }
    }

    pub fn default() -> Self {
        let mut palette = Palette::new();
        for (i, (r, g, b)) in DEFAULT_COLORS.iter().enumerate() {
            let i = Color::from_vga_index(i).register();
            palette.colors[i] = (*r, *g, *b);
        }
        palette
    }

    /// The TUI-OS brand palette ("Ink & Brass"). Place each of the 16 semantic
    /// VGA indexes on its DAC register (mirroring [`Palette::default`]) so the
    /// whole UI — current shell and upcoming desktop — is recolored by loading
    /// this palette once at boot.
    pub fn tuios() -> Self {
        let mut palette = Palette::new();
        for (i, (r, g, b)) in TUIOS_COLORS.iter().enumerate() {
            let i = Color::from_vga_index(i).register();
            palette.colors[i] = (*r, *g, *b);
        }
        palette
    }

    pub fn read() -> Self {
        let mut palette = Palette::new();
        for i in 0..256 {
            palette.colors[i] = read_palette(i);
        }
        palette
    }

    pub fn write(&self) {
        for (i, (r, g, b)) in self.colors.iter().enumerate() {
            write_palette(i, *r, *g, *b);
        }
    }

    pub fn to_bytes(&self) -> [u8; 256 * 3] {
        let mut buf = [0; 256 * 3];
        for (i, (r, g, b)) in self.colors.iter().enumerate() {
            buf[i * 3 + 0] = *r;
            buf[i * 3 + 1] = *g;
            buf[i * 3 + 2] = *b;
        }
        buf
    }

    pub fn size() -> usize {
        256 * 3
    }
}

impl TryFrom<&[u8]> for Palette {
    type Error = ();

    fn try_from(buf: &[u8]) -> Result<Self, Self::Error> {
        if buf.len() != Palette::size() {
            return Err(());
        }
        let mut colors = Box::new([(0, 0, 0); 256]);
        for (i, rgb) in buf.chunks(3).enumerate() {
            colors[i] = (rgb[0], rgb[1], rgb[2])
        }

        Ok(Palette { colors })
    }
}

impl FileIO for VgaPalette {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, ()> {
        let res = Palette::read().to_bytes();
        if buf.len() < res.len() {
            return Err(());
        }
        buf.clone_from_slice(&res);
        Ok(res.len())
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize, ()> {
        let palette = Palette::try_from(buf)?;
        palette.write();
        Ok(buf.len())
    }

    fn close(&mut self) {}

    fn poll(&mut self, event: IO) -> bool {
        match event {
            IO::Read => true,
            IO::Write => true,
        }
    }
}

fn write_palette(i: usize, r: u8, g: u8, b: u8) {
    int::without_interrupts(||
        WRITER.lock().set_palette(i, r, g, b)
    )
}

fn read_palette(i: usize) -> (u8, u8, u8) {
    int::without_interrupts(||
        WRITER.lock().palette(i)
    )
}

pub fn restore_palette() {
    if let Some(ref palette) = *PALETTE.lock() {
        palette.write();
    }
}

pub fn backup_palette() {
    *PALETTE.lock() = Some(Palette::read())
}

/// Write the "Ink & Brass" brand palette straight to the DAC, without any
/// heap allocation (unlike [`Palette::tuios`], which boxes a 256-entry
/// buffer). Safe to call during early init, before the kernel heap exists.
pub fn set_brand_palette() {
    for (i, (r, g, b)) in TUIOS_COLORS.iter().enumerate() {
        let i = Color::from_vga_index(i).register();
        write_palette(i, *r, *g, *b);
    }
}

#[test_case]
fn test_tuios_palette_registers() {
    let palette = Palette::tuios();
    // Index 0 (Black) -> ink background, on the DarkBlack register (0x00).
    assert_eq!(palette.colors[Color::DarkBlack.register()], TUIOS_COLORS[0]);
    // Index 1 (Blue) -> charcoal panels, on the DarkBlue register (0x01).
    assert_eq!(palette.colors[Color::DarkBlue.register()], TUIOS_COLORS[1]);
    // Index 6 (Brown) -> dark bronze, on the DarkYellow register (0x14).
    assert_eq!(palette.colors[Color::DarkYellow.register()], TUIOS_COLORS[6]);
    // Index 11 (LightCyan) -> brass accent, on the BrightCyan register (0x3B).
    assert_eq!(palette.colors[Color::BrightCyan.register()], TUIOS_COLORS[11]);
    // Index 12 (LightRed) -> sand red, on the BrightRed register (0x3C).
    assert_eq!(palette.colors[Color::BrightRed.register()], TUIOS_COLORS[12]);
    // Index 15 (White) -> warm white, on the BrightWhite register (0x3F).
    assert_eq!(palette.colors[Color::BrightWhite.register()], TUIOS_COLORS[15]);
}
