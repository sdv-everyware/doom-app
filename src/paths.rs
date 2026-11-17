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

//! Where the game meets the panel: the panel's Wayland socket and the FIFO of the keys next to
//! it, found as the panel finds them (wayland_bridge's socket_path; smarthome-app's game.rs).
//! And whom the files the engine parses may belong to.

/// The panel's Wayland socket: `given` (--socket, else HAR_WAYLAND_SOCKET, else har-display), a
/// path as it is, a name in the runtime directory (XDG_RUNTIME_DIR, without one /data/local/tmp).
pub fn socket(given: Option<&str>, runtime_dir: Option<&str>) -> String {
    let name = given.filter(|s| !s.is_empty()).unwrap_or("har-display");
    if name.starts_with('/') {
        return name.to_string();
    }
    format!("{}/{name}", dir(runtime_dir))
}

/// The FIFO of the keys: `given` (--keys, else HAR_GAME_KEYS), else har-game-keys in the
/// runtime directory.
pub fn keys(given: Option<&str>, runtime_dir: Option<&str>) -> String {
    match given.filter(|s| !s.is_empty()) {
        Some(path) => path.to_string(),
        None => format!("{}/har-game-keys", dir(runtime_dir)),
    }
}

/// The runtime directory: XDG_RUNTIME_DIR when it is a path, else the one of a run from adb.
fn dir(runtime_dir: Option<&str>) -> &str {
    match runtime_dir {
        Some(d) if d.starts_with('/') => d.trim_end_matches('/'),
        _ => "/data/local/tmp",
    }
}

/// A file's owner and mode (st_uid, st_mode).
#[derive(Clone, Copy, Debug)]
pub struct Owner {
    pub uid: u32,
    pub mode: u32,
}

/// Whether the engine may parse the WAD (`file`) in its directory (`dir`) as `euid`: both belong
/// to root or to this user, and nobody else may write them. Run as root, both must be root's.
pub fn wad_trusted(file: Owner, dir: Owner, euid: u32) -> Result<(), String> {
    for (what, o) in [("the WAD", file), ("its directory", dir)] {
        if o.uid != 0 && o.uid != euid {
            return Err(format!("{what} belongs to uid {}, not to root or to this user ({euid})", o.uid));
        }
        if o.mode & 0o022 != 0 {
            return Err(format!("{what} may be written by others (mode {:o})", o.mode & 0o7777));
        }
    }
    Ok(())
}

/// Whether the directory of the settings and saves (`dir`) is `euid`'s alone.
pub fn data_dir_private(dir: Owner, euid: u32) -> Result<(), String> {
    if dir.uid != euid {
        return Err(format!("belongs to uid {}, not to this user ({euid})", dir.uid));
    }
    if dir.mode & 0o077 != 0 {
        return Err(format!("is open to others (mode {:o}; 0700 wanted)", dir.mode & 0o7777));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn o(uid: u32, mode: u32) -> Owner {
        Owner { uid, mode }
    }

    #[test]
    fn the_wad_belongs_to_root_or_the_game_and_nobody_else_writes_it() {
        // the image's: root, read-only
        assert!(wad_trusted(o(0, 0o100644), o(0, 0o40755), 2000).is_ok());
        assert!(wad_trusted(o(0, 0o100644), o(0, 0o40755), 0).is_ok());
        // a dev run's own copy
        assert!(wad_trusted(o(2000, 0o100644), o(2000, 0o40700), 2000).is_ok());
        // run as root: the shell user's copy is not taken
        assert!(wad_trusted(o(2000, 0o100644), o(2000, 0o40755), 0).is_err());
        assert!(wad_trusted(o(0, 0o100644), o(2000, 0o40771), 0).is_err(), "a directory the shell user may change");
        // writable by the group or others
        assert!(wad_trusted(o(2000, 0o100664), o(2000, 0o40755), 2000).is_err());
        assert!(wad_trusted(o(0, 0o100644), o(0, 0o40777), 2000).is_err());
        // another user's
        assert!(wad_trusted(o(1000, 0o100644), o(0, 0o40755), 2000).is_err());
    }

    #[test]
    fn the_settings_and_saves_are_the_game_s_alone() {
        assert!(data_dir_private(o(2000, 0o40700), 2000).is_ok());
        assert!(data_dir_private(o(2000, 0o40750), 2000).is_err());
        assert!(data_dir_private(o(2000, 0o40701), 2000).is_err());
        assert!(data_dir_private(o(0, 0o40700), 2000).is_err());
    }

    #[test]
    fn the_panel_s_places() {
        // a run from adb: both in /data/local/tmp, as the panel run from adb has them
        assert_eq!(socket(None, None), "/data/local/tmp/har-display");
        assert_eq!(keys(None, None), "/data/local/tmp/har-game-keys");
        // the panel started by init (its XDG_RUNTIME_DIR)
        assert_eq!(socket(None, Some("/data/vendor/everyware/run")), "/data/vendor/everyware/run/har-display");
        assert_eq!(keys(None, Some("/data/vendor/everyware/run/")), "/data/vendor/everyware/run/har-game-keys");
        // given
        assert_eq!(socket(Some("/tmp/w"), Some("/run")), "/tmp/w");
        assert_eq!(socket(Some("wayland-1"), Some("/run")), "/run/wayland-1");
        assert_eq!(keys(Some("/tmp/k"), None), "/tmp/k");
        assert_eq!(socket(Some(""), None), "/data/local/tmp/har-display");
        assert_eq!(socket(None, Some("relative")), "/data/local/tmp/har-display");
    }
}
