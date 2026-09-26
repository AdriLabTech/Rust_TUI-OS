# TUI-OS: a 100% Rust OS with a TUI desktop over VGA text

TUI-OS is a self-contained operating system in which **everything lives in the
kernel crate**: there is no userspace. It boots a PS/2 keyboard, the RTC clock,
a serial port, and a **TUI desktop** that renders **ratatui 0.30 widgets
directly into the VGA 80x25 text buffer** (`0xB8000`) through a custom
`VgaBackend`. There are no windows: there is a dock and fullscreen apps.

```
 _____ _   _ ___       ___  ____
|_   _| | | |_ _|     / _ \/ ___|   TUI-OS v0.1.0  ░  a 100% Rust OS
  | | | | | || |_____| | | \___ \   ratatui 0.30 -> VGA text 80x25
  | | | |_| || |_____| |_| |___) |   amd64 · kernel + bootloader + desktop
  |_|  \___/|___|     \___/|____/
```

## Project status

The desktop boots, draws its dock, dispatches apps, and seeds the disk on its
own. **The Files app is a scaffold.**

| Task | State |
| --- | --- |
| 1. `make test` builds and runs the kernel tests | Done |
| 2. Desktop framework: bars, wallpaper, app dispatch, key routing | Done |
| 3. Dock drawn along the bottom | Done |
| 4. Disk formatted and seeded at first boot | Done |
| 5. Terminal with the full command set | Done |
| 6. Three-panel Files app | Pending. Only `q` is handled |
| 7. CPU sparkline and full integration | Pending |

103 kernel tests and 21 tool tests pass in release and in debug, with no
warnings. The full work
plan is in
[`docs/superpowers/plans/2026-09-24-tuios-desktop.md`](docs/superpowers/plans/2026-09-24-tuios-desktop.md).

Booting lands in the terminal, fullscreen between the title bar and the status
bar. `F1` through `F5` open an app from anywhere; `Esc` or `F5` close it.

The dock is row 23: `▸ Archivos  Terminal  Sistema  Ayuda  ░  Apagar`, with `◀`
and `▶` to move, `Enter` to open and `Esc` to go back. The `▸` marks the
highlighted entry.

## The terminal

This is the centre of the system: a full shell living inside the kernel, with
the usual key bindings, history and completion, talking to the real filesystem
rather than a stand-in.

The command names follow the usual Unix conventions, with Spanish aliases.

| Group | Commands |
| --- | --- |
| Files | `pwd` `cd` `ls` `cat` `touch` `write` `mkdir` `rm` `mv` `cp` |
| Apps | `help` `ayuda` `apps` `abrir` `sysinfo` |
| System | `mem` `uptime` `date` `version` `echo` `clear` `random` `pci` `halt` `apagar` `reboot` `reiniciar` |

`abrir` takes a dock name (`abrir Sistema`) and jumps to that app.

`write` takes everything after the file name as the text, so spaces need no
quoting: `write /nota.txt hola mundo`. It overwrites, the way `>` would, and says
in Spanish when the target is a directory or cannot be written.

The prompt line has history on `↑` and `↓`, editing with `←`, `→`, `Home` and
`End`, `Delete` and `Backspace`, and `Tab` completes the command names that
start with what you have already typed (as long as you have not typed a space).

The file commands are not toys: `cat` reads through the real MFS walk, `cp`
copies in chunks via `FileIO` so binaries do not get corrupted, and `mv` moves
within the same disk. `cd` moves the process working directory, `rm` deletes
files, `ls` sorts by name instead of by storage order, and `pci` lists the
hardware by reading `DeviceConfig` fields.

Nothing panics. Every command that cannot do what it was asked writes a Spanish
message on the terminal.

### What the shell does not do yet

- **No redirection.** `>` does not exist, but `write <file> <text>` covers the
  case: it writes the text and overwrites. What is still impossible is real
  redirection, because there are no pipes.
- **`..` does not work.** MFS resolves a path by walking directory entries and
  has no `.` or `..` entries, so `cd ..` and `cat ../notes.txt` fail. Relative
  paths do work: inside `/home`, `mkdir relative` creates `/home/relative`.
- **`rm` refuses directories.** There is no `rm -r`.
- **No pipes.** No `|`, no `&&`.

## Apps

| App | What it does | State |
| --- | --- | --- |
| **Terminal** | In-kernel shell with 27 commands | Works |
| **Sistema** | Live CPU/RAM with a memory gauge | Works. The activity sparkline is Task 7 |
| **Ayuda** | Command list, key bindings, dock apps | Works |
| **Archivos** | Three-panel file manager (parent / current / info+preview) with create, rename, delete, copy, move | Scaffold. Task 6 is missing |
| **Apagar** | ACPI power down | Works. Also `halt`, `apagar`, `reboot`, `reiniciar` from the terminal |

The UI language is Spanish.

## The filesystem

MFS (`src/sys/fs/`) is a hierarchical filesystem with absolute and relative
paths, a per-process working directory, nested subdirectories, files and
devices. It lives in the MFS superblock on the ATA disk, at offset 4 MB.

**On first boot** the kernel scans the ATA drives for a superblock. If it finds
none, it mounts the first drive, formats it, and seeds a welcome tree:

```
/bienvenida.txt   /manual.txt   /usr/README.txt
/usr   /home   /tmp   /etc
```

Seeding is idempotent: a tree that already holds `bienvenida.txt` is left
alone, so a second boot leaves the image byte-identical.

## Setup

You need `git`, `gcc`, `make`, `curl`, `qemu-img`, `qemu-system-x86_64`,
`python3` and a recent Rust **nightly** toolchain (see `rust-toolchain.toml`)
with the `bootimage` cargo subcommand:

    $ curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain none
    $ rustup show
    $ cargo install bootimage

## Build and run in QEMU

Build the bootable disk image (32 MB, created if missing):

    $ make image

Run it in a window, or headless with the unix-socket monitor:

    $ make qemu         # windowed; add monitor=true for a telnet console
    $ qemu-system-x86_64 \
        -name "TUI-OS" -m 32 -smp 2 -cpu core2duo \
        -drive file=disk.img,format=raw \
        -monitor unix:/tmp/tuios-mon.sock,server,nowait \
        -display none -serial file:/tmp/tuios-serial.log

### Reading the screen and typing

`tools/vgatext.py` boots QEMU headless, waits for the desktop, sends keys, and
dumps the 80x25 VGA text buffer as readable text:

    $ make dump KEYS="l s ret"

With no keys it only dumps the screen, and the dump goes to stdout.

**QEMU's `sendkey` takes exactly one key per command**, so burst them as
separate commands:

    (monitor) sendkey a
    (monitor) sendkey r
    (monitor) sendkey c
    (monitor) sendkey h
    (monitor) sendkey i
    (monitor) sendkey ret

Key names that work: bare letters and digits, `spc` (the space), `ret`, `tab`,
`backspace`, `esc`, `slash`, `dot`, `minus`, `f1`-`f12`,
`up`/`down`/`left`/`right`.

Two names that **do not** exist and answer `invalid parameter`: `space` (it is
`spc`) and `escape` (it is `esc`). `make dump` exits 2 and says so on the last
line, instead of typing nothing and leaving a dump that looks fine.

**The space is `spc`, not `space`.** `sendkey space` answers `invalid parameter`
because that is not QEMU's name for it, but `spc` types a space perfectly.
`make dump KEYS="..."` splits the keys on whitespace, so `spc` is exactly the
token to write:

    $ make dump KEYS="e c h o spc h o l a ret"
    ...
    > echo hola
    hola

The kernel drains the whole 8042 output buffer inside a single IRQ1 (cap 32
bytes, iowait between reads), which keeps fast keyboard bursts working on both
QEMU and real hardware.

### Changing the keyboard layout

The layout is compiled into the kernel, so changing it means rebuilding:

    $ make image keyboard=azerty
    $ make image keyboard=dvorak
    $ make image keyboard=qwerty   # the default

A name that is not one of those three stops `make` with an error, instead of
producing an image whose keyboard does not work.

## Run on real hardware

Boots on x86-64 machines from ~2005-2020 with **BIOS/CSM** boot enabled (UEFI
is not supported). Write the image to a USB stick and boot it:

    $ sudo dd if=target/x86_64-tuios/release/bootimage-tuios.bin of=/dev/sdX bs=4M conv=fsync

`sdX` is your USB device. **Double-check the device name, `dd` will overwrite
it.** On the machine, enable Legacy/CSM boot and select the USB stick. The
keyboard is PS/2. The layout is compiled into the image, so a non-qwerty
build has to be rebuilt with `make image keyboard=azerty`.

## Development

    $ make test              # release, 103 tests
    $ make test mode=debug   # debug: enables debug_assert, which catches more

And the tests for the tool that reads the screen back:

    $ cd tools && python3 -m unittest test_vgatext

Both forms build and boot the real image in QEMU. Run the debug one too: several
bugs in this repo only show up with `debug_assert` active, because release
compiles them out and the kernel degrades in silence.

## License

MIT. The original copyright and license are preserved in `LICENSE`.
