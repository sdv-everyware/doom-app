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

//! The engine's frame as the panel takes it: doomgeneric draws 0x00RRGGBB words (i_video.c
//! cmap_to_fb, rgba8888: B, G, R, 0 in memory); the panel uploads the bytes R, G, B, A as they
//! are (DRM ABGR8888, wayland_bridge zone_image.rs) and would swap and fill in the alpha of any
//! other order itself, on its own thread, in every frame. So the game does it here, once, while
//! it copies the frame into the buffer it sends.

/// DRM_FORMAT_ABGR8888 ('AB24'): R, G, B, A in memory.
pub const ABGR8888: u32 = u32::from_le_bytes(*b"AB24");
/// DRM_FORMAT_XRGB8888 ('XR24'): B, G, R, X in memory, the engine's own order.
pub const XRGB8888: u32 = u32::from_le_bytes(*b"XR24");

/// Copies the engine's pixels `src` (0x00RRGGBB) into `dst` as R, G, B, 255.
pub fn to_abgr(src: &[u32], dst: &mut [u8]) {
    for (p, out) in src.iter().zip(dst.chunks_exact_mut(4)) {
        out.copy_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8, 0xff]);
    }
}

/// Copies the engine's pixels `src` into `dst` as they are (B, G, R, X), for a panel that has no
/// ABGR8888 buffers.
pub fn to_xrgb(src: &[u32], dst: &mut [u8]) {
    for (p, out) in src.iter().zip(dst.chunks_exact_mut(4)) {
        out.copy_from_slice(&p.to_le_bytes());
    }
}

/// The engine's frame to the zone's size, on the CPU: the nearest source pixel for each one of
/// the zone (DOOM's square-ish pixels stretched to the zone's 4:3). The panel draws a buffer only
/// at the zone's size (zone.rs); this is the cost of that, a row of the zone built once per
/// engine row and copied for the zone's rows that repeat it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scaler {
    /// The engine's frame and the zone, width and height.
    pub from: (u32, u32),
    pub to: (u32, u32),
    /// For each column and row of the zone, the engine's column and row.
    xs: Vec<u32>,
    ys: Vec<u32>,
}

impl Scaler {
    /// The scaler from `from` to `to` (both at least 1 x 1).
    pub fn new(from: (u32, u32), to: (u32, u32)) -> Self {
        // the source pixel under the centre of each destination pixel
        let map = |n: u32, m: u32| (0..m).map(|i| ((2 * u64::from(i) + 1) * u64::from(n) / (2 * u64::from(m))) as u32).collect();
        Scaler { from, to, xs: map(from.0, to.0), ys: map(from.1, to.1) }
    }

    /// Writes `src` (from.0 x from.1 words 0x00RRGGBB) into `dst` (to.0 x to.1 pixels of 4
    /// bytes, rows of to.0 * 4): R, G, B, 255 with `abgr`, else the words as they are (B, G, R, X).
    pub fn scale(&self, src: &[u32], dst: &mut [u8], abgr: bool) {
        let (sw, dw) = (self.from.0 as usize, self.to.0 as usize);
        let row = dw * 4;
        for (dy, &sy) in self.ys.iter().enumerate() {
            if dy > 0 && self.ys[dy - 1] == sy {
                dst.copy_within((dy - 1) * row..dy * row, dy * row);
                continue;
            }
            let line = &src[sy as usize * sw..(sy as usize + 1) * sw];
            for (out, &sx) in dst[dy * row..(dy + 1) * row].chunks_exact_mut(4).zip(&self.xs) {
                let p = line[sx as usize];
                out.copy_from_slice(&if abgr { [(p >> 16) as u8, (p >> 8) as u8, p as u8, 0xff] } else { p.to_le_bytes() });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_pixel_takes_the_nearest_one() {
        // 2 x 2 to 4 x 4: each source pixel a 2 x 2 block
        let src = [0x0000_0001, 0x0000_0002, 0x0000_0003, 0x0000_0004];
        let s = Scaler::new((2, 2), (4, 4));
        let mut dst = vec![0u8; 4 * 4 * 4];
        s.scale(&src, &mut dst, true);
        let blue: Vec<u8> = dst.chunks_exact(4).map(|p| p[2]).collect();
        assert_eq!(blue, [1, 1, 2, 2, 1, 1, 2, 2, 3, 3, 4, 4, 3, 3, 4, 4]);
        assert!(dst.chunks_exact(4).all(|p| p[3] == 0xff));
        // the same size: a copy; the engine's order kept without abgr
        let s = Scaler::new((2, 2), (2, 2));
        let mut dst = vec![0u8; 16];
        s.scale(&src, &mut dst, false);
        assert_eq!(&dst[12..], &4u32.to_le_bytes());
    }

    #[test]
    fn doom_to_the_dialog_s_picture() {
        // 320 x 200 to 1040 x 780: the corners stay the corners, every source row is used
        let (from, to) = ((320, 200), (1040, 780));
        let s = Scaler::new(from, to);
        assert_eq!((s.xs.len(), s.ys.len()), (1040, 780));
        assert_eq!((s.xs[0], s.xs[1039], s.ys[0], s.ys[779]), (0, 319, 0, 199));
        assert!(s.xs.windows(2).all(|w| w[1] == w[0] || w[1] == w[0] + 1));
        assert_eq!(s.ys.iter().collect::<std::collections::BTreeSet<_>>().len(), 200);
        let src: Vec<u32> = (0..320 * 200).map(|i| i as u32).collect();
        let mut dst = vec![0u8; 1040 * 780 * 4];
        s.scale(&src, &mut dst, false);
        let at = |x: usize, y: usize| u32::from_le_bytes(dst[(y * 1040 + x) * 4..][..4].try_into().unwrap());
        assert_eq!(at(0, 0), 0);
        assert_eq!(at(1039, 779), 320 * 200 - 1);
        assert_eq!(at(520, 390), 100 * 320 + 160);
    }

    #[test]
    fn the_fourccs_are_drm_s() {
        assert_eq!(ABGR8888, 0x3432_4241);
        assert_eq!(XRGB8888, 0x3432_5258);
    }

    #[test]
    fn red_stays_red() {
        let src = [0x00ff_0000, 0x0000_ff00, 0x0000_00ff, 0x0012_3456];
        let mut dst = [0u8; 16];
        to_abgr(&src, &mut dst);
        assert_eq!(dst, [0xff, 0, 0, 0xff, 0, 0xff, 0, 0xff, 0, 0, 0xff, 0xff, 0x12, 0x34, 0x56, 0xff]);
        to_xrgb(&src, &mut dst);
        assert_eq!(&dst[12..], &[0x56, 0x34, 0x12, 0x00]);
    }

    #[test]
    fn a_whole_frame() {
        let src = vec![0x0080_4020u32; 320 * 200];
        let mut dst = vec![0u8; 320 * 200 * 4];
        to_abgr(&src, &mut dst);
        assert!(dst.chunks_exact(4).all(|p| p == [0x80, 0x40, 0x20, 0xff]));
    }
}
