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

// The engine's frame size as the engine sees it: built with the engine's flags
// (Android.bp, doom_app_engine_defaults) and its header, so the Rust side reads the size from
// here (platform.rs frame_size) and the frame has one size.

#include "doomgeneric.h"

const unsigned int doom_app_frame_width = DOOMGENERIC_RESX;
const unsigned int doom_app_frame_height = DOOMGENERIC_RESY;
