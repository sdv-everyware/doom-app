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
//! The engine's platform: the six functions doomgeneric asks of a port (doomgeneric.h), on the
//! zone, the keys' FIFO and the game's clock. The engine calls them on the main thread only,
//! from doomgeneric_Create and doomgeneric_Tick; the state they share is that thread's.
//!
//! ```text
//! doomgeneric_Tick ─► TryRunTics ─► I_StartTic ─► DG_GetKey ◄── FIFO (the panel's keys)
//!                  │             └► I_Sleep ──► DG_SleepMs ──► the zone's events (waits)
//!                  └► D_Display ─► I_FinishUpdate ─► DG_DrawFrame ─► the zone (wl_shm)
//! ```
//!
//! The panel may go away (init starts it again after a crash): the game then pauses as for a
//! hidden zone, lets every key go and tries to reach the panel again, slower and slower up to
//! every 5 s (clock.rs Backoff). The engine's state stays; the game goes on when the new panel
//! shows the zone.

// the engine's names (doomgeneric.h)
#![allow(non_snake_case)]

use crate::clock::{Backoff, Clock};
use crate::keys::{Fifo, Held, KeyQueue, Lines};
use crate::zone::Zone;
use std::cell::RefCell;
use std::ffi::{c_char, c_int, c_uchar, c_uint, CStr};
use std::time::{Duration, Instant};

extern "C" {
    /// doomgeneric.c: the frame the engine drew, frame_size() words 0x00RRGGBB.
    #[allow(non_upper_case_globals)]
    static DG_ScreenBuffer: *mut u32;
    /// src/frame_size.c: DOOMGENERIC_RESX and DOOMGENERIC_RESY as the engine is built with them.
    static doom_app_frame_width: c_uint;
    static doom_app_frame_height: c_uint;
    pub fn doomgeneric_Create(argc: c_int, argv: *mut *mut c_char);
    pub fn doomgeneric_Tick();
}

/// The engine's frame, width and height: the size it was built with (Android.bp's cflags, read
/// from the engine's side, so the frame has one size and DG_DrawFrame never reads past it).
pub fn frame_size() -> (u32, u32) {
    // SAFETY: two constants of the linked C library, never written.
    unsafe { (doom_app_frame_width, doom_app_frame_height) }
}

/// Reads of the FIFO the engine may make between two frames: a writer that keeps it full costs
/// a frame at most this many bounded reads (keys.rs MAX_READ).
const READS_PER_FRAME: u8 = 4;

/// Where the panel is, to reach it again.
pub struct Panel {
    pub socket: String,
    pub tag: String,
}

pub struct Platform {
    pub panel: Panel,
    /// The zone while the panel is reached; None while it is gone.
    pub zone: Option<Zone>,
    pub fifo: Fifo,
    pub lines: Lines,
    pub clock: Clock,
    /// What the engine was told is down.
    pub held: Held,
    /// Keys for the engine's next DG_GetKey calls.
    pub queue: KeyQueue,
    /// FIFO reads left until the next frame.
    reads_left: u8,
}

thread_local! {
    static PLATFORM: RefCell<Option<Platform>> = const { RefCell::new(None) };
}

impl Platform {
    /// The platform before the first connection to the panel (`wait_until_shown` makes it).
    pub fn new(panel: Panel, fifo: Fifo, now: Instant) -> Self {
        Platform {
            panel,
            zone: None,
            fifo,
            lines: Lines::default(),
            clock: Clock::new(now),
            held: Held::default(),
            queue: KeyQueue::default(),
            reads_left: READS_PER_FRAME,
        }
    }

    /// Pauses until the panel shows the zone: reaches the panel first when it is gone. While
    /// paused the clock stands still, the held keys go up for the engine and the keys the panel
    /// sends wait in the FIFO.
    pub fn wait_until_shown(&mut self) {
        self.dispatch(0);
        if self.shown() {
            return;
        }
        self.clock.pause(Instant::now());
        for key in self.held.release() {
            self.queue.push(key, false);
        }
        let (presented, replaced) = self.zone.as_ref().map_or((0, 0), |z| (z.presented, z.replaced));
        log::info!(
            "doom-app: paused; so far {presented} frames shown, {replaced} replaced by newer ones, {} lines of keys and {} keys dropped",
            self.lines.dropped,
            self.queue.dropped
        );
        let mut backoff = Backoff::default();
        while !self.shown() {
            if self.zone.is_none() {
                self.connect(&mut backoff);
            } else {
                self.dispatch(1000);
            }
        }
        self.clock.resume(Instant::now());
        self.reads_left = READS_PER_FRAME;
        log::info!("doom-app: on (the zone is shown)");
    }

    /// One try to reach the panel; after a failure the next wait of `backoff` first.
    fn connect(&mut self, backoff: &mut Backoff) {
        let (width, height) = frame_size();
        match Zone::connect(&self.panel.socket, &self.panel.tag, width, height) {
            Ok(zone) => self.zone = Some(zone),
            Err(e) => {
                let wait = backoff.wait();
                log::warn!("doom-app: {e}; trying again in {} ms", wait.as_millis());
                std::thread::sleep(wait);
            }
        }
    }

    /// Whether the panel shows the zone now.
    fn shown(&self) -> bool {
        self.zone.as_ref().is_some_and(Zone::visible)
    }

    /// The panel is gone: the zone with it, until wait_until_shown reaches it again. What the
    /// gone panel wrote is dropped (it may have died with a key down); what a new panel writes
    /// from now on is kept.
    fn lose(&mut self, e: String) {
        log::warn!("doom-app: {e}; the game pauses until the panel is back");
        self.zone = None;
        self.fifo.discard();
        self.lines = Lines::default();
    }

    /// The panel's events, waiting up to `ms` (without a panel: a sleep).
    fn dispatch(&mut self, ms: u32) {
        let r = match self.zone.as_mut() {
            Some(zone) => zone.dispatch(ms.min(1000) as i32),
            None => {
                std::thread::sleep(Duration::from_millis(ms.min(1000).into()));
                Ok(())
            }
        };
        if let Err(e) = r {
            self.lose(e);
        }
    }

    /// The engine's frame to the panel.
    fn draw(&mut self, pixels: &[u32]) {
        self.reads_left = READS_PER_FRAME;
        if let Some(Err(e)) = self.zone.as_mut().map(|z| z.present(pixels)) {
            self.lose(e);
        }
    }

    /// The next key for the engine, reading the FIFO when none waits (within the frame's reads).
    fn next_key(&mut self) -> Option<(u8, bool)> {
        if self.queue.is_empty() && self.reads_left > 0 {
            self.reads_left -= 1;
            let bytes = self.fifo.read();
            for (key, down) in self.lines.feed(&bytes) {
                self.held.set(key, down);
                self.queue.push(key, down);
            }
        }
        self.queue.pop().map(|(key, down)| (key.code(), down))
    }
}

/// Hands the platform to the engine's functions, before doomgeneric_Create.
pub fn install(platform: Platform) {
    PLATFORM.with(|p| *p.borrow_mut() = Some(platform));
}

/// Runs `f` on the platform; the process ends when it is not installed (the engine has no way to
/// hear an error).
pub fn with<R>(f: impl FnOnce(&mut Platform) -> R) -> R {
    PLATFORM.with(|p| match p.borrow_mut().as_mut() {
        Some(platform) => f(platform),
        None => {
            log::error!("doom-app: the platform is not installed; the game ends");
            std::process::exit(1)
        }
    })
}

/// The engine starts: the FIFO is open, the panel is reached when the game waits for it
/// (main.rs).
#[no_mangle]
pub extern "C" fn DG_Init() {
    let (width, height) = frame_size();
    log::info!("doom-app: the engine starts ({width}x{height})");
}

/// The engine drew a frame into DG_ScreenBuffer: to the zone, as the newest frame (zone.rs).
#[no_mangle]
pub extern "C" fn DG_DrawFrame() {
    let (width, height) = frame_size();
    // SAFETY: doomgeneric_Create allocated the buffer (DOOMGENERIC_RESX * DOOMGENERIC_RESY
    // words, frame_size()) before the engine draws a frame, and frees it never; the engine does
    // not draw while this runs.
    let pixels = unsafe {
        let buffer = DG_ScreenBuffer;
        if buffer.is_null() {
            return;
        }
        std::slice::from_raw_parts(buffer, (width * height) as usize)
    };
    with(|p| p.draw(pixels));
}

/// The engine waits for its next tic: the panel's events meanwhile (a frame callback sends the
/// waiting frame, a hide pauses the game at the next tic).
#[no_mangle]
pub extern "C" fn DG_SleepMs(ms: u32) {
    with(|p| p.dispatch(ms));
}

/// The game's clock, which stands still while the game pauses.
#[no_mangle]
pub extern "C" fn DG_GetTicksMs() -> u32 {
    with(|p| p.clock.ms(Instant::now()))
}

/// The next key for the engine: 1 and the key, or 0 when there is none.
///
/// # Safety
///
/// The engine passes two valid pointers.
#[no_mangle]
pub unsafe extern "C" fn DG_GetKey(pressed: *mut c_int, key: *mut c_uchar) -> c_int {
    match with(Platform::next_key) {
        Some((code, down)) if !pressed.is_null() && !key.is_null() => {
            // SAFETY: the caller's contract.
            unsafe {
                *pressed = c_int::from(down);
                *key = code;
            }
            1
        }
        _ => 0,
    }
}

/// The engine names the game ("Freedoom: Phase 2"): logged.
///
/// # Safety
///
/// `title` is null or a C string.
#[no_mangle]
pub unsafe extern "C" fn DG_SetWindowTitle(title: *const c_char) {
    if !title.is_null() {
        // SAFETY: the caller's contract.
        let title = unsafe { CStr::from_ptr(title) };
        log::info!("doom-app: {}", title.to_string_lossy());
    }
}
