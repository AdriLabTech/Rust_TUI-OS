//! Pure parsing of the two VBE data structures. No hardware, no BIOS: given the
//! bytes, produce the values. Everything here is testable in isolation, which is
//! why the mode enumeration lives in its own module.
//!
//! The offsets are not from memory, they were read off two independent
//! references (the VBE 2.0 layout as given by iPXE's `vesafb.h` and by StuBS's
//! `VbeModeInfo`) and then cross-checked against the structures' own sizes:
//! the mode info's last field ends at 0x100 and the controller info's at 0x100,
//! so a field in the wrong place would not add up.

extern crate alloc;

#[cfg(test)]
use alloc::vec;
use alloc::vec::Vec;

/// One entry of the VBE mode info structure, already decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeInfo {
    pub mode_number: u16,
    pub width: u16,
    pub height: u16,
    /// The row pitch VBE reports, in bytes. Not `width * 4`: modes are padded,
    /// and rasterising with a derived pitch shears the screen.
    pub bytes_per_line: u16,
    pub bpp: u8,
    pub phys_base_ptr: u32,
    /// The mode has a linear framebuffer.
    ///
    /// Which attribute bit says so is not settled by the references: the VBE 2.0
    /// layout says bit 7 (`0x0080`), and iPXE's own `VBE_MODE_LINEAR` macro
    /// says bit 14 (`0x4000`). Both are accepted rather than guessing, and
    /// `vbe_attributes_of_a_real_mode` measures which one this BIOS actually
    /// sets so the question gets answered instead of papered over.
    pub has_lfb: bool,
}

/// The attribute bit that means "mode is supported in hardware".
const ATTR_SUPPORTED: u16 = 0x0001;
/// Bit 7, per the VBE 2.0 mode attribute table.
const ATTR_LFB_V2: u16 = 0x0080;
/// Bit 14, per iPXE's `VBE_MODE_LINEAR`.
const ATTR_LFB_ALT: u16 = 0x4000;

fn le16(buf: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([buf[at], buf[at + 1]])
}

fn le32(buf: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([buf[at], buf[at + 1], buf[at + 2], buf[at + 3]])
}

/// Whether a 256-byte `VBE_INFO` block came from a VBE controller.
pub fn has_vesa(buf: &[u8]) -> bool {
    buf[controller_info::SIGNATURE..][..4] == *b"VESA"
}

/// The linear address of the video mode list, from the far pointer in a
/// `VBE_INFO` block.
///
/// The list is **not** inside the block; the block only points at it. And a
/// real-mode `segment:offset` is not a linear address, so the segment has to be
/// multiplied by 16 before the two are added -- using the offset on its own
/// lands in the wrong page. `None` when both halves are zero, which is the
/// interrupt vector table rather than a mode list.
pub fn mode_list_ptr(buf: &[u8]) -> Option<usize> {
    let segment = le16(buf, controller_info::MODE_LIST_PTR) as usize;
    let offset = le16(buf, controller_info::MODE_LIST_PTR + 2) as usize;
    if segment == 0 && offset == 0 {
        return None;
    }
    Some(segment * 16 + offset)
}

/// Video memory size in 64 KiB blocks, if the block reports one.
pub fn total_memory(buf: &[u8]) -> Option<u16> {
    match le16(buf, controller_info::TOTAL_MEMORY) {
        0 => None,
        blocks => Some(blocks),
    }
}

/// Decode a 256-byte video mode list, stopping at the `0xFFFF` terminator.
pub fn parse_mode_list(list: &[u8]) -> Vec<u16> {
    let mut modes = Vec::new();
    let mut at = 0;
    while at + 2 <= list.len() {
        let mode = le16(list, at);
        if mode == 0xFFFF {
            break;
        }
        modes.push(mode);
        at += 2;
    }
    modes
}

/// Decode a 256-byte `VBE_MODE_INFO` block fetched for `mode_number`, or `None`
/// when the BIOS says the mode is not supported.
pub fn parse_mode_info(buf: &[u8], mode_number: u16) -> Option<ModeInfo> {
    let attrs = le16(buf, mode_info::ATTRIBUTES);
    if attrs & ATTR_SUPPORTED == 0 {
        return None;
    }
    Some(ModeInfo {
        mode_number,
        width: le16(buf, mode_info::X_RESOLUTION),
        height: le16(buf, mode_info::Y_RESOLUTION),
        bytes_per_line: le16(buf, mode_info::BYTES_PER_LINE),
        bpp: buf[mode_info::BITS_PER_PIXEL],
        phys_base_ptr: le32(buf, mode_info::PHYS_BASE_PTR),
        has_lfb: attrs & (ATTR_LFB_V2 | ATTR_LFB_ALT) != 0,
    })
}

/// Field offsets within the 256-byte `VBE_MODE_INFO` structure.
mod mode_info {
    pub const ATTRIBUTES: usize = 0x00;
    pub const BYTES_PER_LINE: usize = 0x10;
    pub const X_RESOLUTION: usize = 0x12;
    pub const Y_RESOLUTION: usize = 0x14;
    pub const BITS_PER_PIXEL: usize = 0x19;
    pub const PHYS_BASE_PTR: usize = 0x28;
}

/// Field offsets within the 256-byte `VBE_INFO` controller block.
mod controller_info {
    pub const SIGNATURE: usize = 0x00;
    pub const MODE_LIST_PTR: usize = 0x0E;
    pub const TOTAL_MEMORY: usize = 0x12;
}

#[test_case]
fn has_vesa_is_false_without_the_signature() {
    // The signature is "VESA" as a little-endian u32 at offset 0. A block without
    // it is not VBE, and treating it as VBE means reading a mode list from
    // whatever the OEM string pointer happens to be.
    let mut buf = [0u8; 256];
    assert!(!has_vesa(&buf));

    buf[0..4].copy_from_slice(b"VESA");
    assert!(has_vesa(&buf));
}

#[test_case]
fn the_mode_list_is_a_far_pointer_not_a_field_in_the_block() {
    // The mode list is NOT inside the controller block. It lives somewhere else
    // in low memory and the block only carries a real-mode segment:offset to it.
    // Reading the list inline at some offset -- which is the natural mistake here
    // -- reads the OEM string pointer and the capabilities word as if they were
    // mode numbers.
    let mut buf = [0u8; 256];
    buf[0..4].copy_from_slice(b"VESA");
    buf[0x0E..0x10].copy_from_slice(&0xC000u16.to_le_bytes()); // segment
    buf[0x10..0x12].copy_from_slice(&0x0200u16.to_le_bytes()); // offset

    // 0xC000 * 16 + 0x0200 == 0xC0200. That is the conversion, and it matters:
    // a real-mode segment is not a linear address, so using the offset alone
    // would land in the wrong page.
    assert_eq!(mode_list_ptr(&buf), Some(0x000C_0200));
}

#[test_case]
fn a_controller_block_without_a_mode_list_pointer_reports_none() {
    // All zeroes means segment 0, offset 0, which is the interrupt vector table.
    // A mode list is never there, so this has to be refused rather than read.
    let buf = [0u8; 256];
    assert_eq!(mode_list_ptr(&buf), None);
}

#[test_case]
fn total_memory_reports_the_size_in_64k_blocks() {
    let mut buf = [0u8; 256];
    buf[0x12..0x14].copy_from_slice(&(16 * 1024u16).to_le_bytes());
    assert_eq!(total_memory(&buf), Some(16 * 1024));
}

#[test_case]
fn parse_mode_list_stops_at_the_terminator() {
    // 256 bytes of mode numbers, terminated by 0xFFFF. Whatever sits past the
    // terminator is not ours to read.
    let mut list = [0u8; 256];
    list[0..2].copy_from_slice(&0x0111u16.to_le_bytes());
    list[2..4].copy_from_slice(&0x0186u16.to_le_bytes());
    list[4..6].copy_from_slice(&0xFFFFu16.to_le_bytes());
    list[6..8].copy_from_slice(&0xDEADu16.to_le_bytes()); // past the end

    assert_eq!(parse_mode_list(&list), vec![0x0111, 0x0186]);
}

#[test_case]
fn parse_mode_list_of_a_terminator_first_list_is_empty() {
    // Some controllers report a version that supports no modes. Reading one
    // garbage mode number out of the reserved area and trying to set it is worse
    // than admitting there is nothing to choose from.
    let mut list = [0u8; 256];
    list[0..2].copy_from_slice(&0xFFFFu16.to_le_bytes());
    assert_eq!(parse_mode_list(&list), Vec::new());
}

#[test_case]
fn parse_mode_info_reads_geometry_stride_and_the_lfb_flag() {
    // Bits per pixel is its own byte at offset 0x19, NOT bits 25..27 of the
    // attributes word. Those bits do not exist: shifting a u16 left by 25 to
    // build them overflows, which is the compiler saying the layout does not
    // have room for them.
    let mut buf = [0u8; 256];
    buf[mode_info::ATTRIBUTES..][..2].copy_from_slice(&0x0091u16.to_le_bytes());
    buf[mode_info::BYTES_PER_LINE..][..2].copy_from_slice(&8000u16.to_le_bytes());
    buf[mode_info::X_RESOLUTION..][..2].copy_from_slice(&1920u16.to_le_bytes());
    buf[mode_info::Y_RESOLUTION..][..2].copy_from_slice(&1080u16.to_le_bytes());
    buf[mode_info::BITS_PER_PIXEL] = 32;
    buf[mode_info::PHYS_BASE_PTR..][..4].copy_from_slice(&0xFD00_0000u32.to_le_bytes());

    let mode = parse_mode_info(&buf, 0x01A4).expect("mode is supported");
    assert_eq!(mode.mode_number, 0x01A4);
    assert_eq!((mode.width, mode.height), (1920, 1080));
    assert_eq!(mode.bpp, 32);
    assert_eq!(mode.phys_base_ptr, 0xFD00_0000);
    assert!(mode.has_lfb);
}

#[test_case]
fn parse_mode_info_keeps_the_stride_the_bios_reported() {
    // The stride is read, not derived. VBE pads rows, and rasterising with
    // `width * 4` instead of the real pitch shears the whole screen while still
    // looking almost right, which is the worst kind of bug to chase. The stride
    // here is deliberately not `width * 4`, so a derived value cannot pass.
    let mut buf = [0u8; 256];
    buf[mode_info::ATTRIBUTES..][..2].copy_from_slice(&0x0091u16.to_le_bytes());
    buf[mode_info::BYTES_PER_LINE..][..2].copy_from_slice(&8000u16.to_le_bytes());
    buf[mode_info::X_RESOLUTION..][..2].copy_from_slice(&1920u16.to_le_bytes());
    buf[mode_info::BITS_PER_PIXEL] = 32;

    let mode = parse_mode_info(&buf, 0x01A4).expect("mode is supported");
    assert_eq!(mode.bytes_per_line, 8000);
    assert_ne!(mode.bytes_per_line, mode.width * 4, "the fixture must be padded");
}

#[test_case]
fn parse_mode_info_rejects_a_mode_the_bios_does_not_support() {
    // Bit 0 clear means "mode not supported". Reading geometry anyway would hand
    // back whatever zeroes happen to sit there.
    let buf = [0u8; 256];
    assert_eq!(parse_mode_info(&buf, 0x0111), None);
}
