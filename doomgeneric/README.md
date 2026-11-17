# doomgeneric

The engine of DOOM as [doomgeneric](https://github.com/ozkl/doomgeneric) has it: Chocolate
Doom's code cut down to a platform of six functions (`doomgeneric.h`). The files are the
engine's own `.c` and `.h` files of upstream's `doomgeneric/` directory, as an earlier import
took them; nothing was changed here. Not here: upstream's platform files (SDL, X11, Windows and the others; this app's platform is
`../src/platform.rs`), its build files and its sound and music backends (the app is built
without `FEATURE_SOUND`).

License: GPL-2.0 (`LICENSE`; the notices of the files say version 2 of the GPL or any later
version), as upstream. The rest of this repository is Apache 2.0; `doom_app` links the two, and
the binary is conveyed under the GPL.

The import has no record of the upstream commit it was taken at.
