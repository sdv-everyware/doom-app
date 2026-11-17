# doom-app

DOOM in the smart home panel: `doom_app` runs the DOOM engine
([doomgeneric](https://github.com/ozkl/doomgeneric), `doomgeneric/`) with Freedoom's game data
and shows its picture in the panel's game dialog. The panel (`smarthome-app`) has the dialog: a
game button in the status row opens it, the picture sits in the middle, a direction pad at its
left, Fire and Use at its right. A held button is a held key, so walking and firing work at once
(see "Not verified"). Closing the dialog pauses the game; opening it again goes on where it
stopped.

```
 panel (smarthome_app)                            doom_app
 #game/up .. #game/use held ──"down left\n"────► FIFO har-game-keys ──► DG_GetKey ──► engine
 #game/picture ◄── wl_shm 320x200, newest frame ─ zone #zone/game ◄──── DG_DrawFrame ◄──┘
 dialog closed ──── har_zone visible 0 ─────────► paused: no tics, the clock stands still
```

`doom_app` draws no screen of its own and uses no GPU: the engine draws a 320 x 200 frame on the
CPU, `src/zone.rs` copies it into a wl_shm buffer and presents it to the zone `#zone/game` over
harry's Wayland client shim (har-wayland-client, the `har_zone` protocol). The panel copies the
256 KB, uploads them as a texture and draws them into `#game/picture`, scaled to 4:3.

The engine runs at 35 tics a second whatever the panel does. A frame goes to the panel once the
panel showed the one before (its frame callback); until then it waits, and a newer frame takes its
place. A panel slower than 35 frames a second shows fewer frames of a game that does not slow
down. The panel tells the zone whether it is shown; hidden, the game runs no tics and its clock
stands still (`src/clock.rs`).

When the panel goes away (init starts it again after a crash), the game pauses the same way,
lets every key go and tries to reach the panel again, after 0.25 s and then slower, up to every
5 s. The game's state stays; keys the old panel wrote are dropped.

## The keys

The panel writes one line per key that goes down or up into the FIFO `har-game-keys`, which
`doom_app` makes: `down <key>` or `up <key>`, the key one of `up`, `down`, `left`, `right`, `fire`,
`use`. Every other line is dropped, and so are keys past 64 waiting (the oldest downs first). They
are DOOM's arrows (walk and turn), fire and use (doors, switches, the next try after death). Both
sides find the FIFO next to the panel's Wayland socket: `$XDG_RUNTIME_DIR`, without one
`/data/local/tmp` (`HAR_GAME_KEYS` overrides it). `doom_app` takes a FIFO there only if it is its
user's and nobody else may open it; it makes one, rw for its user and group.

## Build

```bash
# from the tree's top (bash: env.sh uses BASH_SOURCE)
bash -c 'source device/rb3/sdv_media_db845c/tools/env.sh >/dev/null && m doom_app doom_app_host_test'
out_rb3/host/linux-x86/nativetest64/doom_app_host_test/doom_app_host_test
```

## The game's data

`scripts/fetch-freedoom.sh` downloads Freedoom 0.13.0 from its GitHub release, checks the zip's
SHA-256 against the one pinned in the script and writes `wad/freedoom2.wad` with its license
`wad/COPYING.txt` (the hash is not pinned yet: the script stops and prints it). The WAD is never in
git. With the WAD it writes `wad/Android.bp`, the module `doom_app_freedoom2_wad`, which puts it
read-only at `/product/etc/doom/freedoom2.wad`, `doom_app`'s default; the image has it only when
the product lists the module.

The engine parses the WAD, its settings and its saves with C of the 1990s, so `doom_app` refuses
a WAD (or its directory) that someone other than root or its own user may write; run as root, both
must be root's. Settings and saves go to `--data-path` (default `/data/vendor/everyware/doom`),
which must be the user's alone (0700; made so when missing). Without arguments for the engine it
starts straight into the first map (`-skill 3 -warp 1`), no menu.

## Run (RB3, from adb)

The panel runs with `--wayland-zones`. Where its socket is decides how `doom_app` runs:

- **The panel started by init** (its APEX), or by `run-smarthome-rb3.sh` on an image with that
  APEX: the socket and the FIFO are in the panel's runtime directory `/data/vendor/everyware/run`
  (0750 root:oem_2900). Only root enters it, so `doom_app` runs as root (`adb root`), with its
  binary, its WAD and its data in a directory of root's:

  ```bash
  adb root
  adb shell 'mkdir -p /data/vendor/doom && chmod 0700 /data/vendor/doom'
  adb push out_rb3/target/product/sdv_media_db845c/system/bin/doom_app /data/vendor/doom/
  adb push wad/freedoom2.wad /data/vendor/doom/
  adb shell 'chmod 0644 /data/vendor/doom/freedoom2.wad'
  adb shell 'XDG_RUNTIME_DIR=/data/vendor/everyware/run nohup /data/vendor/doom/doom_app \
      --wad /data/vendor/doom/freedoom2.wad --data-path /data/vendor/doom/state \
      > /data/vendor/doom/doom.log 2>&1 &'
  adb logcat -s doom-app
  ```

  SELinux: `doom_app` then runs in the domain of adb's root shell. The panel's domain has no rule
  for a FIFO, so on an enforcing board (`adb shell getenforce`) the panel cannot open it: the
  picture may show, the keys do not arrive. Whether the panel may map the wl_shm memory of a
  process in that domain is not checked. A domain for the game is a proposal, not made.

- **The panel run from adb on an image without the APEX**: its socket is
  `/data/local/tmp/har-display`; `doom_app` runs as the shell user with the defaults of the
  socket and the FIFO:

  ```bash
  adb shell 'mkdir -p /data/local/tmp/doom && chmod 0700 /data/local/tmp/doom'
  adb push out_rb3/target/product/sdv_media_db845c/system/bin/doom_app /data/local/tmp/doom/
  adb push wad/freedoom2.wad /data/local/tmp/doom/
  adb shell 'chmod 0644 /data/local/tmp/doom/freedoom2.wad'
  adb shell 'cd /data/local/tmp/doom && nohup ./doom_app --wad /data/local/tmp/doom/freedoom2.wad \
      --data-path /data/local/tmp/doom/state > doom.log 2>&1 &'
  ```

`--socket` and `--keys` name the panel's socket and the FIFO when they are elsewhere.

## Not verified

On the board: nothing yet. In particular two fingers that touch in the same input frame: harry's
evdev reader sends at each report only the finger of the last slot, so one of the two may never
press its key (pressing one after the other works). Not measured: what a game frame costs the
panel.

## Licenses

`src/`, the build and the scripts: Apache 2.0 (`LICENSE`). `doomgeneric/`: GPL-2.0, upstream's
(`doomgeneric/LICENSE`, `doomgeneric/README.md`). `doom_app` links both, so the binary is
conveyed under the GPL; its source is this repository. Freedoom's data: BSD
(`wad/COPYING.txt` from the release), not in this repository.
