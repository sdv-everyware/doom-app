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

//! The engine's platform: the six functions doomgeneric asks of a port (doomgeneric.h), on the
//! zone, the keys' FIFO and the game's clock. The engine calls them on the main thread only,
//! from doomgeneric_Create and doomgeneric_Tick; the state they share is that thread's.
//!
//! ```text
//! doomgeneric_Tick ─► TryRunTics ─► I_StartTic ─► DG_GetKey ◄── FIFO (the panel's keys)
//!                  │             └► I_Sleep ──► DG_SleepMs ──► the zone's events (waits)
//!                  └► D_Display ─► I_FinishUpdate ─► DG_DrawFrame ─► the zone (wl_shm)
//! ```

// the engine's names (doomgeneric.h)
#![allow(non_snake_case)]

use crate::clock::Clock;
use crate::keys::{Fifo, Held, Lines};
use crate::zone::Zone;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::{c_char, c_int, c_uchar, CStr};
use std::time::Instant;

/// The engine's frame: DOOMGENERIC_RESX x DOOMGENERIC_RESY (Android.bp's cflags).
pub const WIDTH: u32 = 320;
pub const HEIGHT: u32 = 200;

extern "C" {
    /// doomgeneric.c: the frame the engine drew, WIDTH x HEIGHT words 0x00RRGGBB.
    #[allow(non_upper_case_globals)]
    static DG_ScreenBuffer: *mut u32;
    pub fn doomgeneric_Create(argc: c_int, argv: *mut *mut c_char);
    pub fn doomgeneric_Tick();
}

pub struct Platform {
    pub zone: Zone,
    pub fifo: Fifo,
    pub lines: Lines,
    pub clock: Clock,
    /// What the engine was told is down.
    pub held: Held,
    /// Keys for the engine's next DG_GetKey calls: (DOOM's code, down).
    pub queue: VecDeque<(u8, bool)>,
}

thread_local! {
    static PLATFORM: RefCell<Option<Platform>> = const { RefCell::new(None) };
}

/// Hands the platform to the engine's functions, before doomgeneric_Create.
pub fn install(platform: Platform) {
    PLATFORM.with(|p| *p.borrow_mut() = Some(platform));
}

/// Runs `f` on the platform; the process ends when it is not there or the panel is gone (the
/// engine has no way to hear an error).
pub fn with<R>(f: impl FnOnce(&mut Platform) -> Result<R, String>) -> R {
    let r = PLATFORM.with(|p| match p.borrow_mut().as_mut() {
        Some(platform) => f(platform),
        None => Err("the platform is not installed".to_string()),
    });
    r.unwrap_or_else(|e| {
        log::error!("doom-app: {e}; the game ends");
        std::process::exit(1)
    })
}

impl Platform {
    /// Pauses while the panel does not show the zone: the clock stands still, the held keys go
    /// up for the engine, the keys the panel sends wait in the FIFO. Returns when it is shown.
    pub fn wait_until_shown(&mut self) -> Result<(), String> {
        self.zone.dispatch(0)?;
        if self.zone.visible() {
            return Ok(());
        }
        self.clock.pause(Instant::now());
        for key in self.held.release() {
            self.queue.push_back((key.code(), false));
        }
        log::info!(
            "doom-app: paused (the zone is hidden); so far {} frames shown, {} left out, {} lines of keys dropped",
            self.zone.presented,
            self.zone.skipped,
            self.lines.dropped
        );
        while !self.zone.visible() {
            self.zone.dispatch(1000)?;
        }
        self.clock.resume(Instant::now());
        log::info!("doom-app: on (the zone is shown)");
        Ok(())
    }

    /// The keys the panel wrote since the last call, into the queue.
    fn read_keys(&mut self) {
        let bytes = self.fifo.read();
        for (key, down) in self.lines.feed(&bytes) {
            self.held.set(key, down);
            self.queue.push_back((key.code(), down));
        }
    }
}

/// The engine starts: the zone and the keys are there already (main.rs).
#[no_mangle]
pub extern "C" fn DG_Init() {
    log::info!("doom-app: the engine starts ({WIDTH}x{HEIGHT})");
}

/// The engine drew a frame into DG_ScreenBuffer: to the zone, unless the panel is behind.
#[no_mangle]
pub extern "C" fn DG_DrawFrame() {
    with(|p| {
        // SAFETY: doomgeneric_Create allocated the buffer (WIDTH * HEIGHT words) before the
        // engine draws a frame, and frees it never; the engine does not draw while this runs.
        let pixels = unsafe {
            let buffer = DG_ScreenBuffer;
            if buffer.is_null() {
                return Ok(());
            }
            std::slice::from_raw_parts(buffer, (WIDTH * HEIGHT) as usize)
        };
        p.zone.present(pixels).map(|_| ())
    })
}

/// The engine waits for its next tic.
#[no_mangle]
pub extern "C" fn DG_SleepMs(ms: u32) {
    // the zone's events while the engine waits for its next tic: a frame callback, a hide
    with(|p| p.zone.dispatch(ms.min(1000) as i32))
}

/// The game's clock, which stands still while the game pauses.
#[no_mangle]
pub extern "C" fn DG_GetTicksMs() -> u32 {
    with(|p| Ok(p.clock.ms(Instant::now())))
}

/// The next key for the engine: 1 and the key, or 0 when there is none.
///
/// # Safety
///
/// The engine passes two valid pointers.
#[no_mangle]
pub unsafe extern "C" fn DG_GetKey(pressed: *mut c_int, key: *mut c_uchar) -> c_int {
    let next = with(|p| {
        if p.queue.is_empty() {
            p.read_keys();
        }
        Ok(p.queue.pop_front())
    });
    match next {
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
