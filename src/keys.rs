// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! The game's keys from the panel: lines in a FIFO, one per key that goes down or up
//! ("down left\n", "up fire\n"), written by the panel's game dialog while its buttons are held
//! (smarthome-app's game.rs). Six keys, two ways: every other line is dropped, a line longer than
//! MAX_LINE is dropped whole. The engine gets the keys as DOOM's key codes (doomgeneric's
//! doomkeys.h, the default bindings of m_controls.c).

use std::fs::File;
use std::io::Read;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{FileTypeExt, OpenOptionsExt, PermissionsExt};

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

/// The FIFO the panel writes the keys to, opened to read without ever blocking.
pub struct Fifo {
    file: File,
}

impl Fifo {
    /// Opens the FIFO at `path`, made first when there is none (rw for the owner and the group:
    /// the panel writes it). Opened for reading and writing, so the panel's closing it after
    /// every line is no end of file; no link is followed.
    pub fn open(path: &str) -> std::io::Result<Fifo> {
        let c = std::ffi::CString::new(std::path::Path::new(path).as_os_str().as_bytes())
            .map_err(|_| std::io::Error::other("a NUL in the path"))?;
        // SAFETY: a valid C string; mkfifo creates a file and nothing else.
        if unsafe { libc::mkfifo(c.as_ptr(), 0o660) } != 0 {
            let e = std::io::Error::last_os_error();
            if e.raw_os_error() != Some(libc::EEXIST) {
                return Err(e);
            }
        } else {
            // the umask may have taken the group's write away
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o660))?;
        }
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)?;
        if !file.metadata()?.file_type().is_fifo() {
            return Err(std::io::Error::other("not a FIFO"));
        }
        Ok(Fifo { file })
    }

    /// What the panel wrote since the last read; empty when nothing waits.
    pub fn read(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut buf = [0u8; 256];
        loop {
            match self.file.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => out.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                // WouldBlock: nothing more now
                Err(_) => break,
            }
        }
        out
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
        // opened again: the FIFO that is there is taken
        drop(fifo);
        assert!(Fifo::open(p).is_ok());
        // a plain file in its place is refused
        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, b"").unwrap();
        assert!(Fifo::open(p).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
