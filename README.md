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
CPU, `src/zone.rs` scales it to the size the panel configures for the zone (1040 x 780, 4:3, in
the dialog), writes it into a wl_shm buffer and presents it to the zone `#zone/game` over harry's
Wayland client shim (har-wayland-client, the `har_zone` protocol). The panel copies the 3.2 MB,
uploads them as a texture and draws them into `#game/picture`. The `har_zone` protocol says that
a buffer of another size is scaled into the node, but harry's renderer draws it 1:1 in the
node's top left corner; with a renderer that scales, `--native-size` sends the 256 KB frames as
they are and lets the panel's GPU scale them.

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

`scripts/fetch-freedoom.sh` downloads Freedoom 0.13.0 from its GitHub release (or takes a zip
downloaded before, `FREEDOOM_ZIP=<file>`), checks the zip's size and SHA-256 against the ones
pinned in the script and writes `wad/freedoom2.wad` with its license `wad/COPYING.txt`. GitHub
publishes no digest for the asset: the pinned hash is the one recorded at our first download. The
WAD is never in git. With the WAD it writes `wad/Android.bp`, the module `doom_app_freedoom2_wad`,
which puts it read-only at `/product/etc/doom/freedoom2.wad`, `doom_app`'s default.

The engine parses the WAD, its settings and its saves with C of the 1990s, so `doom_app` checks
the WAD's absolute path (the one the engine opens) from `/` down: every directory and the WAD
belong to root, Android's system user or its own user, and nobody else may write them; run as
root, a directory of the shell user fails. Settings and saves go to `--data-path` (default
`/data/vendor/everyware/doom`), which must be the user's alone (0700; made so when missing).
Without arguments for the engine it starts straight into the first map (`-skill 3 -warp 1`), no
menu.

## In an image: EVERYWARE_GAME

The game is experimental and off by default. A board product inherits `product/doom.mk`
(device-rb3 does); with `EVERYWARE_GAME=true` the image gets `doom_app` (`/system/bin`), the WAD
and `ro.vendor.everyware.game=true`, by which the panel shows its game button:

```bash
packages/services/display_safety/utils/doom-app/scripts/fetch-freedoom.sh   # once: wad/ and its module
bash -c 'source device/rb3/sdv_media_db845c/tools/env.sh >/dev/null && EVERYWARE_GAME=true m'
```

Nothing starts `doom_app` in the image: a service and a SELinux domain for it are a proposal, not
made. It is started from adb, as below.

## Run (RB3, from adb)

The panel runs with `--wayland-zones`, and shows the game button only with
`ro.vendor.everyware.game=true`: an image without the game gets it once per boot, as root, before
the panel starts (`adb shell setprop ro.vendor.everyware.game true`, then start the panel again).
Where the panel's socket is decides how `doom_app` runs:

- **The panel started by init** (its APEX), or by `run-smarthome-rb3.sh` on an image with that
  APEX: the socket and the FIFO are in the panel's runtime directory `/data/vendor/everyware/run`
  (0750 root:oem_2900). Only root enters it, so `doom_app` runs as root (`adb root`). With the game
  in the image:

  ```bash
  adb root
  adb shell 'XDG_RUNTIME_DIR=/data/vendor/everyware/run nohup doom_app > /data/vendor/everyware/doom.log 2>&1 &'
  adb logcat -s doom-app
  ```

  Without it, the binary, the WAD and the data in a directory of root's:

  ```bash
  adb root
  adb shell 'mkdir -p /data/vendor/doom && chmod 0700 /data/vendor/doom'
  adb push out_rb3/target/product/sdv_media_db845c/system/bin/doom_app /data/vendor/doom/
  adb push wad/freedoom2.wad /data/vendor/doom/
  adb shell 'chmod 0644 /data/vendor/doom/freedoom2.wad'
  adb shell 'XDG_RUNTIME_DIR=/data/vendor/everyware/run nohup /data/vendor/doom/doom_app \
      --wad /data/vendor/doom/freedoom2.wad --data-path /data/vendor/doom/state \
      > /data/vendor/doom/doom.log 2>&1 &'
  ```

  SELinux: `doom_app` then runs in the domain of adb's root shell. The panel's domain has no rule
  for a FIFO, so on an enforcing board (`adb shell getenforce`) the panel cannot open it: the
  dialog says that the game may be running but the panel may not send it keys, and no key
  arrives. `doom_app` removes its FIFO when it ends (exit, SIGTERM, SIGINT), so a FIFO there means
  a game; one killed with SIGKILL leaves its FIFO, which the panel then takes for a game. Whether
  the panel may map the wl_shm memory of a process in that domain, and so show the picture, is
  not checked. A domain for the game is a proposal, not made.

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

`--socket` and `--keys` name the panel's socket and the FIFO when they are elsewhere. `doom_app`
refuses a FIFO of another user: after a run as root, a run as the shell user (or the other way
round) first removes the old one (`adb shell rm /data/local/tmp/har-game-keys`, as the user who
made it, or the one in `/data/vendor/everyware/run` as root).

## Not verified

On the board: nothing yet. In particular two fingers that touch in the same input frame: harry's
evdev reader sends at each report only the finger of the last slot, so one of the two may never
press its key (pressing one after the other works). Not measured: what a game frame costs the
panel.

## Licenses

This repository is GPL-3.0-or-later (`LICENSE`), and so is `doom_app` as a whole: it links the
engine, `doomgeneric/`, which is GPL-2.0 or later as upstream has it (`doomgeneric/LICENSE`,
`doomgeneric/README.md`: upstream's commit), and harry's Apache 2.0 Wayland shim and logger.
Freedoom's data: BSD-3-Clause (`wad/COPYING.txt` from the release), not in this repository.
