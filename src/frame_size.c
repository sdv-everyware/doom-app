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

// The engine's frame size as the engine sees it: built with the engine's flags
// (Android.bp, doom_app_engine_defaults) and its header, so the Rust side reads the size from
// here (platform.rs frame_size) and the frame has one size.

#include "doomgeneric.h"

const unsigned int doom_app_frame_width = DOOMGENERIC_RESX;
const unsigned int doom_app_frame_height = DOOMGENERIC_RESY;
