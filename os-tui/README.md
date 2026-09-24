# TUI-OS: a 100% Rust OS with a TUI desktop over VGA text

TUI-OS is a self-contained operating system in which **everything lives in the
kernel crate**: there is no userspace. It boots a PS/2 keyboard, the RTC clock,
a serial port, and a **TUI desktop** that renders **ratatui 0.30 windows
directly into the VGA 80×25 text buffer** (`0xB8000`) through a custom
`VgaBackend`.

```
 _____ _   _ ___       ___  ____
|_   _| | | |_ _|     / _ \/ ___|   TUI-OS v0.1.0  ░  a 100% Rust OS
  | | | | | || |_____| | | \___ \   ratatui 0.30 -> VGA text 80x25
  | | | |_| || |_____| |_| |___) |   amd64 · kernel + bootloader + desktop
  |_|  \___/|___|     \___/|____/
```

## What is the desktop

On boot you land on the **desktop**: a dock at the bottom lets you launch apps
fullscreen

| App | What it does |
| --- | --- |
| **Archivos** | Three-panel file manager (parent / current / info+preview) with create, rename, delete, copy and move |
| **Terminal** | An in-kernel shell with a functional command set (`ls`, `cd`, `cat`, `mkdir`, `rm`, `mv`, `cp`, `mem`, `uptime`, `date`, `abrir`, …) |
| **Sistema** | Live CPU/RAM info with a memory gauge and sparkline |
| **Ayuda** | Command list, key bindings, and apps of the dock |
| **Apagar** | ACPI power down (also `halt` / `reboot` from the terminal) |

The UI language is Spanish; command names follow the usual Unix conventions.
The first boot formats the ATA disk and seeds a few welcome files, so the file
manager and `ls` have content right away.

## Setup

You need `git`, `gcc`, `make`, `curl`, `qemu-img`, `qemu-system-x86_64`,
`python3` and a recent Rust **nightly** toolchain (see `rust-toolchain.toml`)
with the `bootimage` cargo subcommand:

    $ curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain none
    $ rustup show
    $ cargo install bootimage

## Build and run in QEMU

Build the bootable disk image:

    $ make image        # -> target/x86_64-tuios/release/bootimage-tuios.bin

Run it in QEMU with a window, or headless with the unix-socket monitor:

    $ make qemu         # windowed; add monitor=true for a telnet console
    # headless, with a unix-socket monitor:
    $ qemu-system-x86_64 \
        -name "TUI-OS" -m 32 -smp 2 -cpu core2duo \
        -drive file=target/x86_64-tuios/release/bootimage-tuios.bin,format=raw \
        -monitor unix:/tmp/tuios-mon.sock,server,nowait \
        -display none -serial file:/tmp/tuios-serial.log

With `-display none` there is no GUI; drive the keyboard through the QEMU
monitor. **`sendkey` accepts exactly one key per command** — burst them as
separate commands instead of a comma/space list:

    (monitor) sendkey a
    (monitor) sendkey r
    (monitor) sendkey c
    (monitor) sendkey h
    (monitor) sendkey i
    ...
    (monitor) sendkey ret

Key names: `ret`, `tab`, `spc`, `backspace`, `f1`–`f12`, `up`/`down`/
`left`/`right`. Screenshots of the text buffer (80×25 grid) can be read back
with the PDF/QA scripts under `/tmp/opencode` (`qemu-dump.py` + `xp2text.py`).
The kernel drains the whole 8042 output buffer inside a single IRQ1 (cap 32
bytes, iowait between reads), which keeps fast keyboard bursts working on both
QEMU and real hardware.

## Run on real hardware

Boots on x86-64 machines from ~2005–2020 with **BIOS/CSM** boot enabled (UEFI
is not supported). Write the image to a USB stick and boot it:

    $ sudo dd if=target/x86_64-tuios/release/bootimage-tuios.bin of=/dev/sdX bs=4M conv=fsync

`sdX` is your USB device — **double-check the device name, `dd` will overwrite
it**. On the machine, enable Legacy/CSM boot and select the USB stick.
Keyboard is PS/2 (default `qwerty` layout; override at build time with
`make image keyboard=azerty`).

## Development

    $ make test        # build + boot the image in QEMU and run the kernel tests

## License

MIT. The original copyright and license are preserved in `LICENSE`.
