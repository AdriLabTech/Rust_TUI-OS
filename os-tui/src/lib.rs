#![no_std]
#![cfg_attr(test, no_main)]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]
#![feature(custom_test_frameworks)]
#![test_runner(crate::test::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

#[macro_use]
pub mod api;

#[macro_use]
pub mod sys;

pub mod usr;

#[cfg(test)]
mod test;

use sys::boot::MemoryMap;

pub const KERNEL_SIZE: usize = 4 << 20; // 4 MB

// NOTE: The stack size for the bootloader crate is set in Cargo.toml
pub const STACK_SIZE: usize = 256 << 10; // 256 KB

#[cfg(target_arch = "x86")]
const ARCH: &str = "i686";

#[cfg(target_arch = "x86_64")]
const ARCH: &str = "amd64";

pub fn init(memory_map: &MemoryMap, offset: u64) {
    sys::vga::init();
    sys::gdt::init();
    sys::idt::init();
    sys::pic::init();

    sys::x86::int::enable_interrupts();

    sys::serial::init();
    sys::keyboard::init();
    sys::clk::init();

    let v = option_env!("TUIOS_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"));
    log!("SYS TUI-OS v{} {}", v, ARCH);

    sys::mem::init(memory_map, offset);
    sys::cpu::init();
    sys::acpi::init(); // Require MEM
    sys::rng::init();
    sys::pci::init(); // Require MEM
    sys::snd::init();
    sys::net::init(); // Require PCI
    sys::ata::init();
    sys::fs::init(); // Require ATA
    sys::process::init();

    log!("RTC {}", sys::clk::date());
}

pub fn exec() -> ! {
    // No userland filesystem, no boot script: boot straight into the
    // built-in ratatui shell, which owns the VGA text buffer from now on.
    usr::shell::main(&[])
}

pub fn hang() -> ! {
    loop {
        sys::x86::hlt();
    }
}

#[allow(dead_code)]
#[cfg_attr(not(feature = "userspace"), alloc_error_handler)]
fn alloc_error_handler(layout: alloc::alloc::Layout) -> ! {
    let csi_color = api::console::Style::color("red");
    let csi_reset = api::console::Style::reset();
    printk!(
        "{}Error:{} Could not allocate {} bytes\n",
        csi_color,
        csi_reset,
        layout.size()
    );
    hang();
}

#[test_case]
fn test_lib() {
    assert_eq!(1, 1); // Trivial assertion
}
