# doom-app

DOOM in the smart home panel: `doom_app` runs the DOOM engine
([doomgeneric](https://github.com/ozkl/doomgeneric), `doomgeneric/`) with Freedoom's game data
and shows its picture in the panel's game dialog. The panel (`smarthome-app`) has the dialog: a
game button in the status row opens it, the picture sits in the middle, a direction pad at its
left, Fire and Use at its right. A held button is a held key, so walking and firing work at once.
Closing the dialog pauses the game; opening it again goes on where it stopped.

```
 panel (smarthome_app)                            doom_app
 #game/up .. #game/use held ──"down left\n"────► FIFO har-game-keys ──► DG_GetKey ──► engine
 #game/picture ◄── wl_shm 320x200, up to 35 fps ─ zone #zone/game ◄──── DG_DrawFrame ◄──┘
 dialog closed ──── har_zone visible 0 ─────────► paused: no tics, the clock stands still
```

`doom_app` draws no screen of its own and uses no GPU: the engine draws a 320 x 200 frame on the
CPU, `src/zone.rs` copies it into a wl_shm buffer and presents it to the zone `#zone/game` over
harry's Wayland client shim (har-wayland-client, the `har_zone` protocol). The panel copies the
256 KB, uploads them as a texture and draws them into `#game/picture`, scaled to 4:3. A frame
waits for the panel's frame callback of the one before, so the game never queues frames behind
the panel. The panel tells the zone whether it is shown; hidden, the game runs no tics and its
clock stands still (`src/clock.rs`).

## The keys

The panel writes one line per key that goes down or up into the FIFO `har-game-keys`, which
`doom_app` makes: `down <key>` or `up <key>`, the key one of `up`, `down`, `left`, `right`, `fire`,
`use`. Every other line is dropped. They are DOOM's arrows (walk and turn), fire and use (doors,
switches, the next try after death). Both sides find the FIFO next to the panel's Wayland socket:
`$XDG_RUNTIME_DIR`, without one `/data/local/tmp` (`HAR_GAME_KEYS` overrides it).

## Build

```bash
# from the tree's top (bash: env.sh uses BASH_SOURCE)
bash -c 'source device/rb3/sdv_media_db845c/tools/env.sh >/dev/null && m doom_app doom_app_host_test'
out_rb3/host/linux-x86/nativetest64/doom_app_host_test/doom_app_host_test
```

## The game's data

`scripts/fetch-freedoom.sh` downloads Freedoom 0.13.0 from its GitHub release, checks the zip's
SHA-256 and writes `wad/freedoom2.wad` with its license `wad/COPYING.txt`. The WAD is never in
git. `doom_app --wad <path>` reads any DOOM II IWAD; without arguments for the engine it starts
straight into the first map (`-skill 3 -warp 1`), no menu.

## Run (RB3, from adb)

The panel runs with `--wayland-zones` (its APEX's init script passes it). Then:

```bash
adb shell mkdir -p /data/local/tmp/doom
adb push out_rb3/target/product/sdv_media_db845c/system/bin/doom_app /data/local/tmp/doom/
adb push wad/freedoom2.wad /data/local/tmp/doom/
adb shell 'cd /data/local/tmp/doom && nohup ./doom_app > doom.log 2>&1 &'
adb logcat -s doom-app
```

`--socket`, `--keys` and `--data-path` say where the panel's socket and the FIFO are and where
the game keeps its settings and saves; the defaults are the places of a panel run from adb.

## Licenses

`src/`, the build and the scripts: Apache 2.0 (`LICENSE`). `doomgeneric/`: GPL-2.0, upstream's
(`doomgeneric/LICENSE`, `doomgeneric/README.md`). `doom_app` links both, so the binary is
conveyed under the GPL; its source is this repository. Freedoom's data: BSD
(`wad/COPYING.txt` from the release), not in this repository.
