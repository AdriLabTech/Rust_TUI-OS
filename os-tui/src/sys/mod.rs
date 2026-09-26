#[macro_export]
macro_rules! printk {
    ($($arg:tt)*) => ({
        $crate::sys::console::print_fmt(format_args!($($arg)*));
    });
}

#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => ({
        // Gated the same way `log!` is, and for the same reason. `make test` runs
        // with no `-drive`, so `Drive::open(0,0)` succeeds with a made-up
        // geometry and every block read reports an ATA error: this macro alone
        // was responsible for 4,193,465 lines of output, which buries the one
        // line of test results that matters. A test run should read as a test
        // run.
        if !cfg!(test) {
            let csi_color = $crate::api::console::Style::color("blue");
            let csi_reset = $crate::api::console::Style::reset();
            $crate::sys::console::print_fmt(format_args!(
                "{}DEBUG: {}{}\n", csi_color, format_args!($($arg)*), csi_reset
            ));
        }
    });
}

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => ({
        if !cfg!(test) {
            let time = $crate::sys::clk::boot_time();
            let csi_color = $crate::api::console::Style::color("lime");
            let csi_reset = $crate::api::console::Style::reset();
            $crate::sys::console::print_fmt(format_args!(
                "{}[{:.6}]{} {}\n",
                csi_color, time, csi_reset, format_args!($($arg)*)
            ));

            let time = $crate::sys::clk::epoch_time();
            $crate::sys::log::write_fmt(format_args!(
                "[{:.6}] {}\n",
                time, format_args!($($arg)*)
            ));
        }
    });
}

pub mod acpi;
pub mod ata;
pub mod boot;
pub mod clk;
pub mod console;
pub mod cpu;
pub mod fs;
pub mod gdt;
pub mod idt;
pub mod keyboard;
pub mod log;
pub mod mem;
pub mod net;
pub mod pci;
pub mod pic;
pub mod process;
pub mod rng;
pub mod serial;
pub mod snd;
pub mod speaker;
pub mod syscall;
pub mod tss;
pub mod vga;
pub mod x86;
