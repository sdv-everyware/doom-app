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
//! wl_shm buffer of the engine's size. The panel copies the pixels at the commit, gives the
//! buffer back at once and draws them into its node, scaled to the node (the zone's frame on the
//! dashboard, 4:3). No GPU here: the engine draws on the CPU, and a frame is 256 KB.
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

pub struct Zone {
    conn: *mut hwc::hwc_connection,
    zone: *mut hwc::hwc_zone,
    buffers: Vec<*mut hwc::hwc_buffer>,
    next: usize,
    width: u32,
    height: u32,
    /// ABGR8888 when the panel takes it (no conversion there), else XRGB8888 (every wl_shm).
    fourcc: u32,
    /// The buffer with the newest frame, not yet sent: the panel has not shown the one before.
    pending: Option<usize>,
    /// Frames sent to the panel, and frames a newer one replaced before they could go.
    pub presented: u64,
    pub replaced: u64,
}

impl Zone {
    /// Connects to the panel's socket, binds `tag` and makes the buffers of `width` x `height`.
    pub fn connect(socket: &str, tag: &str, width: u32, height: u32) -> Result<Zone, String> {
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
            width,
            height,
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
            for _ in 0..BUFFERS {
                let b = hwc::hwc_shm_buffer_create(conn, width, height, z.fourcc, width * 4);
                if b.is_null() || hwc::hwc_buffer_map(b).is_null() {
                    return Err("no wl_shm buffer".to_string());
                }
                z.buffers.push(b);
            }
        }
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

    /// Handles what the panel sent, waiting up to `timeout_ms` for something (0: no wait), and
    /// sends the waiting frame when the panel can take it. An error when the panel is gone.
    pub fn dispatch(&mut self, timeout_ms: i32) -> Result<(), String> {
        // SAFETY: connected until Drop.
        let r = unsafe { hwc::hwc_dispatch_timeout(self.conn, timeout_ms.max(0)) };
        if r < 0 {
            return Err(format!("the connection to the panel broke ({})", std::io::Error::from_raw_os_error(-r)));
        }
        self.flush()
    }

    /// Takes the engine's frame (`pixels`: width x height words 0x00RRGGBB) as the newest: into
    /// the waiting buffer (the frame there is replaced) or a free one, and sends it when the
    /// panel can take it. With no buffer free (the panel still holds both) the frame is left out.
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
        let src = &pixels[..(self.width * self.height) as usize];
        if self.fourcc == frame::ABGR8888 {
            frame::to_abgr(src, dst);
        } else {
            frame::to_xrgb(src, dst);
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
