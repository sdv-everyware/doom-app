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

//! doom_app: DOOM in the smart home panel. The engine is doomgeneric (doomgeneric/, GPL-2.0),
//! the game Freedoom's (a WAD from scripts/fetch-freedoom.sh); this process draws no screen of
//! its own. Its picture goes to the zone #zone/game of the panel's game dialog, the dialog's
//! buttons come back as keys, and the game pauses while the dialog is closed.
//!
//! ```text
//!  panel (smarthome_app)                          doom_app
//!  #game/up .. #game/use held ──"down left\n"──► FIFO har-game-keys ──► DG_GetKey ──► engine
//!  #game/picture ◄── wl_shm 320x200, 35 fps ──── zone #zone/game ◄──── DG_DrawFrame ◄─┘
//!  dialog closed ──── har_zone.visible 0 ───────► paused: no tics, the clock stands still
//! ```

mod clock;
mod frame;
mod keys;
mod paths;
mod platform;
mod zone;

use clap::Parser as _;
use clap_derive::Parser;
use std::ffi::{c_char, CString};
use std::time::Instant;

#[derive(Parser, Debug)]
struct Args {
    /// The game: an IWAD (Freedoom's freedoom2.wad, scripts/fetch-freedoom.sh).
    #[arg(long, default_value = "/data/local/tmp/doom/freedoom2.wad")]
    wad: String,
    /// Where the game keeps its settings and saves (default.cfg, savegame/): made if missing.
    #[arg(long, default_value = "/data/local/tmp/doom")]
    data_path: String,
    /// The panel's Wayland socket: a path, or a name in XDG_RUNTIME_DIR (else /data/local/tmp).
    /// Default: HAR_WAYLAND_SOCKET, else har-display, as the panel serves it.
    #[arg(long)]
    socket: Option<String>,
    /// The FIFO the panel writes the keys to. Default: HAR_GAME_KEYS, else har-game-keys next
    /// to the socket, as the panel writes it.
    #[arg(long)]
    keys: Option<String>,
    /// The zone the picture goes to: the panel's tag for #game/picture.
    #[arg(long, default_value = "#zone/game")]
    zone: String,
    /// The engine's own arguments, after `--`. Default: straight into the first map on the
    /// middle skill, no menu.
    #[arg(last = true)]
    engine: Vec<String>,
}

/// The engine's arguments when none are given: the first map of a DOOM II IWAD (Freedoom: Phase
/// 2), Hurt me plenty.
const START: [&str; 4] = ["-skill", "3", "-warp", "1"];

fn main() {
    har_log::init_logger("doom-app");
    log::set_max_level(log::LevelFilter::Info);
    let args = Args::parse();
    if let Err(e) = run(args) {
        log::error!("doom-app: {e}");
        eprintln!("doom_app: {e}");
        std::process::exit(2);
    }
}

fn run(args: Args) -> Result<(), String> {
    if !std::path::Path::new(&args.wad).is_file() {
        return Err(format!("no WAD at {} (scripts/fetch-freedoom.sh, then push it)", args.wad));
    }
    std::fs::create_dir_all(&args.data_path).map_err(|e| format!("{}: {e}", args.data_path))?;
    // the engine writes its settings and saves into the working directory (m_config.c)
    std::env::set_current_dir(&args.data_path).map_err(|e| format!("{}: {e}", args.data_path))?;
    let env = |name: &str| std::env::var(name).ok();
    let runtime_dir = env("XDG_RUNTIME_DIR");
    let socket = paths::socket(args.socket.as_deref().or(env("HAR_WAYLAND_SOCKET").as_deref()), runtime_dir.as_deref());
    let keys = paths::keys(args.keys.as_deref().or(env("HAR_GAME_KEYS").as_deref()), runtime_dir.as_deref());

    let zone = zone::Zone::connect(&socket, &args.zone, platform::WIDTH, platform::HEIGHT)?;
    let fifo = keys::Fifo::open(&keys).map_err(|e| format!("the keys' FIFO {keys}: {e}"))?;
    log::info!("doom-app: keys from {keys}, the game {}", args.wad);
    platform::install(platform::Platform {
        zone,
        fifo,
        lines: keys::Lines::default(),
        clock: clock::Clock::new(Instant::now()),
        held: keys::Held::default(),
        queue: Default::default(),
    });

    // The engine keeps argv for its whole life (m_argv.c myargv): the strings are never freed.
    let mut words = vec!["doom_app".to_string(), "-iwad".to_string(), args.wad.clone()];
    words.extend(if args.engine.is_empty() { START.iter().map(|s| s.to_string()).collect() } else { args.engine.clone() });
    let argv: Vec<*mut c_char> = words
        .iter()
        .map(|w| CString::new(w.as_str()).map(CString::into_raw).map_err(|_| format!("a NUL in the argument {w:?}")))
        .collect::<Result<_, _>>()?;
    let argv: &'static mut [*mut c_char] = Vec::leak(argv);
    log::info!("doom-app: engine {}", words[1..].join(" "));
    // SAFETY: argc matches argv, whose strings live for the process; the platform is installed.
    unsafe { platform::doomgeneric_Create(argv.len() as i32, argv.as_mut_ptr()) };
    loop {
        platform::with(|p| p.wait_until_shown());
        // SAFETY: doomgeneric_Create ran; the engine runs on this thread only.
        unsafe { platform::doomgeneric_Tick() };
    }
}
