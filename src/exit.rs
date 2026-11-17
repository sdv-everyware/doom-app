// This program is free software: you can redistribute it and/or modify it under the terms of
// the GNU General Public License as published by the Free Software Foundation, either version 3
// of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See
// the GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License along with this program.
// If not, see <https://www.gnu.org/licenses/>.

//! The FIFO goes with the game: removed when doom_app ends, by exit (also the engine's own, from
//! I_Quit or I_Error) or by SIGTERM or SIGINT. A FIFO left behind would tell the panel that a game
//! is there: on an enforcing board the panel cannot open it (EACCES) and takes that for a game
//! whose keys are blocked. Only the file this process made or took is removed (its device and
//! inode), never one that took its place.

use std::ffi::{c_int, CString};
use std::os::unix::ffi::OsStrExt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

static PATH: OnceLock<CString> = OnceLock::new();
static DEV: AtomicU64 = AtomicU64::new(0);
static INO: AtomicU64 = AtomicU64::new(0);

/// Removes the FIFO at `path` when the process ends, if it is still the file `id` (device,
/// inode). Once per process.
pub fn remove_at_exit(path: &str, id: (u64, u64)) -> Result<(), String> {
    let c = CString::new(std::path::Path::new(path).as_os_str().as_bytes()).map_err(|_| "a NUL in the FIFO's path".to_string())?;
    DEV.store(id.0, Ordering::SeqCst);
    INO.store(id.1, Ordering::SeqCst);
    PATH.set(c).map_err(|_| "the FIFO's removal is set already".to_string())?;
    // SAFETY: the handlers only read the statics above and call async-signal-safe functions
    // (lstat, unlink, signal, raise); atexit and sigaction take function pointers that live for
    // the process.
    unsafe {
        if libc::atexit(at_exit) != 0 {
            return Err("atexit failed".to_string());
        }
        for sig in [libc::SIGTERM, libc::SIGINT] {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = on_signal as extern "C" fn(c_int) as usize;
            libc::sigemptyset(&mut action.sa_mask);
            if libc::sigaction(sig, &action, std::ptr::null_mut()) != 0 {
                return Err(format!("sigaction {sig}: {}", std::io::Error::last_os_error()));
            }
        }
    }
    Ok(())
}

/// Removes the FIFO at `path` if it is the file `id` and a FIFO; whether it did. Only
/// async-signal-safe calls: it runs in a signal handler too.
// (dev_t and ino_t are u64 on some C libraries and not on others)
#[allow(clippy::unnecessary_cast)]
pub fn remove_if_same(path: &CString, id: (u64, u64)) -> bool {
    // SAFETY: `path` is a C string; `st` is a valid out pointer; lstat and unlink touch nothing
    // else.
    unsafe {
        let mut st: libc::stat = std::mem::zeroed();
        if libc::lstat(path.as_ptr(), &mut st) != 0 {
            return false;
        }
        let same = st.st_dev as u64 == id.0 && st.st_ino as u64 == id.1 && st.st_mode & libc::S_IFMT == libc::S_IFIFO;
        same && libc::unlink(path.as_ptr()) == 0
    }
}

fn remove() {
    if let Some(path) = PATH.get() {
        remove_if_same(path, (DEV.load(Ordering::SeqCst), INO.load(Ordering::SeqCst)));
    }
}

extern "C" fn at_exit() {
    remove();
}

extern "C" fn on_signal(sig: c_int) {
    remove();
    // the signal's default action now: the process ends as killed by it
    // SAFETY: signal and raise are async-signal-safe.
    unsafe {
        libc::signal(sig, libc::SIG_DFL);
        libc::raise(sig);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    #[test]
    fn only_the_fifo_it_took_is_removed() {
        let dir = std::env::temp_dir().join(format!("doom-exit-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("keys");
        let c = CString::new(path.to_str().unwrap()).unwrap();
        let fifo = crate::keys::Fifo::open(path.to_str().unwrap()).unwrap();
        let id = fifo.id();
        let meta = std::fs::metadata(&path).unwrap();
        assert_eq!(id, (meta.dev(), meta.ino()));
        // another file in its place stays
        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, b"").unwrap();
        assert!(!remove_if_same(&c, id));
        assert!(path.exists());
        std::fs::remove_file(&path).unwrap();
        // the one it took goes; nothing there: nothing to do
        drop(fifo);
        let fifo = crate::keys::Fifo::open(path.to_str().unwrap()).unwrap();
        assert!(remove_if_same(&c, fifo.id()));
        assert!(!path.exists());
        assert!(!remove_if_same(&c, fifo.id()));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
