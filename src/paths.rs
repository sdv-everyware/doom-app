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

/// A file's owner, group and mode (st_uid, st_gid, st_mode).
#[derive(Clone, Copy, Debug)]
pub struct Owner {
    pub uid: u32,
    pub gid: u32,
    pub mode: u32,
}

/// Android's system user and group (AID_SYSTEM): /data and /data/vendor are theirs.
const AID_SYSTEM: u32 = 1000;

/// Whether the engine may parse the WAD at the end of `chain`: the WAD's absolute path, its
/// directories from `/` down and the WAD last, each with its owner. Nobody but root, the system
/// user and this process (`euid`, `egid`) may change any of them, so none can be replaced
/// between the check and the engine's open: each belongs to root, system or this user, only
/// root's, system's or this process's group may write it, and nobody else may (OpenSSH's
/// StrictModes, with Android's system user). Run as root, a directory of the shell user fails.
pub fn wad_trusted(chain: &[(String, Owner)], euid: u32, egid: u32) -> Result<(), String> {
    let users = [0, AID_SYSTEM, euid];
    let groups = [0, AID_SYSTEM, egid];
    for (path, o) in chain {
        if !users.contains(&o.uid) {
            return Err(format!("{path} belongs to uid {}, not to root, system or this user ({euid})", o.uid));
        }
        if o.mode & 0o002 != 0 || (o.mode & 0o020 != 0 && !groups.contains(&o.gid)) {
            return Err(format!("{path} may be written by others (mode {:o}, group {})", o.mode & 0o7777, o.gid));
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

    const fn o(uid: u32, gid: u32, mode: u32) -> Owner {
        Owner { uid, gid, mode }
    }

    fn chain(parts: &[(&str, Owner)]) -> Vec<(String, Owner)> {
        parts.iter().map(|(p, o)| (p.to_string(), *o)).collect()
    }

    /// Android's own directories: / and /data/local root's, /data and /data/vendor system's,
    /// /data/local/tmp the shell user's (rwxrwx--x).
    const ROOT: Owner = o(0, 0, 0o40755);
    const DATA: Owner = o(1000, 1000, 0o40771);
    const LOCAL: Owner = o(0, 0, 0o40751);
    const TMP: Owner = o(2000, 2000, 0o40771);

    #[test]
    fn the_wad_s_whole_path_belongs_to_root_system_or_the_game() {
        // the image's: root's from / down
        let image = chain(&[("/", ROOT), ("/vendor", ROOT), ("/vendor/etc", ROOT), ("/vendor/etc/doom", ROOT), ("/vendor/etc/doom/freedoom2.wad", o(0, 0, 0o100644))]);
        assert!(wad_trusted(&image, 2000, 2000).is_ok());
        assert!(wad_trusted(&image, 0, 0).is_ok());
        // a run as root from a directory of root's under /data/vendor
        let vendor = chain(&[("/", ROOT), ("/data", DATA), ("/data/vendor", DATA), ("/data/vendor/doom", o(0, 0, 0o40700)), ("/data/vendor/doom/freedoom2.wad", o(0, 0, 0o100644))]);
        assert!(wad_trusted(&vendor, 0, 0).is_ok());
        // the shell user's run from /data/local/tmp
        let shell = chain(&[("/", ROOT), ("/data", DATA), ("/data/local", LOCAL), ("/data/local/tmp", TMP), ("/data/local/tmp/doom", o(2000, 2000, 0o40700)), ("/data/local/tmp/doom/freedoom2.wad", o(2000, 2000, 0o100644))]);
        assert!(wad_trusted(&shell, 2000, 2000).is_ok());
        // run as root, the shell user may rename /data/local/tmp/doom away: refused, also when the
        // directory and the WAD are root's
        assert!(wad_trusted(&shell, 0, 0).is_err());
        let rooted = chain(&[("/", ROOT), ("/data", DATA), ("/data/local", LOCAL), ("/data/local/tmp", TMP), ("/data/local/tmp/doom", o(0, 0, 0o40700)), ("/data/local/tmp/doom/freedoom2.wad", o(0, 0, 0o100644))]);
        assert!(wad_trusted(&rooted, 0, 0).is_err());
        // writable by another group or by others, owned by another user
        assert!(wad_trusted(&chain(&[("/", ROOT), ("/w", o(2000, 2000, 0o100664))]), 2000, 2000).is_ok(), "its own group");
        assert!(wad_trusted(&chain(&[("/", ROOT), ("/w", o(2000, 1003, 0o100664))]), 2000, 2000).is_err());
        assert!(wad_trusted(&chain(&[("/", ROOT), ("/d", o(0, 0, 0o40777)), ("/d/w", o(0, 0, 0o100644))]), 2000, 2000).is_err());
        assert!(wad_trusted(&chain(&[("/", ROOT), ("/w", o(10001, 10001, 0o100644))]), 2000, 2000).is_err());
    }

    #[test]
    fn the_settings_and_saves_are_the_game_s_alone() {
        assert!(data_dir_private(o(2000, 2000, 0o40700), 2000).is_ok());
        assert!(data_dir_private(o(2000, 2000, 0o40750), 2000).is_err());
        assert!(data_dir_private(o(2000, 2000, 0o40701), 2000).is_err());
        assert!(data_dir_private(o(0, 0, 0o40700), 2000).is_err());
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
