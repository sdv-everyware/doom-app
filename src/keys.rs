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

//! The game's keys from the panel: lines in a FIFO, one per key that goes down or up
//! ("down left\n", "up fire\n"), written by the panel's game dialog while its buttons are held
//! (smarthome-app's game.rs). Six keys, two ways: every other line is dropped, a line longer than
//! MAX_LINE is dropped whole. The engine gets the keys as DOOM's key codes (doomgeneric's
//! doomkeys.h, the default bindings of m_controls.c).

use std::collections::VecDeque;
use std::fs::File;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt};

/// A key of the panel's dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    Fire,
    Use,
}

impl Key {
    /// Every key, in the order of `Held`.
    pub const ALL: [Key; 6] = [Key::Up, Key::Down, Key::Left, Key::Right, Key::Fire, Key::Use];

    /// The key's word on a line.
    pub fn word(self) -> &'static str {
        match self {
            Key::Up => "up",
            Key::Down => "down",
            Key::Left => "left",
            Key::Right => "right",
            Key::Fire => "fire",
            Key::Use => "use",
        }
    }

    /// DOOM's code of the key (doomkeys.h): the arrows walk and turn, fire, use (doors, switches,
    /// and after death the next try).
    pub fn code(self) -> u8 {
        match self {
            Key::Up => 0xad,
            Key::Down => 0xaf,
            Key::Left => 0xac,
            Key::Right => 0xae,
            Key::Fire => 0xa3,
            Key::Use => 0xa2,
        }
    }

    fn index(self) -> usize {
        Key::ALL.iter().position(|k| *k == self).unwrap_or(0)
    }
}

/// One line without its newline: the key and whether it went down, or None for anything else.
pub fn parse(line: &[u8]) -> Option<(Key, bool)> {
    let text = std::str::from_utf8(line).ok()?;
    let (way, word) = text.split_once(' ')?;
    let down = match way {
        "down" => true,
        "up" => false,
        _ => return None,
    };
    Key::ALL.into_iter().find(|k| k.word() == word).map(|k| (k, down))
}

/// The longest line taken; "down right" is 10 bytes.
pub const MAX_LINE: usize = 16;

/// Splits the bytes read into lines, across reads.
#[derive(Debug, Default)]
pub struct Lines {
    line: Vec<u8>,
    too_long: bool,
    /// Lines dropped since the start.
    pub dropped: u64,
}

impl Lines {
    /// The keys of the whole lines in `bytes`; a line cut by the read waits for its rest.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<(Key, bool)> {
        let mut out = Vec::new();
        for &b in bytes {
            if b == b'\n' {
                match parse(&self.line) {
                    Some(key) if !self.too_long => out.push(key),
                    _ => self.dropped += 1,
                }
                self.line.clear();
                self.too_long = false;
            } else if self.line.len() < MAX_LINE {
                self.line.push(b);
            } else {
                self.too_long = true;
            }
        }
        out
    }
}

/// Which keys the engine was told are down.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Held([bool; 6]);

impl Held {
    /// Takes a key that went down or up.
    pub fn set(&mut self, key: Key, down: bool) {
        self.0[key.index()] = down;
    }

    /// Whether `key` is down.
    pub fn is_down(&self, key: Key) -> bool {
        self.0[key.index()]
    }

    /// Every key up: the ones that were down, to tell the engine (the game pauses while a finger
    /// may still be on a button).
    pub fn release(&mut self) -> Vec<Key> {
        let down: Vec<Key> = Key::ALL.into_iter().filter(|k| self.is_down(*k)).collect();
        *self = Held::default();
        down
    }
}

/// The most keys waiting for the engine. A writer that sends faster than the engine takes them
/// loses its new downs; an up is never lost (a key would stay down): a full queue makes room for
/// it by dropping an older down, or an older up of a key that has a later one.
pub const MAX_QUEUE: usize = 64;

/// The keys for the engine's next DG_GetKey calls, in the order they came.
#[derive(Debug, Default)]
pub struct KeyQueue {
    keys: VecDeque<(Key, bool)>,
    /// Keys dropped because the queue was full.
    pub dropped: u64,
}

impl KeyQueue {
    /// Adds a key. Full: a down is dropped; an up takes the place of the oldest down, or of the
    /// oldest up that a later up of its key repeats (there is one: 64 ups of six keys).
    pub fn push(&mut self, key: Key, down: bool) {
        if self.keys.len() >= MAX_QUEUE {
            self.dropped += 1;
            if down {
                return;
            }
            let repeated = |i: usize| {
                let k = self.keys[i].0;
                k == key || self.keys.iter().skip(i + 1).any(|(later, d)| *later == k && !*d)
            };
            let room = self.keys.iter().position(|(_, d)| *d).or_else(|| (0..self.keys.len()).find(|&i| repeated(i)));
            if let Some(i) = room {
                self.keys.remove(i);
            }
        }
        self.keys.push_back((key, down));
    }

    /// The oldest key.
    pub fn pop(&mut self) -> Option<(Key, bool)> {
        self.keys.pop_front()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

/// The most bytes taken from the FIFO in one read: a writer that keeps the pipe full costs the
/// engine one bounded read, not a loop.
pub const MAX_READ: usize = 1024;

/// What to do with the FIFO found at the path (`fifo_check`).
#[derive(Debug, PartialEq, Eq)]
pub enum Found {
    /// Ours and private enough: use it.
    Take,
    /// Ours, but others may have it open (bits for others): make a new one.
    Remake,
    /// Not ours or not a FIFO: refuse, with why.
    Refuse(String),
}

/// Whether a file at the FIFO's path (`is_fifo`, its owner `uid`, its `mode`) may be the keys'
/// FIFO of a process of `euid`.
pub fn fifo_check(is_fifo: bool, uid: u32, mode: u32, euid: u32) -> Found {
    if !is_fifo {
        Found::Refuse("not a FIFO".to_string())
    } else if uid != euid {
        Found::Refuse(format!("owned by uid {uid}, not by this process ({euid})"))
    } else if mode & 0o007 != 0 {
        Found::Remake
    } else {
        Found::Take
    }
}

/// The FIFO the panel writes the keys to, opened to read without ever blocking.
pub struct Fifo {
    file: File,
}

impl Fifo {
    /// Opens the FIFO at `path`, made first when there is none. What is found there is checked
    /// on the open file, not by its path: no link is followed (O_NOFOLLOW), and it must be a
    /// FIFO of this process's user with nothing for others (`fifo_check`); one with bits for
    /// others is made again. Then rw for the owner and the group (the panel writes it), set on
    /// the open file. Opened for reading and writing, so the panel's closing it after every line
    /// is no end of file.
    pub fn open(path: &str) -> std::io::Result<Fifo> {
        let c = std::ffi::CString::new(std::path::Path::new(path).as_os_str().as_bytes())
            .map_err(|_| std::io::Error::other("a NUL in the path"))?;
        // SAFETY: no arguments; geteuid cannot fail.
        let euid = unsafe { libc::geteuid() };
        for _ in 0..2 {
            // SAFETY: a valid C string; mkfifo creates a file and nothing else (owner only first).
            if unsafe { libc::mkfifo(c.as_ptr(), 0o600) } != 0 {
                let e = std::io::Error::last_os_error();
                if e.raw_os_error() != Some(libc::EEXIST) {
                    return Err(e);
                }
            }
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(path)?;
            let meta = file.metadata()?;
            match fifo_check(meta.file_type().is_fifo(), meta.uid(), meta.mode(), euid) {
                Found::Take => {
                    // SAFETY: the fd is open for the call; fchmod changes this file only.
                    if unsafe { libc::fchmod(file.as_raw_fd(), 0o660) } != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    return Ok(Fifo { file });
                }
                Found::Remake => {
                    drop(file);
                    // unlink removes the name, never a file a link points to
                    std::fs::remove_file(path)?;
                }
                Found::Refuse(why) => return Err(std::io::Error::other(why)),
            }
        }
        Err(std::io::Error::other("made again, still not private"))
    }

    /// What the panel wrote since the last read, at most MAX_READ bytes; empty when nothing
    /// waits.
    pub fn read(&mut self) -> Vec<u8> {
        let mut buf = vec![0u8; MAX_READ];
        loop {
            match self.file.read(&mut buf) {
                Ok(n) => {
                    buf.truncate(n);
                    return buf;
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                // WouldBlock: nothing now
                Err(_) => return Vec::new(),
            }
        }
    }

    /// Drops what waits in the FIFO (a panel that started again holds no key; lines of the one
    /// before must not press one). At most the pipe's capacity.
    pub fn discard(&mut self) {
        for _ in 0..(64 * 1024 / MAX_READ) {
            if self.read().is_empty() {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn twelve_lines_and_nothing_else() {
        assert_eq!(parse(b"down left"), Some((Key::Left, true)));
        assert_eq!(parse(b"up fire"), Some((Key::Fire, false)));
        assert_eq!(parse(b"down use"), Some((Key::Use, true)));
        for bad in [&b"down"[..], b"left", b"down  left", b"down left ", b"DOWN left", b"down jump", b"", b"\xff\xfe"] {
            assert_eq!(parse(bad), None, "{bad:?}");
        }
        for k in Key::ALL {
            assert_eq!(parse(format!("down {}", k.word()).as_bytes()), Some((k, true)));
            assert_eq!(parse(format!("up {}", k.word()).as_bytes()), Some((k, false)));
        }
    }

    #[test]
    fn the_keys_are_doom_s_default_bindings() {
        // doomkeys.h: KEY_UPARROW 0xad, KEY_DOWNARROW 0xaf, KEY_LEFTARROW 0xac,
        // KEY_RIGHTARROW 0xae, KEY_FIRE 0xa3, KEY_USE 0xa2 (m_controls.c binds them)
        assert_eq!(Key::ALL.map(Key::code), [0xad, 0xaf, 0xac, 0xae, 0xa3, 0xa2]);
    }

    #[test]
    fn lines_across_reads_and_long_ones_dropped() {
        let mut lines = Lines::default();
        assert_eq!(lines.feed(b"down le"), vec![]);
        assert_eq!(lines.feed(b"ft\nup left\ndown fi"), vec![(Key::Left, true), (Key::Left, false)]);
        assert_eq!(lines.feed(b"re\n"), vec![(Key::Fire, true)]);
        let long = format!("down {}\nup use\n", "x".repeat(100));
        assert_eq!(lines.feed(long.as_bytes()), vec![(Key::Use, false)]);
        assert_eq!(lines.dropped, 1);
        assert_eq!(lines.feed(b"hello\n\n"), vec![]);
        assert_eq!(lines.dropped, 3);
    }

    #[test]
    fn a_pause_lets_every_held_key_go() {
        let mut held = Held::default();
        held.set(Key::Left, true);
        held.set(Key::Fire, true);
        held.set(Key::Fire, false);
        held.set(Key::Use, true);
        assert_eq!(held.release(), vec![Key::Left, Key::Use]);
        assert_eq!(held.release(), vec![]);
    }

    #[test]
    fn a_full_queue_drops_downs_and_never_an_up() {
        let mut q = KeyQueue::default();
        q.push(Key::Left, true);
        for _ in 0..(MAX_QUEUE - 2) {
            q.push(Key::Fire, true);
        }
        q.push(Key::Left, false);
        // full: a new down is dropped
        q.push(Key::Use, true);
        assert_eq!(q.dropped, 1);
        // a new up takes the oldest down's place (Left's down; its up stays)
        q.push(Key::Use, false);
        assert_eq!(q.dropped, 2);
        let all: Vec<_> = std::iter::from_fn(|| q.pop()).collect();
        assert_eq!(all.len(), MAX_QUEUE);
        assert_eq!(all[0], (Key::Fire, true));
        assert!(all.contains(&(Key::Left, false)));
        assert!(!all.contains(&(Key::Use, true)));
        assert_eq!(all.last(), Some(&(Key::Use, false)));
        // only ups: an up of the same key earlier goes, every key keeps its last up
        let mut q = KeyQueue::default();
        q.push(Key::Down, false);
        for _ in 0..(MAX_QUEUE - 1) {
            q.push(Key::Up, false);
        }
        q.push(Key::Right, false);
        let all: Vec<_> = std::iter::from_fn(|| q.pop()).collect();
        assert_eq!(all.len(), MAX_QUEUE);
        assert_eq!(all[0], (Key::Down, false), "Down's only up stays");
        assert_eq!(all.last(), Some(&(Key::Right, false)));
        assert!(all.contains(&(Key::Up, false)));
    }

    #[test]
    fn only_a_private_fifo_of_ours_is_taken() {
        assert_eq!(fifo_check(true, 1000, 0o10600, 1000), Found::Take);
        assert_eq!(fifo_check(true, 1000, 0o10660, 1000), Found::Take);
        assert_eq!(fifo_check(true, 1000, 0o10666, 1000), Found::Remake, "others may have it open");
        assert_eq!(fifo_check(true, 1000, 0o10602, 1000), Found::Remake);
        assert!(matches!(fifo_check(true, 0, 0o10600, 1000), Found::Refuse(_)), "another user's");
        assert!(matches!(fifo_check(false, 1000, 0o100600, 1000), Found::Refuse(_)), "a plain file");
    }

    #[test]
    fn the_fifo_is_made_and_read_without_blocking() {
        let dir = std::env::temp_dir().join(format!("doom-keys-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("keys");
        let p = path.to_str().unwrap();
        let mut fifo = Fifo::open(p).unwrap();
        assert_eq!(fifo.read(), Vec::<u8>::new(), "nothing written: no wait");
        // the panel's side: a writer that opens, writes a line and closes, twice
        for line in ["down up\n", "up up\n"] {
            let mut w = std::fs::OpenOptions::new().write(true).custom_flags(libc::O_NONBLOCK).open(&path).unwrap();
            w.write_all(line.as_bytes()).unwrap();
        }
        let mut lines = Lines::default();
        assert_eq!(lines.feed(&fifo.read()), vec![(Key::Up, true), (Key::Up, false)]);
        assert_eq!(fifo.read(), Vec::<u8>::new(), "the writers' close is no end of file");
        // made private: rw for the owner and the group only
        let mode = std::fs::metadata(&path).unwrap().mode() & 0o777;
        assert_eq!(mode, 0o660);
        // a read takes at most MAX_READ bytes; discard empties the pipe
        {
            let mut w = std::fs::OpenOptions::new().write(true).custom_flags(libc::O_NONBLOCK).open(&path).unwrap();
            w.write_all(&vec![b'x'; 3 * MAX_READ]).unwrap();
        }
        assert_eq!(fifo.read().len(), MAX_READ);
        fifo.discard();
        assert!(fifo.read().is_empty());
        // opened again: the FIFO that is there is taken
        drop(fifo);
        assert!(Fifo::open(p).is_ok());
        // one others could open is made again, private: a writer that opened the old one
        // reaches nothing
        std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o666)).unwrap();
        let _reader = std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK).open(&path).unwrap();
        let mut old = std::fs::OpenOptions::new().write(true).custom_flags(libc::O_NONBLOCK).open(&path).unwrap();
        let mut fifo = Fifo::open(p).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, 0o660);
        old.write_all(b"down fire\n").unwrap();
        assert!(fifo.read().is_empty(), "the old FIFO's writer does not reach the new one");
        drop(fifo);
        // a link in its place is not followed
        std::fs::remove_file(&path).unwrap();
        let target = dir.join("target");
        std::fs::write(&target, b"").unwrap();
        std::fs::set_permissions(&target, std::os::unix::fs::PermissionsExt::from_mode(0o600)).unwrap();
        std::os::unix::fs::symlink(&target, &path).unwrap();
        assert!(Fifo::open(p).is_err());
        assert_eq!(std::fs::metadata(&target).unwrap().mode() & 0o777, 0o600, "the link's target untouched");
        // a plain file in its place is refused
        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, b"").unwrap();
        assert!(Fifo::open(p).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
