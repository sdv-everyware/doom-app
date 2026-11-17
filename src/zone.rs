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

//! The game's picture in a zone of the panel: a Wayland client of the panel (harry's
//! har-wayland-client shim) that binds the zone's tag (har_zone) and presents each frame in a
//! wl_shm buffer. The panel copies the pixels at the commit, gives the buffer back at once and
//! draws them into its node. No GPU here: the engine draws on the CPU.
//!
//! The buffers have the size the panel configures, the node's (har_zone.configure: "the client
//! should allocate buffers of that size"), and the engine's 320 x 200 frame is scaled to it here
//! (frame.rs Scaler; 1040 x 780 in the game's dialog, 3.2 MB a frame). The protocol says a
//! buffer of another size is scaled into the node, but harry's renderer draws an external image
//! 1:1 from its top left (impeller.rs takes the node's size as the source rectangle): a 320 x 200
//! buffer filled a corner of the zone. With a renderer that scales, `--native-size` sends the
//! engine's frames as they are (256 KB) and the panel's GPU scales them.
//!
//! The engine runs on at 35 tics a second whatever the panel does. A frame goes to the panel when
//! the panel showed the one before (its frame callback); until then it waits in a buffer of its
//! own, and a newer frame takes its place. So the panel always gets the newest frame as soon as
//! it can take one, and a panel slower than 35 frames a second shows fewer frames of a game that
//! does not slow down. The panel says whether the zone is shown (har_zone.visible): hidden, the
//! game pauses (platform.rs).

use crate::frame;
use har_wayland_client_bindgen as hwc;
use std::ffi::CString;

/// Two buffers: one the panel may still hold for a moment after a commit, one with the newest
/// frame waiting for the panel.
const BUFFERS: usize = 2;
/// The largest side of a buffer taken from a configure (the bridge's own limit for wl_shm).
const MAX_SIDE: u32 = 8192;

pub struct Zone {
    conn: *mut hwc::hwc_connection,
    zone: *mut hwc::hwc_zone,
    buffers: Vec<*mut hwc::hwc_buffer>,
    next: usize,
    /// The engine's frame, and the buffers' size (the zone's, or the engine's with `native`).
    engine: (u32, u32),
    width: u32,
    height: u32,
    /// Keep the engine's size whatever the panel configures (`--native-size`).
    native: bool,
    /// The engine's frame to the buffers' size, while they differ.
    scaler: Option<frame::Scaler>,
    /// ABGR8888 when the panel takes it (no conversion there), else XRGB8888 (every wl_shm).
    fourcc: u32,
    /// The buffer with the newest frame, not yet sent: the panel has not shown the one before.
    pending: Option<usize>,
    /// Frames sent to the panel, and frames a newer one replaced before they could go.
    pub presented: u64,
    pub replaced: u64,
}

impl Zone {
    /// Connects to the panel's socket, binds `tag` and makes the buffers: of the engine's frame
    /// (`width` x `height`) until the panel configures the zone's size, then of that, unless
    /// `native`.
    pub fn connect(socket: &str, tag: &str, width: u32, height: u32, native: bool) -> Result<Zone, String> {
        let socket_c = CString::new(socket).map_err(|_| "a NUL in the socket's path".to_string())?;
        let tag_c = CString::new(tag).map_err(|_| "a NUL in the zone's tag".to_string())?;
        // SAFETY: a valid C string that lives through the call.
        let conn = unsafe { hwc::hwc_connect(socket_c.as_ptr()) };
        if conn.is_null() {
            return Err(format!("no panel on {socket} (is it running with --wayland-zones?)"));
        }
        let mut z = Zone {
            conn,
            zone: std::ptr::null_mut(),
            buffers: Vec::new(),
            next: 0,
            engine: (width, height),
            width,
            height,
            native,
            scaler: None,
            fourcc: frame::XRGB8888,
            pending: None,
            presented: 0,
            replaced: 0,
        };
        // SAFETY: `conn` is connected; Drop takes down what was made if this fails.
        unsafe {
            if hwc::hwc_capabilities(conn) & hwc::HWC_CAP_SHM == 0 {
                return Err("the panel takes no wl_shm buffers".to_string());
            }
            if hwc::hwc_shm_supports(conn, frame::ABGR8888) == 1 {
                z.fourcc = frame::ABGR8888;
            }
            z.zone = hwc::hwc_zone_create(conn, tag_c.as_ptr(), width, height);
            if z.zone.is_null() {
                return Err(format!("the panel refused the zone {tag}"));
            }
        }
        z.make_buffers()?;
        // the zone's size, if the panel configured it already (a zone that is shown)
        z.follow_configure()?;
        log::info!(
            "doom-app: zone {tag} on {socket}: {width}x{height}, {} buffers, {}",
            BUFFERS,
            if z.fourcc == frame::ABGR8888 { "ABGR8888" } else { "XRGB8888" }
        );
        Ok(z)
    }

    /// Whether the panel shows the zone now (it said so; before it said anything: no).
    pub fn visible(&self) -> bool {
        // SAFETY: the zone lives as long as self.
        unsafe { hwc::hwc_zone_visible(self.zone) == 1 }
    }

    /// The buffers of the current size, BUFFERS of them.
    fn make_buffers(&mut self) -> Result<(), String> {
        for _ in 0..BUFFERS {
            // SAFETY: connected until Drop; the buffer is destroyed in drop_buffers or Drop.
            let b = unsafe { hwc::hwc_shm_buffer_create(self.conn, self.width, self.height, self.fourcc, self.width * 4) };
            // SAFETY: `b` is null or a buffer just made.
            if b.is_null() || unsafe { hwc::hwc_buffer_map(b) }.is_null() {
                return Err(format!("no wl_shm buffer of {}x{}", self.width, self.height));
            }
            self.buffers.push(b);
        }
        Ok(())
    }

    /// Destroys the buffers. The panel copied what it was sent at the commit: a buffer it still
    /// counts as busy has nothing it reads.
    fn drop_buffers(&mut self) {
        for b in self.buffers.drain(..) {
            // SAFETY: made by make_buffers, destroyed once.
            unsafe { hwc::hwc_buffer_destroy(b) };
        }
        self.pending = None;
        self.next = 0;
    }

    /// Takes the size the panel configured, if it sent one: the buffers get it, and the engine's
    /// frame is scaled to it. A size beyond any display is not taken.
    fn follow_configure(&mut self) -> Result<(), String> {
        let (mut w, mut h) = (0u32, 0u32);
        // SAFETY: the zone lives as long as self; `w` and `h` are locals.
        if unsafe { hwc::hwc_zone_take_configure(self.zone, &mut w, &mut h) } != 1 || self.native {
            return Ok(());
        }
        if !(1..=MAX_SIDE).contains(&w) || !(1..=MAX_SIDE).contains(&h) {
            log::warn!("doom-app: the panel configured {w}x{h}: not taken");
            return Ok(());
        }
        if (w, h) == (self.width, self.height) {
            return Ok(());
        }
        self.drop_buffers();
        (self.width, self.height) = (w, h);
        self.make_buffers()?;
        self.scaler = ((w, h) != self.engine).then(|| frame::Scaler::new(self.engine, (w, h)));
        log::info!("doom-app: the zone is {w}x{h}: the engine's {}x{} scaled to it", self.engine.0, self.engine.1);
        Ok(())
    }

    /// Handles what the panel sent, waiting up to `timeout_ms` for something (0: no wait), and
    /// sends the waiting frame when the panel can take it. An error when the panel is gone.
    pub fn dispatch(&mut self, timeout_ms: i32) -> Result<(), String> {
        // SAFETY: connected until Drop.
        let r = unsafe { hwc::hwc_dispatch_timeout(self.conn, timeout_ms.max(0)) };
        if r < 0 {
            return Err(format!("the connection to the panel broke ({})", std::io::Error::from_raw_os_error(-r)));
        }
        self.follow_configure()?;
        self.flush()
    }

    /// Takes the engine's frame (`pixels`: the engine's width x height words 0x00RRGGBB) as the
    /// newest: into the waiting buffer (the frame there is replaced) or a free one, scaled to the
    /// buffers' size, and sends it when the panel can take it. With no buffer free (the panel
    /// still holds both) the frame is left out.
    pub fn present(&mut self, pixels: &[u32]) -> Result<(), String> {
        let free = |z: &Zone, k: usize| {
            // SAFETY: the buffers live as long as the zone.
            z.pending != Some(k) && unsafe { hwc::hwc_buffer_is_busy(z.buffers[k]) } == 0
        };
        let target = match self.pending {
            Some(i) => {
                self.replaced += 1;
                Some(i)
            }
            None => (0..BUFFERS).map(|k| (self.next + k) % BUFFERS).find(|&k| free(self, k)),
        };
        let Some(i) = target else {
            self.replaced += 1;
            return self.dispatch(0);
        };
        let len = (self.width * self.height * 4) as usize;
        // SAFETY: a wl_shm buffer's mapping is stride * height bytes (stride = width * 4) until
        // hwc_buffer_destroy; this buffer is not with the panel (free, or never sent).
        let dst = unsafe { std::slice::from_raw_parts_mut(hwc::hwc_buffer_map(self.buffers[i]) as *mut u8, len) };
        let src = &pixels[..(self.engine.0 * self.engine.1) as usize];
        let abgr = self.fourcc == frame::ABGR8888;
        match &self.scaler {
            Some(scaler) => scaler.scale(src, dst, abgr),
            None if abgr => frame::to_abgr(src, dst),
            None => frame::to_xrgb(src, dst),
        }
        self.pending = Some(i);
        self.dispatch(0)
    }

    /// Sends the waiting frame if the panel showed the one before.
    fn flush(&mut self) -> Result<(), String> {
        let Some(i) = self.pending else { return Ok(()) };
        // SAFETY: the zone and the buffers live as long as self.
        let r = unsafe {
            if hwc::hwc_zone_frame_pending(self.zone) == 1 {
                return Ok(());
            }
            hwc::hwc_zone_present(self.zone, self.buffers[i], -1)
        };
        if r == -libc::EBUSY {
            // the panel still holds this buffer: it was sent before; try again with the next event
            return Ok(());
        }
        if r < 0 {
            return Err(format!("present failed ({})", std::io::Error::from_raw_os_error(-r)));
        }
        self.pending = None;
        self.next = (i + 1) % BUFFERS;
        self.presented += 1;
        Ok(())
    }
}

impl Drop for Zone {
    fn drop(&mut self) {
        // SAFETY: each pointer is made once and destroyed here once; the buffers and the zone
        // before the connection that carries them.
        unsafe {
            for b in self.buffers.drain(..) {
                hwc::hwc_buffer_destroy(b);
            }
            if !self.zone.is_null() {
                hwc::hwc_zone_destroy(self.zone);
            }
            hwc::hwc_disconnect(self.conn);
        }
    }
}
