//! The boot entry point kept at its original path.
//!
//! The desktop now owns the screen; this module stays so that
//! `usr::shell::main` — the call in `lib.rs::exec` — keeps working, and so the
//! old module path does not become a second way in.

/// Launch the desktop. This function never returns.
pub fn main(_args: &[&str]) -> ! {
    crate::usr::desktop::main()
}
