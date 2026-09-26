use crate::sys;

use crate::sys::fs::{FileIO, IO};

use crate::sys::x86::int;
use crate::sys::x86::port::*;

use alloc::collections::vec_deque::VecDeque;
use alloc::format;
use alloc::string::String;
use core::sync::atomic::{AtomicBool, Ordering};
use lazy_static::lazy_static;
use pc_keyboard::{
    layouts, DecodedKey, Error, HandleControl, KeyCode, KeyEvent, KeyState,
    Keyboard, ScancodeSet1,
};
use spin::Mutex;

lazy_static! {
    pub static ref BUF: Mutex<VecDeque<u8>> = Mutex::new(VecDeque::new());
    // Raw key events (KeyEvent { code, state }) decoded by pc-keyboard.
    // The TUI shell reads from this queue to react to arrows, function
    // keys, tabs, etc. while the character buffer (STDIN) keeps working
    // for the legacy text-mode console.
    pub static ref KEYS: Mutex<VecDeque<KeyEvent>> = Mutex::new(VecDeque::new());
    // Fully decoded key events (letters, digits, arrows, F-keys, ...).
    // The TUI shell reads from this queue so it receives characters as
    // they were mapped through the active keyboard layout, without having
    // to run the layout decoder itself.
    pub static ref DECODED_KEYS: Mutex<VecDeque<DecodedKey>> = Mutex::new(VecDeque::new());
}

// Pop the next key event, or None if the queue is empty. Non-blocking.
pub fn try_pop_key() -> Option<KeyEvent> {
    int::without_interrupts(|| KEYS.lock().pop_front())
}

// Drop every pending key event.
pub fn drain_keys() {
    int::without_interrupts(|| KEYS.lock().clear());
}

// Pop the next decoded key event, or None if the queue is empty. Non-blocking.
pub fn try_pop_decoded_key() -> Option<DecodedKey> {
    int::without_interrupts(|| DECODED_KEYS.lock().pop_front())
}

// Drop every pending decoded key event.
pub fn drain_decoded_keys() {
    int::without_interrupts(|| DECODED_KEYS.lock().clear());
}

// Push a decoded key event into the queue used by the TUI shell.
fn push_decoded_key(key: DecodedKey) {
    let mut decoded = DECODED_KEYS.lock();
    if decoded.len() >= 256 {
        decoded.pop_front();
    }
    decoded.push_back(key);
}

// Push a key event into the queue used by the TUI shell.
fn push_key(event: KeyEvent) {
    let mut keys = KEYS.lock();
    if keys.len() >= 256 {
        keys.pop_front();
    }
    keys.push_back(event);
}

pub static KEYBOARD: Mutex<Option<KeyboardDecoder>> = Mutex::new(None);

pub static ALT: AtomicBool = AtomicBool::new(false);
pub static CTRL: AtomicBool = AtomicBool::new(false);
pub static SHIFT: AtomicBool = AtomicBool::new(false);

pub enum KeyboardDecoder {
    Azerty(Keyboard<layouts::Azerty, ScancodeSet1>),
    Dvorak(Keyboard<layouts::Dvorak104Key, ScancodeSet1>),
    Qwerty(Keyboard<layouts::Us104Key, ScancodeSet1>),
}

impl KeyboardDecoder {
    fn add_byte(&mut self, scancode: u8) -> Result<Option<KeyEvent>, Error> {
        match self {
            KeyboardDecoder::Azerty(kb) => kb.add_byte(scancode),
            KeyboardDecoder::Dvorak(kb) => kb.add_byte(scancode),
            KeyboardDecoder::Qwerty(kb) => kb.add_byte(scancode),
        }
    }

    fn process_keyevent(&mut self, event: KeyEvent) -> Option<DecodedKey> {
        match self {
            KeyboardDecoder::Azerty(kb) => kb.process_keyevent(event),
            KeyboardDecoder::Dvorak(kb) => kb.process_keyevent(event),
            KeyboardDecoder::Qwerty(kb) => kb.process_keyevent(event),
        }
    }

    fn from(name: &str) -> Option<Self> {
        match name {
            "azerty" => Some(KeyboardDecoder::Azerty(Keyboard::new(
                ScancodeSet1::new(),
                layouts::Azerty,
                HandleControl::MapLettersToUnicode,
            ))),
            "dvorak" => Some(KeyboardDecoder::Dvorak(Keyboard::new(
                ScancodeSet1::new(),
                layouts::Dvorak104Key,
                HandleControl::MapLettersToUnicode,
            ))),
            "qwerty" => Some(KeyboardDecoder::Qwerty(Keyboard::new(
                ScancodeSet1::new(),
                layouts::Us104Key,
                HandleControl::MapLettersToUnicode,
            ))),
            _ => None,
        }
    }
}

fn set_keyboard(layout: &str) -> bool {
    if let Some(keyboard) = KeyboardDecoder::from(layout) {
        int::without_interrupts(||
            *KEYBOARD.lock() = Some(keyboard)
        );
        true
    } else {
        false
    }
}

/// The layout the decoder is running right now, read from the live decoder
/// rather than from `option_env!` a second time, so it reports the driver's
/// actual state instead of restating the build variable. `"none"` means `init`
/// was handed a name the decoder does not know, which is a machine with no
/// keyboard: `set_keyboard` refused it and `init` ignored the refusal.
pub fn layout_name() -> &'static str {
    match *KEYBOARD.lock() {
        Some(KeyboardDecoder::Azerty(_)) => "azerty",
        Some(KeyboardDecoder::Dvorak(_)) => "dvorak",
        Some(KeyboardDecoder::Qwerty(_)) => "qwerty",
        None => "none",
    }
}

#[derive(Debug, Clone)]
pub struct KeyboardLayout;

impl KeyboardLayout {
    pub fn new() -> Self {
        Self {}
    }

    pub fn size() -> usize {
        16
    }
}

impl FileIO for KeyboardLayout {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, ()> {
        int::without_interrupts(|| {
            let layout = match layout_name() {
                "none" => return Err(()),
                name => name,
            };
            let n = layout.len();
            if n > buf.len() {
                return Err(());
            }
            buf[0..n].copy_from_slice(layout.as_bytes());
            Ok(n)
        })
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize, ()> {
        if let Ok(layout) = String::from_utf8(buf.to_vec()) {
            if set_keyboard(layout.trim()) {
                return Ok(buf.len());
            }
        }
        Err(())
    }

    fn close(&mut self) {}

    fn poll(&mut self, event: IO) -> bool {
        int::without_interrupts(||
            match event {
                IO::Read => true,
                IO::Write => true,
            }
        )
    }
}

fn read_scancode() -> u8 {
    unsafe { inb(0x60) }
}

// True when the 8042 output buffer holds a scancode waiting to be read.
// Check the status register (port 0x64) before reading the data register
// (port 0x60): on an empty buffer the data register can return stale or
// garbage bytes that would corrupt the keyboard decoder state.
fn scancode_available() -> bool {
    unsafe { inb(0x64) & 1 == 1 }
}

fn send_key(c: char) {
    sys::console::key_handle(c);
}

fn send_csi(code: &str) {
    send_key('\x1B'); // ESC
    send_key('[');
    for c in code.chars() {
        send_key(c);
    }
}

#[derive(Debug, Clone)]
pub struct KeyboardBuffer;

impl KeyboardBuffer {
    pub fn new() -> Self {
        int::without_interrupts(|| BUF.lock().clear());
        Self {}
    }

    pub fn size() -> usize {
        4
    }
}

impl FileIO for KeyboardBuffer {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, ()> {
        int::without_interrupts(||
            if let Some(scancode) = BUF.lock().pop_front() {
                buf[0] = scancode;
                Ok(1)
            } else {
                Ok(0)
            }
        )
    }

    fn write(&mut self, _buf: &[u8]) -> Result<usize, ()> {
        Err(())
    }

    fn close(&mut self) {}

    fn poll(&mut self, event: IO) -> bool {
        int::without_interrupts(||
            match event {
                IO::Read => !BUF.lock().is_empty(),
                IO::Write => false,
            }
        )
    }
}

fn interrupt_handler() {
    if let Some(ref mut keyboard) = *KEYBOARD.lock() {
        // Drain the whole 8042 output buffer within this single interrupt.
        // With an edge-triggered PIC, QEMU's i8042 only asserts one IRQ1
        // edge per burst of scancodes; any byte left in the buffer after
        // this ISR would sit there until the controller line goes low and
        // high again, so it must be consumed here. On real hardware this is
        // harmless: the loop simply reads the bytes that were already queued.
        let mut handled = 0;
        while scancode_available() && handled < 32 {
            handled += 1;
            let scancode = read_scancode();

            let mut buf = BUF.lock();
            if buf.len() > 256 {
                buf.pop_front();
            }
            buf.push_back(scancode);

            if let Ok(Some(event)) = keyboard.add_byte(scancode) {
                push_key(event.clone()); // Feed the TUI shell key-event queue

                let ord = Ordering::Relaxed;
                match event.code {
                    KeyCode::LAlt | KeyCode::RAltGr => {
                        ALT.store(event.state == KeyState::Down, ord)
                    }
                    KeyCode::LShift | KeyCode::RShift => {
                        SHIFT.store(event.state == KeyState::Down, ord)
                    }
                    KeyCode::LControl | KeyCode::RControl => {
                        CTRL.store(event.state == KeyState::Down, ord)
                    }
                    _ => {}
                }
                let is_alt = ALT.load(ord);
                let is_ctrl = CTRL.load(ord);
                let is_shift = SHIFT.load(ord);
                if let Some(key) = keyboard.process_keyevent(event) {
                    push_decoded_key(key); // Feed the TUI shell decoded-key queue
                    match key {
                        // Ctrl + Alt + Del
                        DecodedKey::Unicode('\u{7f}') if is_alt && is_ctrl => {
                            sys::idt::reset();
                        }

                        // [Ctrl +] [Shift +] Tab
                        DecodedKey::Unicode('\t') => {
                            if is_ctrl {
                                if is_shift {
                                    send_csi("1;6I")
                                } else {
                                    send_csi("1;5I")
                                }
                            } else {
                                if is_shift {
                                    send_csi("Z") // Backtab
                                } else {
                                    send_key('\t') // Tab
                                }
                            }
                        }

                        DecodedKey::RawKey(KeyCode::ArrowUp) => {
                            if is_ctrl {
                                send_csi("1;5A")
                            } else if is_alt {
                                send_csi("1;3A")
                            } else {
                                send_csi("A")
                            }
                        }

                        DecodedKey::RawKey(KeyCode::ArrowDown) => {
                            if is_ctrl {
                                send_csi("1;5B")
                            } else if is_alt {
                                send_csi("1;3B")
                            } else {
                                send_csi("B")
                            }
                        }

                        DecodedKey::RawKey(KeyCode::ArrowRight) => {
                            if is_ctrl {
                                send_csi("1;5C")
                            } else if is_alt {
                                send_csi("1;3C")
                            } else {
                                send_csi("C")
                            }
                        }

                        DecodedKey::RawKey(KeyCode::ArrowLeft) => {
                            if is_ctrl {
                                send_csi("1;5D")
                            } else if is_alt {
                                send_csi("1;3D")
                            } else {
                                send_csi("D")
                            }
                        }

                        DecodedKey::RawKey(KeyCode::PageUp) => send_csi("5~"),

                        DecodedKey::RawKey(KeyCode::PageDown) => send_csi("6~"),

                        DecodedKey::Unicode(c) => {
                            let letter = (c as u8 | 0x40) as char;
                            if is_ctrl && is_shift && letter.is_ascii_uppercase() {
                                // Ctrl + Shift + Letter
                                send_csi(&format!("1;6{}", letter));
                            } else {
                                send_key(c)
                            }
                        }

                        _ => {}
                    };
                }
            }

            // Give the 8042 a moment to clear the OBF flag before the next
            // status register read (real hardware needs a few microseconds).
            unsafe { outb(0x80, 0) }
        }
    }
}

pub fn init() {
    set_keyboard(option_env!("TUIOS_KEYBOARD").unwrap_or("qwerty"));
    sys::idt::set_irq_handler(sys::pic::KBD_IRQ, interrupt_handler);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `init` throws away `set_keyboard`'s result, so a layout name the
    /// decoder does not know leaves `KEYBOARD` as `None` and the machine with
    /// no keyboard at all, with no message anywhere. The Makefile rejects bad
    /// names, and this is the in-kernel half of the same guard.
    #[test_case]
    fn the_driver_is_running_a_known_layout_after_init() {
        let name = layout_name();
        assert!(
            matches!(name, "qwerty" | "azerty" | "dvorak"),
            "the driver came up with layout {:?}; a keyboard that is not one of \
             the three known layouts is a kernel with no keyboard at all",
            name
        );
    }

    /// Compares the layout the driver is actually running against the one the
    /// build asked for, so `make test keyboard=azerty` fails if the Makefile
    /// ever stops reaching the compiler. Under the default `qwerty` both sides
    /// are qwerty and the test is satisfied but not informative.
    #[test_case]
    fn the_compiled_layout_is_the_one_the_build_asked_for() {
        let expected = option_env!("TUIOS_KEYBOARD").unwrap_or("qwerty");
        assert_eq!(
            layout_name(),
            expected,
            "the driver is not running the layout this binary was built with"
        );
    }

    /// The list the Makefile validates `keyboard=` against, pinned here so the
    /// two cannot drift apart.
    #[test_case]
    fn the_three_named_layouts_are_accepted_and_others_are_not() {
        for name in ["qwerty", "azerty", "dvorak"] {
            assert!(
                KeyboardDecoder::from(name).is_some(),
                "the Makefile offers {} but the driver rejects it",
                name
            );
        }
        assert!(KeyboardDecoder::from("qwertz").is_none());
        assert!(KeyboardDecoder::from("").is_none());
    }
}
