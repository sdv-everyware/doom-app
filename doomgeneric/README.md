# doomgeneric

The engine of DOOM as [doomgeneric](https://github.com/ozkl/doomgeneric) has it: Chocolate
Doom's code cut down to a platform of six functions (`doomgeneric.h`). The files are upstream's,
unchanged, at commit `63e2d26a8c6b67fa922aff7782e315e548bf28e3` (2026-10-11, "Add safeguards for
linuxvt"): the `.c` and `.h` files of its `doomgeneric/` directory and its `LICENSE`, byte for
byte. Upstream's engine files are the same at its commits `dcb7a8d` (2026-04-12) to `63e2d26`; its
`e257a09` (2026-10-11) changes one line of `doomtype.h` and adds `Makefile.win`.

Not here, of upstream's `doomgeneric/`: the platform files (`doomgeneric_allegro.c`,
`_emscripten.c`, `_linuxvt.c`, `_sdl.c`, `_soso.c`, `_sosox.c`, `_win.c`, `_xlib.c`; this app's
platform is `../src/platform.rs`), the sound and music files (`i_allegromusic.c`,
`i_allegrosound.c`, `i_sdlmusic.c`, `i_sdlsound.c`, `gusconf.c`, `mus2mid.c`; the app is built
without `FEATURE_SOUND`), `icon.c` and the build files (`Makefile*`, `*.vcxproj*`).

License: GPL-2.0 (`LICENSE`; the notices of the files say version 2 of the GPL or any later
version), as upstream. The rest of this repository is GPL-3.0-or-later (`../LICENSE`), and so is
`doom_app`, which links the two.
