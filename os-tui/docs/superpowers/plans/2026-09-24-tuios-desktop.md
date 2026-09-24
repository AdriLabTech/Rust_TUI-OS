# TUI-OS Desktop Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. On top of the skill's per-task commit step, this project follows a **branch-per-task workflow**: each task is implemented on its own branch (`feat/<name>`), reviewed and merged back to `trunk` with `git merge --no-ff`, the branch is deleted, and the human partner is checked in before the next task starts.

**Goal:** Convert the TUI-OS boot shell into a Spanish-language TUI desktop (dock + fullscreen apps: Terminal with a functional command set, a 1:1 port of the files app, System, Help), booting on VGA 80×25, with a disk that is formatted and seeded at first boot.

**Architecture:** Option A — a `desktop.rs` compositor that owns all apps as concrete structs and dispatches via an `AppKind` enum (no trait objects). `shell.rs` is dismantled: its pieces move into `apps/terminal.rs` (TerminalApp), `apps/sysinfo.rs` (SysInfoApp), `apps/help.rs` (HelpApp) and the desktop itself (wallpaper); the ratatui widgets demo is removed (its sparkline moves to SysInfoApp). The files app port lives in `apps/files/` with the same module layout as the reference implementation. `usr::shell::main` remains as a shim. Kernel-side unit tests run through the existing `#[test_case]` in-kernel test runner (`make test`), which Task 1 first repairs.

**Tech Stack:** Rust nightly 2026-03-01 (no_std + alloc), ratatui 0.30 with the custom `VgaBackend` (80×25 VGA text, CP437), MFS (in-kernel filesystem, `format_mem`/`mount_mem` for tests), bootimage + QEMU for verification.

**Spec:** `docs/superpowers/specs/2026-09-24-tuios-desktop-design.md`

## Global Constraints

- UI text in **Spanish**; command names follow Unix conventions (`touch`, `mkdir`, …).
- Desktop = dock + **fullscreen** apps. No windows, no start menu.
- VGA 80×25, CP437 only — any glyph used must already exist in `usr/tui.rs::char_to_cp437` (`─│┌┐└┘░▒▓█▸▀▄▌▐•·…` + ASCII).
- Color tokens (paleta de marca **"Ink & Brass"**, cargada en la DAC por `sys::vga::palette::set_brand_palette()`; los índices VGA conservan su rol): desktop/ink bg Black(0) `#0C0C10`; paneles/título/dock Blue(1) `#181C24`; cuerpo LightGray(7) `#BCC4CC`; apagado/bordes DarkGray(8) `#3C4450`; acento global **LightCyan(11) = latón** `#C8A05C`; prompt/éxito LightGreen(10) `#94AC88`; LightRed(12) `#C06058` solo para errores/destructive. Todos los canales múltiplos de 4 (DAC 6-bit exacta). `Color::Reset` como **fondo** mapea a 0 (ink) vía `usr/tui.rs::bg_to_vga_index`.
- Keys: F1 Ayuda · F2 Sistema · F3 Terminal · F4 Archivos · F5/Esc → Escritorio. Dock navigable with ←/→ + Enter when the desktop is focused.
- Apps `handle_key` returns `AppAction` (enum dispatch). The approved spec lists `Keep/Close`; this plan extends it with `Switch(AppKind)` because the terminal's `help`/`sysinfo`/`abrir` commands switch apps (F-keys alone cannot express this). This is the only structural deviation from the design doc and is done to satisfy the approved command list.
- Errors show as Spanish alerts/log lines/dialogs; never panic. Boot continues if formatting fails.
- `AppAction::Close` from Files' normal mode: `q` or `Esc` (the first `Esc` closes any open dialog/preview inside Files).
- Crate is `tuios`, version `0.1.0`, brand **TUI-OS**. `make image` must keep producing `tuios-x86_64.img`/`disk.img`.
- Only kernel APIs: `sys::keyboard::try_pop_decoded_key`, `sys::fs` (+ `FileIO`), `sys::clk::{date,ticks,boot_time}`, `sys::mem::{memory_size,memory_used,memory_free}`, `sys::acpi::shutdown`, `sys::idt::reset`, `sys::process::{dir,set_dir}`, `api::unit::SizeUnit`. No std, no crossterm.
- MFS has **no rename/copy primitive**: move/copy (terminal `mv`/`cp` and Files app) is implemented as copy-then-delete in the port's fsops.

## Review Focus

Failure modes the spec implies but no task's own tests exercise directly:

1. **Dock keyboard focus** — when an app is open, ←/→/Enter must not hijack app keys (they belong to the app); dock navigation only works when the desktop is focused. Desktop's key routing pins this with a unit test on a pure `route_key` helper.
2. **Esc layer ordering in Files** — dialog open + Esc (close dialog) vs normal mode + Esc (close app). The port's `handle_key` returns the right `AppAction`; unit tests cover dialog-open + Esc → Keep, normal + Escc → Close, normal + q → Close.
3. **`fs::seed()` idempotency** — seeding must not duplicate files or crash on a second call (and on boot when the disk is already mounted). Test: call `seed()` twice on a fresh mem FS; content/name counts unchanged; `Dir::open("/")` lists exactly the expected entries.
4. **Command-line arg edge cases** — `cd` with no args (→ home), `rm` on a non-existent path, `cat` on a directory, `ls` with no args (current dir), `mv` with wrong arg count → Spanish error, no panic. The commands module tests these.
5. **80-column overflow** — all fixed strings (title bar, dock, status, wallpaper tagline, banner) must fit 80 cols; the terminal banner already wraps if the version grows. Desktop UI task renders a full-screen screendump and checks no line exceeds 80 chars.

---

### Task 1: Repair the in-kernel test harness (`make test`)

**Files:**
- Modify: `src/api/font.rs:41-51` (`parse_psf_font` test)
- Modify: `src/sys/process/spawn.rs:277-301` (`test_load` test)

**Interfaces:**
- Consumes: nothing (first task).
- Produces: a working `make test` pipeline for all later tasks.

**Context:** `make test` builds the lib with `--features serial` and `#[test_case]` functions. Two tests reference `dsk/` assets that the fork trimmed away:
- `src/api/font.rs:43,46` — `include_bytes!("../../dsk/ini/boot.sh")` and `.../zap-light-8x16.psf` inside `parse_psf_font`.
- `src/sys/process/spawn.rs:281` — `include_bytes!("../../../dsk/bin/echo")` inside `test_load`.

- [ ] **Step 1: Verify the failure mode**

Run: `make test`
Expected: fails with `couldn't read 'src/api/../../dsk/ini/boot.sh'` and `couldn't read '.../dsk/bin/echo'`.

- [ ] **Step 2: Rewrite `parse_psf_font` asset-free**

Replace the body of `parse_psf_font` (keep the `#[test_case]` attribute) with a synthetic PSF1 buffer:

```rust
#[test_case]
fn parse_psf_font() {
    let buf = [0x36, 0x04, 0x00, 0x10]; // magic, mode, height=16
    assert!(Font::try_from(&buf[..]).is_err()); // header-only is too short

    let mut buf = Vec::new();
    buf.extend_from_slice(&[0x36, 0x04, 0x00, 0x10]);
    buf.extend_from_slice(&[0u8; 256 * 16]);
    let font = Font::try_from(&buf[..]).unwrap();
    assert_eq!(font.height, 16);
    assert_eq!(font.size, 256);
    assert_eq!(font.data.len(), 256 * 16);
}
```

- [ ] **Step 3: Drop the userland binary case from `test_load`**

The fork has no userland bins, so the loader test keeps its magic-buffer cases and loses the real-ELF one. Remove the three lines referencing the include and leave the loop over the remaining cases:

```rust
let bins = vec![
    (vec![], Err(())),
    (vec![b'F'], Err(())),
    (vec![b'F', b'A', b'I', b'L'], Err(())),
    (vec![0x7F, b'E', b'L', b'F'], Err(())),
    (vec![0x7F, b'E', b'L', b'F', b'F', b'A', b'I', b'L'], Err(())),
    (vec![0x7F, b'B', b'I', b'N', b'P', b'A', b'S', b'S'], Ok(USER_ADDR)),
];
```

(the `use alloc::vec;` import stays; `object::File::parse(&print_bin[..])` calls are gone).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `make test`
Expected: builds, boots headless in QEMU, `test_runner` prints each `test_*` result, exits via the isa-debug-exit device, exit code 0.

- [ ] **Step 5: Commit + merge**

```bash
git checkout -b fix/test-harness
git add src/api/font.rs src/sys/process/spawn.rs
git commit -m "fix: make cargo test build without trimmed dsk assets"
git checkout trunk && git merge --no-ff fix/test-harness -m "Merge fix/test-harness: repair make test"
git branch -d fix/test-harness
```

---

### Task 2: Desktop framework (apps dispatcher)

**Files:**
- Create: `src/usr/apps/mod.rs`
- Create: `src/usr/apps/terminal.rs`, `src/usr/apps/sysinfo.rs`, `src/usr/apps/help.rs`, `src/usr/apps/files.rs` (scaffold)
- Create: `src/usr/desktop.rs`
- Modify: `src/usr/mod.rs` (module declarations), `src/usr/shell.rs` (shim), `src/lib.rs:64-68` (`exec()` unchanged, `usr::shell::main` keeps working)

**Interfaces:**
- Produces (consumed by later tasks):
```rust
// src/usr/apps/mod.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppKind { Terminal, Files, SysInfo, Help }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppAction { Keep, Close, Switch(AppKind) }
pub static APPS: &[(AppKind, &str, &str)] = &[
    (AppKind::Files,     "Archivos", "Explorador de archivos"),
    (AppKind::Terminal,  "Terminal", "Shell con comandos"),
    (AppKind::SysInfo,   "Sistema",  "Información del sistema"),
    (AppKind::Help,      "Ayuda",    "Comandos y atajos"),
];
```
```rust
// src/usr/desktop.rs
pub fn main() -> !                       // owns wallpaper + dock; runs the loop
pub enum DesktopCmd { None, Open(AppKind), Halt, Close, Switch(AppKind) }
fn route_key(app: Option<AppKind>, dock_index: usize, key: DecodedKey)
    -> (DesktopCmd, usize)                // pure, testable
pub struct Desktop { app: Option<AppKind>, dock_index: usize,
    terminal: TerminalApp, files: FilesApp, sysinfo: SysInfoApp, help: HelpApp }
```
- TerminalApp keeps the shell's `log/input/history/cursor/screen` machinery minus the Home/Help/SysInfo/Widgets screens; its `exec` returns `AppAction` for command side-effects (`help`→`Switch(Help)`, `sysinfo`→`Switch(SysInfo)`, `halt`→halt, etc.). The `Screen` enum is deleted.

- [ ] **Step 1: Write the failing test — dock routing**

In `src/usr/desktop.rs`, a `#[cfg(test)]` module with the pure `route_key` helper first:

```rust
// route_key(app: Option<AppKind>, dock_index: usize, key) -> (DesktopCmd, usize)
// Desktop-focused (app == None): Left/Right move dock_index (wrap 0..=4, 4 = "Apagar"),
//   Enter opens the app at dock_index (Open), Enter at 4 -> Halt, F1..F4 -> Switch.
// App-focused: F1..F4 -> Switch(app), Esc/F5 -> Close, anything else -> None (the
//   desktop then passes the key to the app unchanged; dock_index is untouched).
#[test_case]
fn route_key_desktop_navigation() {
    assert_eq!(route_key(None, 0, KeyCode::ArrowRight), (DesktopCmd::None, 1));
    assert_eq!(route_key(None, 1, KeyCode::ArrowLeft), (DesktopCmd::None, 0));
    assert_eq!(route_key(None, 4, KeyCode::ArrowRight), (DesktopCmd::None, 0)); // wrap
    assert_eq!(route_key(None, 2, KeyCode::Enter), (DesktopCmd::Open(AppKind::SysInfo), 2));
    assert_eq!(route_key(None, 4, KeyCode::Enter), (DesktopCmd::Halt, 4));
    assert_eq!(route_key(None, 0, KeyCode::F3), (DesktopCmd::Switch(AppKind::Terminal), 0));
    assert_eq!(
        route_key(Some(AppKind::Terminal), 2, KeyCode::F2),
        (DesktopCmd::Switch(AppKind::SysInfo), 2)
    );
    assert_eq!(
        route_key(Some(AppKind::Terminal), 2, KeyCode::Esc),
        (DesktopCmd::Close, 2)
    );
    // App focused: Left/Right belong to the app, not the dock.
    assert_eq!(
        route_key(Some(AppKind::Files), 2, KeyCode::ArrowRight),
        (DesktopCmd::None, 2)
    );
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `make test`
Expected: FAIL — `route_key` not found.

- [ ] **Step 3: Implement the framework**

- Create `apps/mod.rs` with the enum types above.
- Create `Desktop` in `desktop.rs` holding the four app structs and a `run(&mut self, terminal: &mut Terminal<VgaBackend>)` loop cloned from `Shell::run` (poll `try_pop_decoded_key`, F-keys intercepted, others → `route_key` then dispatch): desktop-focused keys → dock nav / launch; app-focused → app `handle_key`, honoring `AppAction`.
- Cut `shell.rs`: keep `version()` and move it to `desktop.rs` (or `util.rs`); move `Shell` input/log/history machinery into `apps/terminal.rs` as `TerminalApp`; `render_sysinfo` body → `apps/sysinfo.rs::SysInfoApp::render`; `render_help` body → `apps/help.rs::HelpApp::render`; **delete** `Screen::Widgets`, `render_widgets`, the `widgets` command entry and its COMMANDS line (task 5 rebuilds the catalog). Keep the terminal's own log view as its render.
- `apps/files.rs` scaffold renders an empty "Archivos: (port pendiente)" area for now (task 6 fills it).
- `shell.rs::main` becomes: `pub fn main(_args: &[&str]) -> ! { usr::desktop::main() }` (Keep `.run` territory on the desktop; module path `crate::usr::desktop`).
- Remove the old keyboard `handle_key` F-key screen switching (F1..F4) from TerminalApp — the desktop owns F-keys now.

- [ ] **Step 4: Run tests to verify they pass**

Run: `make test`
Expected: the new `route_key_desktop_navigation` test passes; all prior tests still pass.

- [ ] **Step 5: Boot sanity check**

Run: `make image`, then boot headless and dump the screen:

```bash
qemu-system-x86_64 -m 32 -smp 2 -cpu core2duo -drive file=disk.img,format=raw \
  -monitor telnet:127.0.0.1:7781,server,nowait -display none &
sleep 7; python3 /tmp/opencode/vgatext.py --port 7781; pkill -f qemu-system-x86_64
```

Expected: boots to the terminal view (output identical in spirit to today — full brand/UI polish is Task 3).

- [ ] **Step 6: Commit + merge**

```bash
git checkout -b feat/desktop-framework
git add src/usr
git commit -m "feat: desktop framework with enum dispatch (AppKind/AppAction), split shell into apps"
git checkout trunk && git merge --no-ff feat/desktop-framework -m "Merge feat/desktop-framework"
git branch -d feat/desktop-framework
```

---

### Task 3: Desktop UI (wallpaper, title bar, dock, status)

**Files:**
- Modify: `src/usr/desktop.rs` (render pipeline, dock widget)
- Modify: `src/usr/apps/terminal.rs` (its inner render now owns hint+input; remove old title/status from it)

**Interfaces:**
- Consumes: `Desktop` from Task 2, `version()`, `util.rs` helpers (`format_uptime`, `format_size`).
- Produces: the desktop render contract:
  - row 0 title bar: `‹ Escritorio` (or app name when focused) left, ` TUI-OS v{} ` center-left, clock ` sys::clk::date() ` right — Blue(1) bg, White(15) fg.
  - rows 1..=22 main area: wallpaper when `focus == None`; otherwise the focused app renders fullscreen.
  - row 23 dock: `▸ Archivos  ▸ Terminal  ▸ Sistema  ▸ Ayuda  ░  Apagar` on Black(0)/LightGray(7); the selected item inverted LightCyan bg + Black fg.
  - row 24 status: ` MEM … ` left, ` UP … ` center, clock right, on Blue(1)/White(15).
- `util.rs`: `pub fn format_uptime(secs: usize) -> String` and `pub fn format_size(n: usize) -> String` (binary units, Spanish `M`/`K` suffixes).

- [ ] **Step 1: Write the failing tests — widget atoms**

`src/usr/util.rs` with pure helpers (testable without a screen):

```rust
#[test_case]
fn format_uptime_parts() {
    assert_eq!(format_uptime(0), "0s");
    assert_eq!(format_uptime(65), "1m 5s");
    assert_eq!(format_uptime(3661), "1h 1m 1s");
}

#[test_case]
fn format_size_binary_units() {
    assert_eq!(format_size(1024), "1K");
    assert_eq!(format_size(17 * 1024 * 1024), "17M");
}
```

And a pure dock atom builder in `desktop.rs` (returns label + whether it is the selected item, so the renderer only applies styles):

```rust
fn dock_cells(selected: usize) -> Vec<(&'static str, bool)>
    // [("▸ Archivos", sel), ("▸ Terminal", sel), ("▸ Sistema", sel),
    //  ("▸ Ayuda", sel), ("░", false), ("Apagar", sel)]  -- idx 4 == Apagar
#[test_case]
fn dock_cells_mark_single_selection() {
    let cells = dock_cells(2);
    assert_eq!(cells.iter().filter(|(_, sel)| *sel).count(), 1);
    assert!(cells[2].1);            // Sistema selected
    let cells = dock_cells(0);
    assert!(cells[0].1 && !cells[2].1);
    let joined: String = dock_cells(0).iter().map(|(l, _)| *l).collect::<Vec<_>>().join("  ");
    assert!(joined.contains("Apagar"));
    assert!(joined.len() < 70);     // fits 80 cols with title/status margins
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `make test` → FAIL (`util` module / helpers missing).

- [ ] **Step 3: Implement the UI**

- `src/usr/util.rs`: implement `format_uptime` (h/m/s breakdown, omit zero-leading parts) and `format_size` (binary, `K`/`M`/`G`).
- `desktop.rs::render`: build the vertical `Layout::vertical([Length(1), Min(8), Length(1), Length(1)])` split on `frame.area()`; swap the terminal's former title/status rows for the desktop's own.
- `render_wallpaper`: ink(0) bg for the whole area; centered **refined wordmark** `TUI-OS` (White(15) + brass/LightCyan(11) version), tagline `"Escritorio 100% Rust sobre VGA 80x25"` centered DarkGray(8), and a hairline `────── · ──────` (DarkGray). No ASCII art, no `░▒▓█` horizon — the previous banner art and block horizon are retired ("menos retro").
- `render_dock`: one row, items from `APPS` + `░` separator + `Apagar`; selection highlight = `Style::bg(Color::LightCyan).fg(Color::Black)`, otherwise `bg(Black).fg(LightGray)`; the selected item prefixed `▸` (others ` `).
- `render_status` on row 24: `MEM {used}/{total}` (via `sys::mem` + `util::format_size`), `UP {format_uptime(boot_time)}`, date right.
- Title bar: `‹ Escritorio` when desktop, else the app's Spanish name; clock right.
- TerminalApp render: log + hint row + input row + status inside its main area (its own 5-row inner layout: log `Min`, hint, input, status — dropped from the desktop level).

- [ ] **Step 4: Run tests + screendump verification**

Run: `make test` → new tests pass. Then `make image` and boot + `vgatext.py` (same as Task 2 Step 5). Verify in the dump:
- title bar shows `‹ Escritorio` and a clock; wallpaper art + tagline + `░▒▓█` horizon; dock row with `▸` on the selected item and `Apagar` at the end; status line with MEM/UP/clock; **no line over 80 chars**.

- [ ] **Step 5: Commit + merge**

```bash
git checkout -b feat/desktop-ui
git add src/usr
git commit -m "feat: desktop UI — wallpaper, title bar, dock and status (Spanish)"
git checkout trunk && git merge --no-ff feat/desktop-ui -m "Merge feat/desktop-ui"
git branch -d feat/desktop-ui
```

---

### Task 4: Disk formatted and seeded at first boot

**Files:**
- Modify: `src/sys/fs/mod.rs` (`init`, new `pub fn seed_root()`)

**Interfaces:**
- Consumes: `SuperBlock::check_ata`, `mount_ata(bus, dsk)`, `format_ata()`, `format_mem`, `mount_mem`, `Dir::{open,root,create_dir}`, `sys::fs::open` + `OpenFlag`, `FileIO::write`, `File::read_to_string`.
- Produces: `pub fn seed_root()` — idempotent seeding used by `init()` and by tests; `init()` formats+mounts when no superblock is found.

- [ ] **Step 1: Write the failing test — seed on a memory disk**

In `src/sys/fs/mod.rs`:

```rust
#[test_case]
fn seed_root_creates_welcome_files() {
    format_mem();
    mount_mem();
    seed_root();
    assert!(Dir::open("/").unwrap().find("bienvenida.txt").is_some());
    assert!(Dir::open("/").unwrap().find("manual.txt").is_some());
    assert!(Dir::open("/usr").unwrap().find("README.txt").is_some());
    let mut f = File::open("/bienvenida.txt").unwrap();
    let text = f.read_to_string();
    assert!(text.contains("TUI-OS"));
    seed_root(); // idempotent
    assert_eq!(Dir::open("/").unwrap().entries().count(), 3);
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `make test` → FAIL (`seed_root` missing / FS empty).

- [ ] **Step 3: Implement seeding**

```rust
fn write_file(pathname: &str, content: &str) {
    let flags = OpenFlag::Write as u8 | OpenFlag::Create as u8 | OpenFlag::Truncate as u8;
    if let Some(mut res) = open(pathname, flags) {
        res.write(content.as_bytes()).ok();
        res.close();
    }
}

pub fn seed_root() {
    if Dir::root().find("bienvenida.txt").is_some() {
        return; // already seeded
    }
    let _ = Dir::root().create_dir("usr");
    write_file("/bienvenida.txt",
        "Bienvenido a TUI-OS v0.1.0\n\nUn sistema operativo 100% Rust con escritorio de texto.\nPulsa F1 para la ayuda y F4 para el explorador de archivos.\n");
    write_file("/manual.txt",
        "MANUAL DE TUI-OS\n\n1. Escritorio: F1 Ayuda · F2 Sistema · F3 Terminal · F4 Archivos\n2. Dock: ← → para elegir, Enter para abrir\n3. Terminal: escribe 'help' para los comandos\n4. La primera vez, el disco se formatea y se siembran estos archivos.\n");
    write_file("/usr/README.txt", "Directorio /usr\n\nAquí viven los archivos de usuario.\n");
}

pub fn init() {
    let mut mounted = false;
    'scan: for bus in 0..2 {
        for dsk in 0..2 {
            if SuperBlock::check_ata(bus, dsk) {
                log!("MFS Superblock found in ATA {}:{}", bus, dsk);
                mount_ata(bus, dsk);
                mounted = true;
                break 'scan;
            }
        }
    }
    if !mounted {
        log!("TUI-OS: no MFS superblock found, formatting ATA 0:0");
        format_ata();
        mount_ata(0, 0);
    }
    seed_root();
}
```

- [ ] **Step 4: Run tests + boot verification**

Run: `make test` → passes. Then `make image`, delete `disk.img` (`rm -f disk.img`), run `make image` again (fresh 32M disk), boot and check with `vgatext.py` that `ls`/the Files scaffold see `/bienvenida.txt`; the serial log line `TUI-OS: no MFS superblock found...` appears on first boot only.

- [ ] **Step 5: Commit + merge**

```bash
git checkout -b feat/fs-seed
git add src/sys/fs/mod.rs
git commit -m "feat: format and seed the MFS disk at first boot (bienvenida/manual/usr)"
git checkout trunk && git merge --no-ff feat/fs-seed -m "Merge feat/fs-seed"
git branch -d feat/fs-seed
```

---

### Task 5: Terminal — full command set

**Files:**
- Modify: `src/usr/apps/terminal.rs` (COMMANDS catalog, `exec`)

**Interfaces:**
- Consumes: `sys::fs` (open/delete/info/Dir/File/FileIO, `resource write`), `sys::process::{dir,set_dir}`, `sys::clk`, `sys::mem`, `api::unit::SizeUnit`, `version()`.
- Produces: the final Spanised help catalog and command set below.

**Command list (approval from the spec):** `pwd cd ls cat touch mkdir rm mv cp help(ayuda) apps abrir sysinfo mem uptime date version echo clear random pci halt(apagar) reboot(reiniciar)`.

- [ ] **Step 1: Write the failing tests — command behaviors on a mem FS**

In `src/usr/apps/terminal.rs` `#[cfg(test)]`:

```rust
#[test_case]
fn exec_fs_commands() {
    format_mem();
    mount_mem();
    let mut term = TerminalApp::new();
    let (out, action) = term.exec("touch /a.txt");
    assert!(out.is_empty() && action.is_none());
    let (out, _) = term.exec("ls /");
    assert_eq!(out, vec!["a.txt".to_string()]);
    let _ = term.exec("mkdir /d");
    let (out, _) = term.exec("mv /a.txt /d/a.txt");
    let (out, _) = term.exec("ls /d");
    assert_eq!(out, vec!["a.txt".to_string()]);
    let (out, _) = term.exec("cat /d/a.txt");
    assert_eq!(out, vec!["".to_string()]);
    let (out, _) = term.exec("rm /d/a.txt");
    assert!(term.exec("ls /d").0.is_empty());
    let (out, _) = term.exec("cd");          // no args -> home, no error
    assert!(out.is_empty());
    let (out, _) = term.exec("cat /");        // dir, not a file -> Spanish error, no panic
    assert!(out[0].contains("no es un archivo") || out[0].contains("no existe"));
    let (out, _) = term.exec("mv /a.txt");    // wrong arg count -> Spanish error
    assert!(out[0].contains("uso"));
}
```

and a pure catalog test:

```rust
#[test_case]
fn command_catalog_has_dock_aliases() {
    let names: Vec<&str> = COMMANDS.iter().map(|(n, _)| *n).collect();
    for n in ["pwd","cd","ls","cat","touch","mkdir","rm","mv","cp","help","ayuda",
              "apps","abrir","sysinfo","mem","uptime","date","version","echo","clear",
              "random","pci","halt","apagar","reboot","reiniciar"] {
        assert!(names.contains(&n), "missing command {n}");
    }
    assert!(!names.contains(&"widgets"));
}
```

(Skip commands whose side effects are not FS-testable — `halt`/`reboot`/`abrir` — in the behavior test; the catalog test pins them.)

- [ ] **Step 2: Run to verify they fail**

Run: `make test` → FAIL.

- [ ] **Step 3: Implement the commands**

- New COMMANDS catalog (Spanish descriptions, `ayuda`/`apagar`/`reiniciar` aliases listed alongside).
- `exec` additions (each returns `Vec<String>` of log lines; commands with side effects return `AppAction`-bearing variants):
  - `pwd` → `process::dir()` (String, no trailing slash except `/`).
  - `cd <path>` → validate with `Dir::open(path)`; ok → `process::set_dir(realpath)`; else "cd: no existe la ruta '<path>'". No args → `process::set_dir("/")`... (home: `env_var("HOME")` or `/`).
  - `ls [path]` → `Dir::open(path or cwd)`, list `name` per entry, one per line.
  - `cat <file>` → `File::open` + `read_to_string`, push each line.
  - `touch <file>` → `open(path, Write|Create)` then close (truncation: use `Write|Create` only — no Truncate — to preserve content like `touch`).
  - `mkdir <path>` → `Dir::create` via `Dir::root().create_dir` on the realpath; error line on failure.
  - `rm <path>` → `sys::fs::delete(path)`; error line on failure.
  - `mv <src> <dst>` → fsops copy-to-new + `delete(src)` (reuse the File copy helper from Task 6's `apps/files/fsops.rs` — implement a minimal `copy_file` in `terminal.rs` now, Task 6 promotes it to the shared fsops module).
  - `cp <src> <dst>` → copy_file.
  - `help`/`ayuda` → `AppAction::Switch(AppKind::Help)` + "Mostrando la ayuda…".
  - `apps` → list `APPS` names + description lines.
  - `abrir <app>` → `AppAction::Switch(kind)` for a known name (Archivos/Terminal/Sistema/Ayuda, case-insensitive on first letter too: `a`/`t`/`s`/`h`); else error "abrir: app desconocida '…'".
  - `sysinfo` → `Switch(SysInfo)`; `mem` keeps total/used/free via `SizeUnit::Binary`.
  - `uptime`, `date`, `version`, `echo`, `clear` keep current behavior.
  - `random [n]` → `sys::rng::get_u64()`; with `n` given, produce `n` values (u64 range), one per line.
  - `pci` → iterate `sys::pci::list(): Vec<DeviceConfig>` and print bus/device/function + vendor/device/class lines per entry (device config accessors from `src/sys/pci.rs`), one line per device, prefixed `PCI `.
  - `halt`/`apagar` → log "Apagando…", `sys::acpi::shutdown()`; `reboot`/`reiniciar` → log, `sys::idt::reset()`.
  - Unknown command → `command not found: '…' — escribe 'help'` (Spanish).
- `TerminalApp::exec` signature becomes `fn exec(&mut self, cmdline: &str) -> (Vec<String>, Option<AppAction>)`; `submit` forwards the action to the desktop through `handle_key`'s return.

- [ ] **Step 4: Run tests + screendump verification**

Run: `make test` → passes. Then `make image`, boot, and drive the terminal via the monitor: `sendkey` burst `l s` + `ret` → screendump shows the seeded files; `sendkey` F3-then-F1 sequence verifies `abrir`/`apps` produce switch actions. (See Task 7 for the full script; a manual smoke of `ls`, `cat /bienvenida.txt` suffices here.)

- [ ] **Step 5: Commit + merge**

```bash
git checkout -b feat/terminal-commands
git add src/usr/apps/terminal.rs
git commit -m "feat: full terminal command set (fs, apps, system) with Spanish help"
git checkout trunk && git merge --no-ff feat/terminal-commands -m "Merge feat/terminal-commands"
git branch -d feat/terminal-commands
```

---

### Task 6: Files app — 1:1 port of the files app reference

**Files:**
- Create: `src/usr/apps/files/mod.rs`
- Create: `src/usr/apps/files/state.rs`, `src/usr/apps/files/input.rs`, `src/usr/apps/files/fsops.rs`, `src/usr/apps/files/preview.rs`, `src/usr/apps/files/ui.rs`
- Modify: `src/usr/apps/mod.rs` (declare `pub mod files;`), `src/usr/apps/terminal.rs` (mv/cp now call `files::fsops`)
- Remove: `src/usr/apps/files.rs` (the Task 2 scaffold — a `files.rs` file and a `files/` directory cannot coexist; `git rm` it and let `files/mod.rs` become the module root)

**Interfaces:**
- Consumes: the files app reference implementation (source of truth for the port), MFS kernel APIs, `AppAction`/`AppKind` from Task 2.
- Produces: `pub struct FilesApp` with `new()`, `handle_key(&mut self, k: DecodedKey) -> AppAction`, `render(&mut self, frame: &mut Frame, area: Rect)`; shared fsops moved to `pub mod files::fsops` usable by the terminal (`copy_file`, `move_path`, `list_dir`, `read_text`, `metadata_text`).

**std → kernel mapping (apply per module):**

| files app (std) | TUI-OS (kernel) |
|---|---|
| `std::path::PathBuf` | `String` (absolute paths) |
| `std::fs::read_dir` | `Dir::open(&path).entries()` (ReadDir: `name`, `size`, `time`, `kind`) |
| `std::fs::{create_dir, remove_file/remove_dir}` | `Dir::create` (via `Dir::root().create_dir` at realpath), `sys::fs::delete(path)` |
| `std::fs::rename` | copy-then-delete (`move_path` in fsops) |
| `std::fs::read_to_string` | `File::open(path)?.read_to_string()` |
| `std::fs::write/metadata` | `sys::fs::open` + `FileIO::write`; `sys::fs::info(path)` (`FileInfo { name, size, time, kind }`) |
| `chrono`/filetime dates | MFS `time` field (seconds) — render via the kernel clock, "día mes aaaa hh:mm" |
| crossterm events | `sys::keyboard::try_pop_decoded_key` → ratatui `DecodedKey` (already the terminal's type) |
| mouse / wheel | not ported (no mouse in kernel) |
| image preview (kitty) | `preview::render` falls back to metadata text (the reference app already has this fallback) |
| permissions / symlinks | not present in MFS → show `—` / `no` |

- [ ] **Step 1: Write the failing tests — fsops on a mem FS (port of the reference `src/fsops.rs` tests)**

Port the reference `src/fsops.rs:325-430`'s logic tests to MFS (they use `tempdir`; here `format_mem`/`mount_mem`):

```rust
#[test_case]
fn fsops_roundtrip() {
    format_mem();
    mount_mem();
    let _ = Dir::root().create_dir("tmp");
    fsops::mkdir("/tmp/d");
    fsops::touch("/tmp/d/f.txt");
    assert_eq!(fsops::read_text("/tmp/d/f.txt").unwrap(), "");
    fsops::write_text("/tmp/d/f.txt", "hola");
    assert_eq!(fsops::read_text("/tmp/d/f.txt").unwrap(), "hola");
    fsops::copy_path("/tmp/d/f.txt", "/tmp/d/g.txt");
    assert_eq!(fsops::read_text("/tmp/d/g.txt").unwrap(), "hola");
    fsops::move_path("/tmp/d/g.txt", "/tmp/d/h.txt");
    assert!(fsops::read_text("/tmp/d/h.txt").is_ok());
    assert!(sys::fs::info("/tmp/d/g.txt").is_none());
    fsops::delete_path("/tmp/d/h.txt");
    assert!(sys::fs::info("/tmp/d/h.txt").is_none());
}
```

And port the reference `src/input.rs:415-560`'s tests to `DecodedKey` (j/k/Enter/Backspace/g/G/Home/End/q/Esc/dot mappings), plus the Esc-layer ordering the app's `handle_key` owns:

```rust
#[test_case]
fn esc_layer_closes_dialog_before_app() {
    format_mem();
    mount_mem();
    let mut app = FilesApp::new();
    let key = DecodedKey::Char('n'); // open "new" dialog
    assert_eq!(app.handle_key(key), AppAction::Keep);
    assert!(app.dialog_open());
    assert_eq!(app.handle_key(DecodedKey::RawKey(KeyCode::Esc)), AppAction::Keep); // dialog closed
    assert!(!app.dialog_open());
    assert_eq!(app.handle_key(DecodedKey::RawKey(KeyCode::Esc)), AppAction::Close); // app closed
    assert_eq!(app.handle_key(DecodedKey::Char('q')), AppAction::Close);
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `make test` → FAIL (`files::fsops` missing).

- [ ] **Step 3: Port the modules**

- `fsops.rs`: `list_dir(path) -> Vec<(String name, bool is_dir, u64 size, u32 time)>`, `mkdir`, `touch`, `write_text`, `read_text`, `copy_path(src,dst)` (recursive for dirs: walk + recreate + copy files), `move_path` (copy + delete), `delete_path`, `metadata_text(path) -> Vec<String>` (the preview/info fallback shown for non-text files).
- `state.rs`: `State { current: String, base: String, selected: Option<usize>, entries: Vec<Entry> }` mirroring the reference `State` (`Entry { name, is_dir, size, time }` — no perms/symlinks fields, show `—`/`no` in the UI).
- `input.rs`: map `DecodedKey`→reference actions exactly (keys j/k, Enter/→, Backspace/←, g/G, Home/End, ., h/H, n/N/r/d/c/m/p/q/Esc); `q`/`Esc` in normal mode → `AppAction::Close`; while a dialog is open, `Esc` closes the dialog (`AppAction::Keep`).
- `preview.rs`: text preview with the reference wrapping (adapted to `Vec<String>` + VGA width); non-text → metadata fallback.
- `ui.rs`: three panels (parent / current / info+preview) using Spans + Block borders with LightCyan accent; selection style LightCyan-bg/Esc-inverted as in the reference.
- `mod.rs`: `FilesApp { state, dialogs, preview }`; `handle_key` returns `AppAction`; `render` splits its area into the three panels.
- Terminal's `mv`/`cp` bodies are replaced by calls into `files::fsops::{copy_path, move_path}` (single source of truth).

- [ ] **Step 4: Run tests + interactive verification**

`make test` → passes. Then `make image`, boot, F4 → Files app. Drive via monitor: `sendkey f4`, screendump (three panels + `/` + `usr` + `bienvenida.txt`/`manual.txt`), `sendkey` down/enter into `/usr`, `sendkey n` (new dir dialog) type name + ret, screendump shows the created dir; `sendkey q` closes the app back to the desktop (screendump shows wallpaper). Esc-layer test: open a dialog, `sendkey esc` → dialog closes (app stays).

- [ ] **Step 5: Commit + merge**

```bash
git checkout -b feat/files-app
git add src/usr/apps/files src/usr/apps/mod.rs src/usr/apps/terminal.rs
git commit -m "feat: Files app — 1:1 port of the files app reference onto MFS (3 panels, dialogs, preview)"
git checkout trunk && git merge --no-ff feat/files-app -m "Merge feat/files-app"
git branch -d feat/files-app
```

---

### Task 7: Integration — SysInfo sparkline + full verification

**Files:**
- Modify: `src/usr/apps/sysinfo.rs` (live RAM sparkline + gauge)
- Verify: full boot script for every screen

**Interfaces:**
- Consumes: everything from Tasks 2-6 (sparkline reuses the deleted demo's widget logic; `sys::mem` sampling per render).

- [ ] **Step 1: Add the sparkline test**

```rust
#[test_case]
fn sparkline_ring_buffer() {
    let mut s = SysInfoApp::new();
    assert_eq!(s.samples.len(), 0);
    for i in 0..40 { s.push_sample(sys::mem::memory_used()); }
    assert_eq!(s.samples.len(), 30); // ring capped
}
```

- [ ] **Step 2: Run to verify it fails**

`make test` → FAIL (ring not implemented).

- [ ] **Step 3: Implement**

`SysInfoApp { samples: VecDeque<usize> }` capped at 30; `render` draws the kernel/version/RAM rows (existing `render_sysinfo` body), then a `Gauge` for `/` used and a `Sparkline` from the ring, rescaled to the widget width. Update the `APPS`/help descriptions if any wording shifts.

- [ ] **Step 4: Full end-to-end verification script**

- `make test` → all `#[test_case]` green.
- `make image` from a clean `disk.img`; boot headless; run a scripted monitor session asserting (via `vgatext.py` dumps) each of: wallpaper; dock selection moves with ←/→; F1 help app; F2 system app (sparkline row visible as `▁▂▃▄▅▆▇█` glyphs); F3 terminal + `ls` showing the seeded files; `abrir sistema` returns to System; F4 Files navigation into `/usr`; `q` returns to desktop; `F5` from an app returns to desktop.
- Confirm `TUIO_VERSION` env is exported (version shows `v0.1.0` everywhere; no `0.13.0` and no trace of the original project in the dump or in `strings` of the image except `LICENSE` attribution).

- [ ] **Step 5: Update docs**

- `README.md`: replace the "What is the desktop" table with the final dock items + commands list; mention the seeded first-boot disk.
- `docs/superpowers/specs/…-design.md`: mark all sections implemented.

- [ ] **Step 6: Commit + merge**

```bash
git checkout -b feat/integration
git add src/usr/apps/sysinfo.rs README.md docs
git commit -m "feat: System app RAM sparkline; full integration verification"
git checkout trunk && git merge --no-ff feat/integration -m "Merge feat/integration"
git branch -d feat/integration
```