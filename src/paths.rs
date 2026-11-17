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

#[cfg(test)]
mod tests {
    use super::*;

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
