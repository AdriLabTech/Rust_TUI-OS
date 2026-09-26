mod bitmap_block;
mod block;
mod block_device;
mod device;
mod dir;
mod dir_entry;
mod file;
mod io;
mod pipe;
mod read_dir;
mod super_block;

pub use io::{FileIO, IO};
pub use bitmap_block::BITMAP_SIZE;
pub use block_device::{
    dismount, format_ata, format_mem, is_mounted, mount_ata, mount_mem
};
pub use device::{Device, device_type};
pub use dir::Dir;
pub use dir_entry::FileInfo;
pub use file::{File, SeekFrom};

pub use crate::api::fs::{dirname, filename};
pub use crate::sys::ata::BLOCK_SIZE;

pub use block_device::block_count;
use block_device::BlockDeviceIO;
use dir_entry::DirEntry;
use super_block::SuperBlock;

use crate::sys::process;

use alloc::format;
use alloc::string::{String, ToString};
use core::convert::TryFrom;
use core::ops::BitOr;

pub const VERSION: u8 = 2;

// Duplicate of `api::fs::realpath`, using `process::dir()` from `sys` instead
// of `api` to bypass syscall overhead.
pub fn realpath(pathname: &str) -> String {
    if pathname.starts_with('/') {
        pathname.into()
    } else {
        let dirname = process::dir();
        let sep = if dirname.ends_with('/') { "" } else { "/" };
        format!("{}{}{}", dirname, sep, pathname)
    }
}

// TODO: Move that to API
#[derive(Clone, Copy)]
#[repr(u8)]
pub enum OpenFlag {
    Read     = 1,
    Write    = 2,
    Append   = 4,
    Create   = 8,
    Truncate = 16,
    Dir      = 32,
    Device   = 64,
}

impl OpenFlag {
    fn is_set(&self, flags: u8) -> bool {
        flags & (*self as u8) != 0
    }
}

impl BitOr for OpenFlag {
   type Output = u8;

   fn bitor(self, rhs: Self) -> Self::Output {
       (self as u8) | (rhs as u8)
   }
}

pub fn open(path: &str, flags: u8) -> Option<Resource> {
    if OpenFlag::Dir.is_set(flags) {
        let res = Dir::open(path);
        if res.is_none() && OpenFlag::Create.is_set(flags) {
            Dir::create(path)
        } else {
            res
        }.map(Resource::Dir)
    } else if OpenFlag::Device.is_set(flags) {
        let res = Device::open(path);
        if res.is_none() && OpenFlag::Create.is_set(flags) {
            Device::create(path)
        } else {
            res
        }.map(Resource::Device)
    } else {
        let mut res = File::open(path);
        if res.is_none() && OpenFlag::Create.is_set(flags) {
            File::create(path)
        } else {
            if OpenFlag::Append.is_set(flags) {
                if let Some(ref mut file) = res {
                    file.seek(SeekFrom::End(0)).ok();
                }
            }
            res
        }.map(Resource::File)
    }
}

pub fn delete(path: &str) -> Result<(), ()> {
    if let Some(info) = info(path) {
        if info.is_dir() {
            return Dir::delete(path);
        } else if info.is_file() || info.is_device() {
            return File::delete(path);
        }
    }
    Err(())
}

pub fn info(pathname: &str) -> Option<FileInfo> {
    if pathname == "/" {
        return Some(FileInfo::root());
    }
    DirEntry::open(pathname).map(|e| e.info())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    Dir = 0,
    File = 1,
    Device = 2,
}

impl TryFrom<usize> for FileType {
    type Error = ();

    fn try_from(num: usize) -> Result<Self, Self::Error> {
        match num {
             0 => Ok(FileType::Dir),
             1 => Ok(FileType::File),
             2 => Ok(FileType::Device),
             _ => Err(()),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Resource {
    Dir(Dir),
    File(File),
    Device(Device),
}

impl Resource {
    pub fn kind(&self) -> FileType {
        match self {
            Resource::Dir(_) => FileType::Dir,
            Resource::File(_) => FileType::File,
            Resource::Device(_) => FileType::Device,
        }
    }
}

impl FileIO for Resource {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, ()> {
        match self {
            Resource::Dir(io) => io.read(buf),
            Resource::File(io) => io.read(buf),
            Resource::Device(io) => io.read(buf),
        }
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize, ()> {
        match self {
            Resource::Dir(io) => io.write(buf),
            Resource::File(io) => io.write(buf),
            Resource::Device(io) => io.write(buf),
        }
    }

    fn close(&mut self) {
        match self {
            Resource::Dir(io) => io.close(),
            Resource::File(io) => io.close(),
            Resource::Device(io) => io.close(),
        }
    }

    fn poll(&mut self, event: IO) -> bool {
        match self {
            Resource::Dir(io) => io.poll(event),
            Resource::File(io) => io.poll(event),
            Resource::Device(io) => io.poll(event),
        }
    }
}

pub fn canonicalize(path: &str) -> Result<String, ()> {
    match process::env_var("HOME") {
        Some(home) => {
            if path.starts_with('~') {
                Ok(path.replace('~', &home))
            } else {
                Ok(path.to_string())
            }
        }
        None => Ok(path.to_string()),
    }
}

pub fn disk_size() -> usize {
    (SuperBlock::read().block_count() as usize) * BLOCK_SIZE
}

pub fn disk_used() -> usize {
    (SuperBlock::read().alloc_count() as usize) * BLOCK_SIZE
}

pub fn disk_free() -> usize {
    disk_size() - disk_used()
}

pub fn init() {
    for bus in 0..2 {
        for dsk in 0..2 {
            if SuperBlock::check_ata(bus, dsk) {
                log!("MFS Superblock found in ATA {}:{}", bus, dsk);
                mount_ata(bus, dsk);
                seed_root();
                return;
            }
        }
    }
    // No filesystem on any drive: mount the first one and format it. Mounting
    // has to come first — `format_ata` needs a block device to write the
    // superblock through.
    log!("TUI-OS: no MFS superblock found, formatting ATA 0:0");
    mount_ata(0, 0);
    if !is_mounted() {
        // No drive answered at all. Nothing to format, nowhere to seed, and
        // every MFS call would quietly degrade to zeroed blocks — so say so on
        // the log and carry on booting rather than pretending there is a disk.
        log!("TUI-OS: no ATA drive found, running without a filesystem");
        return;
    }
    format_ata();
    seed_root();
}

/// Create the first-boot directory tree and its welcome files.
///
/// Idempotent: a tree that already holds `bienvenida.txt` is left alone, so
/// this is safe to call on every boot.
///
/// **Paths must be absolute literals, and nothing here may touch the process
/// table.** `init` runs at `lib.rs:58` but `process::init` only runs at `:59`,
/// and `process::dir`/`canonicalize`/`env_var` all reach `current_process`,
/// which is `table[id()].unwrap()` — before `process::init` the table is empty,
/// so calling any of them from here panics the machine on a fresh boot.
/// Absolute paths are safe because `realpath` returns them without consulting
/// the CWD, and `Dir::root` reads the superblock directly.
pub fn seed_root() {
    if !is_mounted() {
        return;
    }
    if Dir::root().find("bienvenida.txt").is_some() {
        return; // already seeded
    }

    // `create_dir` takes `&mut self`, so the root needs a name of its own.
    let mut root = Dir::root();
    for dir in ["usr", "home", "tmp", "etc"] {
        if root.create_dir(dir).is_none() {
            log!("TUI-OS: could not create /{}", dir);
        }
    }

    write_file(
        "/bienvenida.txt",
        "Bienvenido a TUI-OS v0.1.0\n\
         \n\
         Un sistema operativo 100% Rust con escritorio de texto.\n\
         \n\
         El dock de abajo lanza las aplicaciones a pantalla completa:\n\
         flechas para elegir, Enter para abrir, Esc para volver.\n\
         \n\
         En la terminal, 'help' lista los comandos.\n",
    );
    write_file(
        "/manual.txt",
        "MANUAL DE TUI-OS\n\
         \n\
         1. Escritorio: dock abajo, Enter abre, Esc vuelve.\n\
         2. Terminal: 'ls', 'cd', 'cat', 'mkdir', 'touch', 'rm', 'mv', 'cp'.\n\
         3. Archivos: crea, renombra, borra, copia y mueve.\n\
         4. La primera vez, el disco se formatea y se siembran estos archivos.\n",
    );
    write_file(
        "/usr/README.txt",
        "Directorio /usr\n\
         \n\
         Archivos de sistema. Los tuyos van en /home.\n",
    );
}

/// Write `content` to `pathname`, replacing anything already there.
///
/// `pathname` must be absolute (see `seed_root`). Failures are logged, not
/// propagated: seeding must never take the boot down with it.
fn write_file(pathname: &str, content: &str) {
    let flags = OpenFlag::Write as u8 | OpenFlag::Create as u8 | OpenFlag::Truncate as u8;
    match open(pathname, flags) {
        Some(mut res) => {
            if let Err(()) = res.write(content.as_bytes()) {
                log!("TUI-OS: could not write {}", pathname);
            }
            res.close();
        }
        None => log!("TUI-OS: could not create {}", pathname),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A clean in-memory filesystem, independent of what other tests did to
    /// the global block device.
    fn fresh_mem_fs() {
        dismount();
        mount_mem();
        format_mem();
    }

    #[test_case]
    fn seed_root_creates_welcome_files() {
        fresh_mem_fs();
        seed_root();
        assert!(Dir::open("/").unwrap().find("bienvenida.txt").is_some());
        assert!(Dir::open("/").unwrap().find("manual.txt").is_some());
        assert!(Dir::open("/usr").unwrap().find("README.txt").is_some());
        let mut f = File::open("/bienvenida.txt").unwrap();
        let text = f.read_to_string();
        assert!(text.contains("TUI-OS"));
    }

    #[test_case]
    fn seed_root_creates_the_directory_tree() {
        fresh_mem_fs();
        seed_root();
        for dir in ["/usr", "/home", "/tmp", "/etc"] {
            assert!(Dir::open(dir).is_some(), "missing directory {}", dir);
        }
    }

    #[test_case]
    fn seed_root_is_idempotent() {
        fresh_mem_fs();
        seed_root();
        let before = Dir::open("/").unwrap().entries().count();
        seed_root();
        let after = Dir::open("/").unwrap().entries().count();
        assert_eq!(before, after, "seeding twice changed the root");
    }

    /// R26: `init` runs before `process::init`, and `make test` boots with no
    /// drive at all. Seeding without a device must be a no-op, not a panic.
    #[test_case]
    fn seed_root_without_a_filesystem_does_not_panic() {
        dismount();
        seed_root();
        assert!(!is_mounted());
    }

    /// R25: `format_mem` opens with `debug_assert!(is_mounted())`, so formatting
    /// before mounting is a debug panic and a silent no-op in release. Mounting
    /// first is the only order that leaves a usable filesystem.
    #[test_case]
    fn format_after_mount_produces_a_usable_filesystem() {
        dismount();
        assert!(!is_mounted());
        mount_mem();
        assert!(is_mounted());
        format_mem();
        assert!(disk_size() > 0);
        assert!(Dir::create("/probando").is_some());
    }

    /// `AtaBlockDevice::read` used to compute `block_count() - block_addr`,
    /// which aborts the kernel on the underflow. `make test` boots with no
    /// drive, but an IDENTIFY still answers, so `init` mounts a device with
    /// untrustworthy geometry and MFS addresses blocks past its end. This is
    /// the regression that aborts `make test mode=debug`; a read past the end
    /// has to be an error, because `Block::read` already degrades gracefully
    /// on `Err` by keeping its zeroed buffer.
    #[test_case]
    fn reading_past_the_end_of_the_device_is_an_error() {
        dismount();
        mount_ata(0, 0);
        let count = match block_count() {
            Some(n) => n,
            // No drive answers at all here; the guard is exercised by the debug
            // boot in that case, and there is nothing to read.
            None => {
                dismount();
                return;
            }
        };
        let mut dev = match block_device::take_for_test() {
            Some(dev) => dev,
            None => {
                dismount();
                return;
            }
        };
        let mut buf = [0u8; BLOCK_SIZE];
        // One block past the end of the device.
        let past_end = count as u32;
        let past_the_end = dev.read(past_end, &mut buf);
        assert!(
            past_the_end.is_err(),
            "a read past the end of the device must be an error"
        );
        // The buffer is left untouched, so a caller that ignores the error
        // still sees zeroes rather than stale data.
        assert_eq!(&buf[..], &[0u8; BLOCK_SIZE][..]);
        block_device::put_for_test(dev);
        dismount();
    }
}
