//! The built-in userspace replacement: the TUI desktop.
//!
//! TUI-OS used to ship a full userspace (bins, scripts, network daemons) on
//! top of the kernel. This fork keeps only the kernel and the bootloader and
//! replaces the whole userland with a built-in desktop that renders its
//! windows with [`ratatui`] directly into the VGA text buffer.

pub mod shell;
pub mod tui;