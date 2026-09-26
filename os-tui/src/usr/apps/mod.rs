//! The applications the desktop can run.
//!
//! Each app is fullscreen: the desktop owns the title bar, the hint bar and
//! the status bar, and hands the rest of the screen to whichever app is open.
//! Apps never talk to the hardware directly — they take a decoded key, return
//! an [`AppAction`], and draw themselves into the area they are given.

pub mod files;
pub mod help;
pub mod sysinfo;
pub mod terminal;

/// One of the apps the dock can launch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppKind {
    Terminal,
    Files,
    SysInfo,
    Help,
}

/// What the desktop should do after an app has handled a key.
///
/// `Switch` is the one addition to the spec's `Keep`/`Close`: the terminal's
/// `help` and `sysinfo` commands have to bring another app to the front, and
/// the F-keys alone cannot express that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppAction {
    /// Stay in this app.
    Keep,
    /// Close this app and go back to the desktop.
    Close,
    /// Replace the open app with another one.
    Switch(AppKind),
}

/// The dock contents: each app with its label and its one-line description.
///
/// The dock index runs over this list; the shutdown entry sits one past its
/// end, so it is deliberately not an app.
pub static APPS: &[(AppKind, &str, &str)] = &[
    (AppKind::Files, "Archivos", "Explorador de archivos"),
    (AppKind::Terminal, "Terminal", "Shell con comandos"),
    (AppKind::SysInfo, "Sistema", "Información del sistema"),
    (AppKind::Help, "Ayuda", "Comandos y atajos"),
];

impl AppKind {
    /// The dock index that launches this app.
    pub fn index(self) -> usize {
        APPS
            .iter()
            .position(|(kind, _, _)| *kind == self)
            .unwrap_or(0)
    }

    /// The label shown in the dock.
    pub fn label(self) -> &'static str {
        APPS[self.index()].1
    }

    /// The one-line description shown in the dock tooltip line.
    pub fn description(self) -> &'static str {
        APPS[self.index()].2
    }
}
