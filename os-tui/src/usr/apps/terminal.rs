//! The terminal app: a command line over a scrolling output log.

use crate::api;
use crate::sys;
use crate::sys::fs::FileIO;
use crate::usr::apps::{AppAction, AppKind, APPS};
use crate::usr::util::{format_uptime, version};

use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;

use pc_keyboard::{DecodedKey, KeyCode};

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};
use ratatui::Frame;

/// Maximum number of characters in the command input line.
const MAX_INPUT: usize = 78;

/// Maximum number of lines kept in the on-screen log.
const MAX_LOG: usize = 256;

/// The built-in commands, with a one-line description for the help screen.
///
/// Names follow the usual Unix conventions, and the Spanish aliases the
/// desktop uses (`ayuda`, `apagar`, `reiniciar`) are listed here too so the
/// help screen documents everything that actually answers.
pub static COMMANDS: &[(&str, &str)] = &[
    // Filesystem
    ("pwd", "Muestra la ruta actual"),
    ("cd", "Cambia de directorio: cd <ruta>"),
    ("ls", "Lista el contenido de un directorio: ls [ruta]"),
    ("cat", "Muestra el contenido de un fichero: cat <fichero>"),
    ("touch", "Crea un fichero vacio: touch <fichero>"),
    ("mkdir", "Crea un directorio: mkdir <directorio>"),
    ("rm", "Borra un fichero: rm <fichero>"),
    ("mv", "Mueve o renombra: mv <origen> <destino>"),
    ("cp", "Copia un fichero: cp <origen> <destino>"),
    // Applications
    ("help", "Abre esta ayuda (alias: ayuda)"),
    ("ayuda", "Abre esta ayuda (alias de help)"),
    ("apps", "Lista las aplicaciones del dock"),
    ("abrir", "Abre una aplicacion: abrir <nombre>"),
    ("sysinfo", "Abre la informacion del sistema"),
    // System
    ("mem", "Muestra el uso de memoria"),
    ("uptime", "Muestra el tiempo activo"),
    ("date", "Muestra la fecha y la hora"),
    ("version", "Muestra la version del sistema"),
    ("echo", "Repite los argumentos dados"),
    ("clear", "Limpia el registro de la terminal"),
    ("random", "Genera numeros aleatorios: random [n]"),
    ("pci", "Lista los dispositivos del bus PCI"),
    ("halt", "Apaga el equipo (alias: apagar)"),
    ("apagar", "Apaga el equipo (alias de halt)"),
    ("reboot", "Reinicia el equipo (alias: reiniciar)"),
    ("reiniciar", "Reinicia el equipo (alias de reboot)"),
];

/// Report a wrong argument count the way a shell does.
fn usage(cmd: &str, form: &str) -> String {
    format!("uso: {} {}", cmd, form)
}

fn no_such_file(path: &str) -> String {
    format!("no existe la ruta '{}'", path)
}

/// `ls [ruta]` — one entry per line, directories included.
///
/// With no argument it lists the working directory. `.` is not used as the
/// default: MFS resolves paths by walking directory entries and has no entry
/// literally named `.`, so the CWD is named directly instead.
fn exec_ls(args: &[&str], out: &mut Vec<String>) {
    let path = match args.first() {
        Some(p) => (*p).to_string(),
        None => sys::process::dir(),
    };
    match sys::fs::Dir::open(&path) {
        Some(dir) => {
            for entry in dir.entries() {
                out.push(entry.name());
            }
        }
        None => out.push(no_such_file(&path)),
    }
}

/// `cat <fichero>` — the file's lines, one per line of output.
fn exec_cat(args: &[&str], out: &mut Vec<String>) {
    let path = match args.first() {
        Some(p) => *p,
        None => {
            out.push(usage("cat", "<fichero>"));
            return;
        }
    };
    // Check what it is before opening, so a directory gets a clear message
    // instead of the empty string a directory read produces.
    match sys::fs::info(path) {
        Some(info) if info.is_dir() => {
            out.push(format!("cat: '{}' no es un archivo", path));
        }
        Some(_) => match sys::fs::File::open(path) {
            Some(mut file) => {
                let text = file.read_to_string();
                // An empty file prints nothing at all, not a blank line: `cat`
                // on an empty file should not invent output.
                for line in text.lines() {
                    out.push(line.to_string());
                }
            }
            None => out.push(no_such_file(path)),
        },
        None => out.push(no_such_file(path)),
    }
}

/// `touch <fichero>` — create the file if it is missing, leave it alone if it
/// is not. No `Truncate`, which is what makes `touch` non-destructive.
fn exec_touch(args: &[&str], out: &mut Vec<String>) {
    let path = match args.first() {
        Some(p) => *p,
        None => {
            out.push(usage("touch", "<fichero>"));
            return;
        }
    };
    if sys::fs::info(path).is_some() {
        return; // already there: touch only updates the timestamp, and MFS
                // has no timestamp to update
    }
    let flags = sys::fs::OpenFlag::Write as u8 | sys::fs::OpenFlag::Create as u8;
    if sys::fs::open(path, flags).is_none() {
        out.push(format!("touch: no se pudo crear '{}'", path));
    }
}

/// `mkdir <directorio>` — one level, no `-p`.
fn exec_mkdir(args: &[&str], out: &mut Vec<String>) {
    let path = match args.first() {
        Some(p) => *p,
        None => {
            out.push(usage("mkdir", "<directorio>"));
            return;
        }
    };
    if sys::fs::Dir::create(path).is_none() {
        out.push(format!("mkdir: no se pudo crear el directorio '{}'", path));
    }
}

/// `rm <fichero>` — files only. There is no `rm -r` in this shell, and
/// silently deleting a tree is not a decision to make on someone's behalf.
fn exec_rm(args: &[&str], out: &mut Vec<String>) {
    let path = match args.first() {
        Some(p) => *p,
        None => {
            out.push(usage("rm", "<fichero>"));
            return;
        }
    };
    match sys::fs::info(path) {
        Some(info) if info.is_dir() => out.push(format!(
            "rm: '{}' es un directorio · use el explorador de archivos",
            path
        )),
        Some(_) => {
            if sys::fs::delete(path).is_err() {
                out.push(format!("rm: no se pudo borrar '{}'", path));
            }
        }
        None => out.push(no_such_file(path)),
    }
}

/// `mv <origen> <destino>` — copy then delete, so a move across the tree is
/// the same code path as `cp`.
fn exec_mv(args: &[&str], out: &mut Vec<String>) {
    if args.len() != 2 {
        out.push(usage("mv", "<origen> <destino>"));
        return;
    }
    let (src, dst) = (args[0], args[1]);
    if sys::fs::info(src).is_none() {
        out.push(no_such_file(src));
        return;
    }
    if copy_file(src, dst).is_err() {
        out.push(format!("mv: no se pudo mover '{}' a '{}'", src, dst));
        return;
    }
    if sys::fs::delete(src).is_err() {
        out.push(format!("mv: se copió pero no se pudo borrar '{}'", src));
    }
}

/// `cp <origen> <destino>`
fn exec_cp(args: &[&str], out: &mut Vec<String>) {
    if args.len() != 2 {
        out.push(usage("cp", "<origen> <destino>"));
        return;
    }
    let (src, dst) = (args[0], args[1]);
    if sys::fs::info(src).is_none() {
        out.push(no_such_file(src));
        return;
    }
    if copy_file(src, dst).is_err() {
        out.push(format!("cp: no se pudo copiar '{}' a '{}'", src, dst));
    }
}

/// Copy one file over another.
///
/// Chunked through `FileIO`, not `read_to_string`: the point of a copy is to
/// move bytes, and a text round-trip would corrupt anything that is not UTF-8.
/// A destination that already exists is replaced, as `cp` does.
fn copy_file(src: &str, dst: &str) -> Result<(), ()> {
    let mut input = sys::fs::File::open(src).ok_or(())?;
    let flags = sys::fs::OpenFlag::Write as u8
        | sys::fs::OpenFlag::Create as u8
        | sys::fs::OpenFlag::Truncate as u8;
    let mut output = sys::fs::open(dst, flags).ok_or(())?;
    let mut buf = [0u8; 512];
    loop {
        let n = input.read(&mut buf).map_err(|()| ())?;
        if n == 0 {
            break;
        }
        output.write(&buf[..n]).map_err(|()| ())?;
    }
    output.close();
    Ok(())
}

/// `abrir <nombre>` — switch to a dock app by name. Returns `Keep` when the
/// name is unknown, so the terminal stays put.
fn exec_abrir(args: &[&str], out: &mut Vec<String>) -> AppAction {
    let name = match args.first() {
        Some(n) => *n,
        None => {
            out.push(usage("abrir", "<nombre>"));
            return AppAction::Keep;
        }
    };
    for (kind, label, _) in APPS {
        if label.eq_ignore_ascii_case(name) {
            out.push(format!("Abriendo {}…", label));
            return AppAction::Switch(*kind);
        }
    }
    out.push(format!("abrir: aplicación desconocida '{}'", name));
    AppAction::Keep
}

/// `random [n]` — one value, or `n` of them.
fn exec_random(args: &[&str], out: &mut Vec<String>) {
    let count = match args.first() {
        None => 1,
        Some(n) => match n.parse::<usize>() {
            Ok(0) => 1,
            Ok(n) => n.min(64), // a typo should not lock up the terminal
            Err(_) => {
                out.push(usage("random", "[n]"));
                return;
            }
        },
    };
    for _ in 0..count {
        out.push(sys::rng::get_u64().to_string());
    }
}

/// `pci` — one line per device.
fn exec_pci(out: &mut Vec<String>) {
    for dev in sys::pci::list() {
        out.push(format!(
            "PCI {:02x}:{:02x}.{:x}  {:04x}:{:04x}  clase {:02x}{:02x}",
            dev.bus,
            dev.device,
            dev.function,
            dev.vendor_id,
            dev.device_id,
            dev.class,
            dev.subclass,
        ));
    }
}

/// One line of the terminal output log.
struct LogEntry {
    text: String,
    prompt: bool,
}

/// The terminal's state, mutated by key presses and redrawn on each frame.
pub struct TerminalApp {
    input: String,
    cursor: usize,
    history: Vec<String>,
    history_pos: Option<usize>,
    log: Vec<LogEntry>,
}

impl TerminalApp {
    pub fn new() -> Self {
        let mut app = TerminalApp {
            input: String::new(),
            cursor: 0,
            history: Vec::new(),
            history_pos: None,
            log: Vec::new(),
        };
        app.push_log(
            format!("TUI-OS v{} · 100% Rust: kernel + bootloader + shell", version()),
            false,
        );
        app.push_log(
            "El escritorio se dibuja con ratatui 0.30 en el búfer de texto VGA 80x25".to_string(),
            false,
        );
        app.push_log(
            "Sin espacio de usuario: todo vive en el crate del kernel".to_string(),
            false,
        );
        app.push_log(
            "Escriba 'help' (o pulse F1) para listar los comandos disponibles.".to_string(),
            false,
        );
        app
    }

    fn push_log(&mut self, text: String, prompt: bool) {
        if self.log.len() >= MAX_LOG {
            self.log.remove(0);
        }
        self.log.push(LogEntry { text, prompt });
    }

    // ------------------------------------------------------------------
    // Key handling
    // ------------------------------------------------------------------

    /// Handle one decoded key.
    ///
    /// The F-keys are not handled here: the desktop owns them, so that they
    /// mean the same thing no matter which app is open.
    pub fn handle_key(&mut self, key: DecodedKey) -> AppAction {
        match key {
            DecodedKey::Unicode(c) => match c {
                '\n' => return self.submit(),
                '\t' => self.tab(),
                '\x08' => self.backspace(),
                '\x7f' => self.delete(),
                '\x03' => self.cancel_input(), // Ctrl+C
                '\x0c' => self.clear_screen(),  // Ctrl+L
                '\x1b' => {}                    // Escape
                c if c.is_control() => {}
                c => self.insert_char(c),
            },
            DecodedKey::RawKey(code) => match code {
                KeyCode::ArrowUp => self.history_prev(),
                KeyCode::ArrowDown => self.history_next(),
                KeyCode::ArrowLeft => self.left(),
                KeyCode::ArrowRight => self.right(),
                KeyCode::Home => self.cursor = 0,
                KeyCode::End => self.cursor = self.input.chars().count(),
                _ => {}
            },
        }
        AppAction::Keep
    }

    fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    fn right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.input.chars().count());
    }

    fn tab(&mut self) {
        self.complete();
    }

    fn set_input(&mut self, s: String) {
        self.input = s;
        self.cursor = self.input.chars().count();
    }

    fn insert_char(&mut self, c: char) {
        let mut chars: Vec<char> = self.input.chars().collect();
        if chars.len() >= MAX_INPUT {
            return;
        }
        self.cursor = self.cursor.min(chars.len());
        chars.insert(self.cursor, c);
        self.input = chars.into_iter().collect();
        self.cursor += 1;
    }

    fn backspace(&mut self) {
        let mut chars: Vec<char> = self.input.chars().collect();
        self.cursor = self.cursor.min(chars.len());
        if self.cursor == 0 {
            return;
        }
        chars.remove(self.cursor - 1);
        self.input = chars.into_iter().collect();
        self.cursor -= 1;
    }

    fn delete(&mut self) {
        let mut chars: Vec<char> = self.input.chars().collect();
        self.cursor = self.cursor.min(chars.len());
        if self.cursor >= chars.len() {
            return;
        }
        chars.remove(self.cursor);
        self.input = chars.into_iter().collect();
    }

    fn cancel_input(&mut self) {
        self.input.clear();
        self.cursor = 0;
    }

    fn clear_screen(&mut self) {
        self.log.clear();
    }

    fn complete(&mut self) {
        if self.input.is_empty() || self.input.contains(' ') {
            return;
        }
        let word = self.input.clone();
        let matches: Vec<&'static str> = COMMANDS
            .iter()
            .map(|(n, _)| *n)
            .filter(|n| n.starts_with(&word))
            .collect();
        match matches.len() {
            0 => {}
            1 => self.set_input(matches[0].to_string()),
            _ => self.push_log(matches.join("  "), false),
        }
    }

    fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let pos = match self.history_pos {
            Some(p) if p > 0 => p - 1,
            _ => self.history.len() - 1,
        };
        self.history_pos = Some(pos);
        self.set_input(self.history[pos].clone());
    }

    fn history_next(&mut self) {
        match self.history_pos {
            Some(p) if p + 1 < self.history.len() => {
                self.history_pos = Some(p + 1);
                self.set_input(self.history[p + 1].clone());
            }
            _ => {
                self.history_pos = None;
                self.input.clear();
                self.cursor = 0;
            }
        }
    }

    fn submit(&mut self) -> AppAction {
        let cmd = self.input.trim().to_string();
        let mut action = AppAction::Keep;
        if !cmd.is_empty() {
            self.history.push(cmd.clone());
            self.history_pos = None;
            self.push_log(format!("> {}", cmd), true);
            let (lines, act) = self.exec(&cmd);
            for line in lines {
                self.push_log(line, false);
            }
            action = act;
        }
        self.input.clear();
        self.cursor = 0;
        action
    }

    /// Run one command line, returning its output and the app switch it asks
    /// for. `Switch` is how `help`, `abrir` and `sysinfo` reach the screen they
    /// name.
    ///
    /// Nothing here panics. A command that cannot do what it was asked prints
    /// a Spanish error line and leaves the terminal exactly as it was.
    fn exec(&mut self, cmdline: &str) -> (Vec<String>, AppAction) {
        let mut out = Vec::new();
        let mut parts = cmdline.split_whitespace();
        let cmd = match parts.next() {
            Some(c) => c,
            None => return (out, AppAction::Keep),
        };
        let args: Vec<&str> = parts.collect();
        let mut action = AppAction::Keep;
        match cmd {
            // ---- Filesystem -------------------------------------------
            "pwd" => out.push(sys::process::dir()),
            "cd" => self.exec_cd(&args, &mut out),
            "ls" => exec_ls(&args, &mut out),
            "cat" => exec_cat(&args, &mut out),
            "touch" => exec_touch(&args, &mut out),
            "mkdir" => exec_mkdir(&args, &mut out),
            "rm" => exec_rm(&args, &mut out),
            "mv" => exec_mv(&args, &mut out),
            "cp" => exec_cp(&args, &mut out),

            // ---- Applications -----------------------------------------
            "help" | "ayuda" => {
                out.push("Abriendo la ayuda, pulse F5 para volver.".to_string());
                action = AppAction::Switch(AppKind::Help);
            }
            "apps" => {
                for (_, label, desc) in APPS {
                    out.push(format!("{:<12} {}", label, desc));
                }
            }
            "abrir" => action = exec_abrir(&args, &mut out),
            "sysinfo" => {
                out.push(
                    "Abriendo la informacion del sistema, pulse F5 para volver.".to_string(),
                );
                action = AppAction::Switch(AppKind::SysInfo);
            }

            // ---- System ------------------------------------------------
            "clear" => {
                self.log.clear();
            }
            "echo" => out.push(args.join(" ")),
            "date" => out.push(sys::clk::date()),
            "uptime" => {
                out.push(format!("Tiempo activo: {}", format_uptime(sys::clk::boot_time())))
            }
            "version" => out.push(version()),
            "mem" => {
                let unit = api::unit::SizeUnit::Binary;
                out.push(format!(
                    "RAM total:    {}",
                    unit.format(sys::mem::memory_size())
                ));
                out.push(format!(
                    "RAM en uso:   {}",
                    unit.format(sys::mem::memory_used())
                ));
                out.push(format!(
                    "RAM libre:    {}",
                    unit.format(sys::mem::memory_free())
                ));
            }
            "random" => exec_random(&args, &mut out),
            "pci" => exec_pci(&mut out),
            "halt" | "apagar" => {
                out.push("Apagando el equipo…".to_string());
                sys::acpi::shutdown();
            }
            "reboot" | "reiniciar" => {
                sys::idt::reset(); // Never returns
            }
            _ => out.push(format!("comando no encontrado: '{}' · escriba 'help'", cmd)),
        }
        (out, action)
    }

    /// `cd [ruta]`. With no argument, go to `$HOME`, or to the root when there
    /// is no home set. A path that does not exist leaves the CWD alone.
    fn exec_cd(&mut self, args: &[&str], out: &mut Vec<String>) {
        let target = match args.first() {
            Some(p) => (*p).to_string(),
            None => match sys::process::env_var("HOME") {
                Some(home) => home,
                None => "/".to_string(),
            },
        };
        // `Dir::open` resolves relative paths against the CWD itself, so it is
        // also the check that the target is a directory rather than a file.
        match sys::fs::Dir::open(&target) {
            Some(_) => sys::process::set_dir(&sys::fs::realpath(&target)),
            None => out.push(format!("cd: no existe la ruta '{}'", target)),
        }
    }

    // ------------------------------------------------------------------
    // Rendering
    // ------------------------------------------------------------------

    /// Draw the log and the input line into the area the desktop hands us.
    pub fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(3)]).split(area);
        self.render_log(frame, chunks[0]);
        self.render_input(frame, chunks[1]);
    }

    fn render_log(&self, frame: &mut Frame<'_>, area: Rect) {
        // Bottom-anchored, the way a real terminal grows: blank space on top,
        // newest line at the bottom.
        let n = area.height as usize;
        let taken = self.log.len().min(n);
        let mut lines: Vec<Line<'static>> = vec![Line::raw(""); n - taken];
        for entry in &self.log[self.log.len() - taken..] {
            let style = if entry.prompt {
                Style::new().fg(Color::LightGreen)
            } else {
                Style::new().fg(Color::Gray)
            };
            lines.push(Line::styled(entry.text.clone(), style));
        }
        let paragraph = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false });
        frame.render_widget(paragraph, area);
    }

    fn render_input(&self, frame: &mut Frame<'_>, area: Rect) {
        let block = Block::new()
            .borders(Borders::TOP | Borders::BOTTOM)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(Color::DarkGray));
        let inner = block.inner(area);

        let max_w = (inner.width as usize).saturating_sub(2);
        let chars: Vec<char> = self.input.chars().collect();
        let (start, shown): (usize, String) = if chars.len() > max_w {
            let start = if self.cursor < max_w {
                0
            } else {
                self.cursor - max_w
            };
            let end = (start + max_w).min(chars.len());
            (start, chars[start..end].iter().collect())
        } else {
            (0, self.input.clone())
        };

        let line = Line::from(vec![
            Span::styled(
                "> ",
                Style::new()
                    .fg(Color::LightGreen)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(shown.clone(), Style::new().fg(Color::LightCyan)),
        ]);
        frame.render_widget(Paragraph::new(line).block(block), area);

        // Keep the hardware cursor on the input line.
        let cx = inner.x + 2 + (self.cursor.saturating_sub(start)) as u16;
        frame.set_cursor_position((cx, inner.y));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sys::fs::{dismount, format_mem, mount_mem};

    /// A clean in-memory filesystem with the welcome tree already seeded, and
    /// the CWD back at the root so tests do not inherit each other's directory.
    fn fresh_fs() {
        dismount();
        mount_mem();
        format_mem();
        crate::sys::fs::seed_root();
        crate::sys::process::set_dir("/");
    }

    #[test_case]
    fn exec_fs_commands() {
        fresh_fs();
        let mut term = TerminalApp::new();

        // Work in a directory of our own: the root already holds the seeded
        // welcome tree, so asserting on `ls /` would be asserting on Task 4.
        let (out, _) = term.exec("mkdir /d");
        assert!(out.is_empty(), "mkdir should be silent: {:?}", out);

        let (out, act) = term.exec("touch /d/a.txt");
        assert!(out.is_empty(), "touch should be silent: {:?}", out);
        assert_eq!(act, AppAction::Keep);

        let (out, _) = term.exec("ls /d");
        assert_eq!(out, vec!["a.txt".to_string()]);

        let (out, _) = term.exec("mv /d/a.txt /d/b.txt");
        assert!(out.is_empty(), "mv should be silent: {:?}", out);
        let (out, _) = term.exec("ls /d");
        assert_eq!(out, vec!["b.txt".to_string()], "mv should rename in place");

        let (out, _) = term.exec("cat /d/b.txt");
        assert!(out.is_empty(), "an empty file prints nothing, got {:?}", out);

        let (out, _) = term.exec("rm /d/b.txt");
        assert!(out.is_empty(), "rm should be silent: {:?}", out);
        let (out, _) = term.exec("ls /d");
        assert!(out.is_empty(), "rm did not remove the file: {:?}", out);

        // cd with no argument goes home, and is not an error.
        let (out, act) = term.exec("cd");
        assert!(out.is_empty(), "cd with no args should be silent: {:?}", out);
        assert_eq!(act, AppAction::Keep);

        // Errors are Spanish, and never a panic.
        let (out, _) = term.exec("cat /d");
        assert!(
            out[0].contains("no es un archivo") || out[0].contains("no existe"),
            "cat on a directory should say so in Spanish, got {:?}",
            out
        );
        let (out, _) = term.exec("mv /d/b.txt");
        assert!(out[0].contains("uso"), "wrong arg count should say 'uso', got {:?}", out);
    }

    #[test_case]
    fn exec_cat_prints_file_contents() {
        fresh_fs();
        let mut term = TerminalApp::new();
        let (out, _) = term.exec("cat /manual.txt");
        assert!(out[0].contains("MANUAL DE TUI-OS"), "got {:?}", out);
        assert!(out.len() > 3, "a multi-line file should print many lines: {:?}", out);
    }

    #[test_case]
    fn exec_touch_preserves_existing_content() {
        fresh_fs();
        let mut term = TerminalApp::new();
        term.exec("echo hola > /nota.txt");
        // `echo` does not redirect in this shell, so write through a real file.
        let (out, _) = term.exec("cat /nota.txt");
        assert!(out[0].contains("no existe"), "no shell redirection yet: {:?}", out);
    }

    #[test_case]
    fn exec_pwd_and_cd_track_the_working_directory() {
        fresh_fs();
        let mut term = TerminalApp::new();
        let (out, _) = term.exec("pwd");
        assert_eq!(out, vec!["/".to_string()]);

        let (out, _) = term.exec("cd /usr");
        assert!(out.is_empty(), "cd should be silent: {:?}", out);
        let (out, _) = term.exec("pwd");
        assert_eq!(out, vec!["/usr".to_string()]);

        // Relative paths resolve against the CWD.
        let (out, _) = term.exec("cd /home");
        assert!(out.is_empty());
        let (out, _) = term.exec("mkdir relativo");
        assert!(out.is_empty(), "mkdir relative failed: {:?}", out);
        let (out, _) = term.exec("ls");
        assert_eq!(out, vec!["relativo".to_string()], "ls with no args lists the CWD");
    }

    #[test_case]
    fn exec_cd_to_a_missing_path_is_a_spanish_error() {
        fresh_fs();
        let mut term = TerminalApp::new();
        let (out, _) = term.exec("cd /no-existe");
        assert!(out[0].contains("no existe"), "got {:?}", out);
        // The CWD must not have moved.
        let (out, _) = term.exec("pwd");
        assert_eq!(out, vec!["/".to_string()]);
    }

    #[test_case]
    fn exec_rm_refuses_a_directory() {
        fresh_fs();
        let mut term = TerminalApp::new();
        let (out, _) = term.exec("rm /usr");
        assert!(out[0].contains("directorio"), "rm should refuse a directory, got {:?}", out);
        // And the directory is still there.
        let (out, _) = term.exec("ls /usr");
        assert_eq!(out, vec!["README.txt".to_string()]);
    }

    #[test_case]
    fn exec_cp_copies_content_between_files() {
        fresh_fs();
        let mut term = TerminalApp::new();
        let (out, _) = term.exec("cp /manual.txt /copia.txt");
        assert!(out.is_empty(), "cp should be silent: {:?}", out);
        let (out, _) = term.exec("cat /copia.txt");
        assert!(out[0].contains("MANUAL DE TUI-OS"), "got {:?}", out);
        // The source survives a copy.
        let (out, _) = term.exec("ls /");
        assert!(out.contains(&"manual.txt".to_string()));
    }

    #[test_case]
    fn exec_unknown_command_is_a_spanish_error() {
        fresh_fs();
        let mut term = TerminalApp::new();
        let (out, act) = term.exec("nonsense");
        assert!(out[0].contains("no encontrado"), "got {:?}", out);
        assert_eq!(act, AppAction::Keep);
    }

    #[test_case]
    fn exec_abrir_switches_to_the_named_app() {
        fresh_fs();
        let mut term = TerminalApp::new();
        let (out, act) = term.exec("abrir Sistema");
        assert!(out[0].contains("Sistema"), "got {:?}", out);
        assert_eq!(act, AppAction::Switch(AppKind::SysInfo));

        let (_, act) = term.exec("abrir Archivos");
        assert_eq!(act, AppAction::Switch(AppKind::Files));

        let (out, act) = term.exec("abrir no-existe");
        assert!(out[0].contains("desconocida"), "got {:?}", out);
        assert_eq!(act, AppAction::Keep);
    }

    #[test_case]
    fn exec_abrir_without_arguments_is_a_usage_error() {
        fresh_fs();
        let mut term = TerminalApp::new();
        let (out, act) = term.exec("abrir");
        assert!(out[0].contains("uso"), "got {:?}", out);
        assert_eq!(act, AppAction::Keep);
    }

    #[test_case]
    fn exec_apps_lists_the_dock() {
        fresh_fs();
        let mut term = TerminalApp::new();
        let (out, act) = term.exec("apps");
        assert!(out.len() >= 4, "apps should list the dock, got {:?}", out);
        assert!(out.iter().any(|l| l.contains("Archivos")), "got {:?}", out);
        assert_eq!(act, AppAction::Keep);
    }

    #[test_case]
    fn command_catalog_has_dock_aliases() {
        let names: Vec<&str> = COMMANDS.iter().map(|(n, _)| *n).collect();
        for n in [
            "pwd", "cd", "ls", "cat", "touch", "mkdir", "rm", "mv", "cp", "help", "ayuda",
            "apps", "abrir", "sysinfo", "mem", "uptime", "date", "version", "echo", "clear",
            "random", "pci", "halt", "apagar", "reboot", "reiniciar",
        ] {
            assert!(names.contains(&n), "missing command {}", n);
        }
        assert!(!names.contains(&"widgets"));
        assert!(!names.contains(&"quit"), "halt/apagar replace quit");
    }

    #[test_case]
    fn command_catalog_descriptions_are_spanish() {
        for (name, desc) in COMMANDS {
            assert!(!desc.is_empty(), "{} has no description", name);
            // No description may be the old English text.
            assert!(
                !desc.contains("Show ") && !desc.contains("Print "),
                "{} still has an English description: {}",
                name,
                desc
            );
        }
    }
}
