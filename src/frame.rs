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

#[cfg(test)]
mod tests {
    use super::*;

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
