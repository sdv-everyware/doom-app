# doom-app: the guide for coding agents

DOOM in the smart home panel: a process without a screen of its own that runs the DOOM engine and
shows its picture in a zone of the panel. Its own git repository in the tree at
`packages/services/display_safety/utils/doom-app`. The project's guide (the repositories, every
skill, the rules for every change) is `vendor/sdv-everyware/everyware/AGENTS.md`: read it first.

## What is where

| Path | What |
|---|---|
| `doomgeneric/` | the engine, upstream's files (GPL-2.0): never changed here |
| `src/platform.rs` | the engine's platform: the six `DG_*` functions of `doomgeneric.h` |
| `src/zone.rs` | the picture to the panel's zone (wl_shm over har-wayland-client) |
| `src/keys.rs` | the keys from the panel's FIFO, as DOOM's key codes |
| `src/clock.rs`, `src/frame.rs`, `src/paths.rs` | the pausable clock and the reconnect's backoff, the frame's bytes, where the socket and the FIFO are and whom the WAD and the data may belong to |
| `src/frame_size.c` | the engine's frame size for the Rust side: its one place is `doom_app_engine_defaults` in `Android.bp` |
| `scripts/fetch-freedoom.sh` | the game's data (Freedoom), never in git; writes `wad/Android.bp` for the image |
| `product/doom.mk` | the flag EVERYWARE_GAME: doom_app, the WAD and `ro.vendor.everyware.game` in the image |

The repository is GPL-3.0-or-later (`LICENSE`); a new file of ours gets its notice (the one in
`src/main.rs`, without a copyright line). `doomgeneric/` keeps upstream's GPL-2.0-or-later.

The other side is smarthome-app: `src/game.rs` (the keys, the FIFO), the dialog in
`design/game-patch.js`. The two sides share the FIFO's lines and the zone's tag: change both.

## Rules

- `doomgeneric/` stays upstream's: a fix of the engine goes upstream, not here.
- Logic that needs neither the engine nor the panel is a module with host tests
  (`src/host_tests.rs`, `m doom_app_host_test`).
- On the board or VM: say what each command does and whether it changes state.

A change lands through its own branch, a review by another agent and the user's word
(`vendor/sdv-everyware/everyware/AGENTS.md`, "How a change lands"; `scripts/feature.sh` there).
The hooks in `scripts/git-hooks/` refuse a commit on `main`, by another author, with something
private in it or with an AI attribution (`vendor/sdv-everyware/everyware/scripts/setup-agents.sh`
installs them).
