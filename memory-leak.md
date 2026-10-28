# Desktop session failure and possible J3 memory leak

Investigation date: October 4, 2026. Times below are local CDT (UTC−05:00).

## Status and purpose

**J3 has not been proven free of memory leaks.** This investigation identified a desktop message-bus quota failure and staged a workaround. It did not identify the application responsible for the resource buildup, nor perform a comprehensive J3 memory audit.

The user reports that unexpected logouts seem to occur primarily while playing or developing `examples/industry`, sometimes roughly every 30 minutes. Treat that correlation as a significant lead. A previous statement that the short test pointed away from the game must not be read as clearing J3 or SDL of responsibility.

This document preserves the findings for an agent investigating a recurrence. No compiler, runtime, or game code was changed during this investigation. The repository already contained substantial unrelated uncommitted changes; preserve them.

## What happened

Firefox was playing a YouTube video. Pressing the keyboard's pause/play media control was immediately followed by a black screen and a return to the login screen. After signing back in, all application windows were closed.

The boot and login records show that the machine remained running: it booted at 07:26 and the affected desktop session ended at approximately **12:06:15**. This was a GNOME session failure, not a full reboot. No kernel panic, GPU reset, or kernel out-of-memory kill was found in the examined failure window. That statement applies to this incident, not any future failure.

## Verified evidence from the journal

The affected session bus was `dbus-broker`, PID **48342**, running for UID **1000**.

At **12:03:56.592013**, it logged:

```text
UID 1000 exceeded its 'bytes' quota on UID 1000.
```

The structured journal fields recorded:

```text
DBUS_BROKER_USER_CHARGE_AMOUNT=1600
DBUS_BROKER_USER_CHARGE_REMAINING=213
DBUS_BROKER_USER_CHARGE_CONSUMED=276447019
DBUS_BROKER_USER_CHARGE_N_ACTORS=1
DBUS_BROKER_USER_CHARGE_SLOT=bytes
```

Peer `:1.125` was then disconnected. Its process identity was not recovered. Do not assume it was the game or Firefox.

At **12:06:15.748826**, the broker began disconnecting many desktop clients. The first recorded broadcast involved:

```text
sender: org.gnome.SessionManager (:1.29)
path: /org/gnome/SessionManager
interface: org.gnome.SessionManager
member: InhibitorRemoved
first recipient: org.gnome.SettingsDaemon.Sharing (:1.49)
```

Other disconnected clients included GNOME sound, keyboard, color, media-key, power, and window-settings services; desktop portals; the session manager; and Firefox's MPRIS media-control connection (`org.mpris.MediaPlayer2.firefox.instance_1_100`). GNOME Shell then logged `Shutting down GNOME Shell`, and systemd stopped the graphical-session targets.

The quota problem therefore existed **more than two minutes before the final logout**. Earlier in the same boot, the journal also recorded quota exhaustion at **08:48:53** and additional peer disconnections later that morning, including around **10:47:03**, when another desktop session ended.

### Interpretation of the pause key

Pausing Firefox likely removed a playback-related idle/sleep inhibitor, producing the `InhibitorRemoved` notification that encountered the exhausted bus quota. This is an inference consistent with the timing and message type, not a captured proof of the entire keypress-to-logout chain.

The media-key bindings inspected were ordinary `XF86AudioPause` / `XF86AudioPlay` bindings; logout was bound to Ctrl+Alt+Delete. There was no evidence that the pause key was intentionally configured to log out.

## Verified broker quota problem

The installed broker version was **35-2ubuntu0.1**. Its session process was launched with:

```text
--max-bytes 100000000000000
```

Upstream version 35 accepts this value as a 64-bit argument in `broker_new`, but passes it into `bus_init`, whose quota arguments are `unsigned int` (32 bits on this machine). The configured value truncates to:

```text
100000000000000 modulo 4294967296 = 276447232 bytes
276447019 consumed + 213 remaining = 276447232 bytes
```

Live broker debug statistics independently reported **276447232** as the user's byte limit. This exact numerical match supports the truncation diagnosis, rather than merely assuming that all physical RAM was exhausted.

Relevant upstream source:

- [v35 broker constructor](https://github.com/bus1/dbus-broker/blob/v35/src/broker/broker.c)
- [v35 bus initialization](https://github.com/bus1/dbus-broker/blob/v35/src/bus/bus.c)
- [v35 quota accounting](https://github.com/bus1/dbus-broker/blob/v35/src/util/user.c)
- [v35 accounting field types](https://github.com/bus1/dbus-broker/blob/v35/src/util/user.h)
- [Upstream report of a GNOME session dying after broker quota disconnections](https://github.com/bus1/dbus-broker/issues/167)

**Two questions remain separate:** why the broker imposed this unexpectedly small limit, and what consumed the available allowance. The first has strong direct evidence; the second is unresolved.

## Relevant environment

Recorded during the investigation:

| Component | Version / configuration |
|---|---|
| OS | Pop!_OS 24.04 LTS |
| Desktop | GNOME, X11 session |
| Kernel | 7.1.5-76070105-generic |
| GNOME Shell | 46.0-0ubuntu6~24.04.14 |
| GNOME Settings Daemon | 46.0-1ubuntu1.24.04.1 |
| Firefox | 1:156.0.1~1790100279~24.04~4154c45 |
| dbus-broker | 35-2ubuntu0.1 |
| dbus-daemon | 1.14.10-4ubuntu4.1 |
| SDL2 | 2.30.0+dfsg-1ubuntu3.1 |
| Industry build | LLVM backend, opt 0, debug enabled, links SDL2 and GL |

`coredumpctl` was not installed. `/var/crash` contained older reports, but no report for this desktop incident.

## J3 and industry inspection

Relevant code paths examined:

- `examples/industry/src/main.j3`: creates one `App<Game>`, enables GL, registers startup/update/draw callbacks, and calls `app.run()`.
- `j3c/stdlib/j3/app.j3`: initializes the window once, polls and renders in its loop, then invokes `game.destroy()` and releases its arenas on exit.
- `j3c/stdlib/j3/game.j3`: initializes SDL, polls SDL events, opens controllers, and tears down controllers, GL context, window, and SDL on normal exit.
- `examples/industry/src/session.j3`: game startup and state ownership.
- `examples/industry/src/mesher.j3` and `mesh_job.j3`: worker-pool and mesh-buffer ownership, inspected only briefly; not audited for leaks or races.
- Compiler run/host plumbing: no direct D-Bus or desktop-notification calls were found in the searched paths.

No direct D-Bus calls were found in the searched J3 stdlib or industry sources. **This does not mean the process never uses D-Bus:** SDL2 uses desktop services internally, including screensaver inhibition/activity calls. See [SDL 2.30.0 Linux D-Bus implementation](https://github.com/libsdl-org/SDL/blob/release-2.30.0/src/core/linux/SDL_dbus.c).

An observation for a deeper audit: `game.ensurePads()` attempts to open controller indices 0 and 1 each time it runs if the stored handles are null; it runs during initialization and each poll. No resource growth was demonstrated from this behavior. Do not label it the cause without measuring it.

An ordinary heap leak in J3 does not directly leak into another process's memory. However, J3/SDL could cause message queues, pending calls, match rules, or desktop resources to accumulate through their interactions. General memory exhaustion is another possible failure mode, but was not shown in this incident's examined kernel logs.

## Tests performed and their limits

### Existing industry development binary, real desktop bus

Ran `examples/industry/target/industry-dev`, last modified at 11:58 that day, for approximately **22 seconds**. Used broker `GetStats` samples at approximately 2, 12, and 22 seconds.

```text
Baseline bus usage:          1128257 bytes
Bus usage during game:       1133647 bytes, unchanged across all three samples
Bus usage after game exited: 1128257 bytes

Game peer: :1.104, PID 63902
MatchBytes: 1300
Matches: 1
ReplyObjects: 0
IncomingBytes: 16
OutgoingBytes: 0
```

The overall startup increase was **5390 bytes** and disappeared after the game exited. These figures are broker-accounted resources, not the game's total RAM consumption. They demonstrate no observed growth in this brief idle run.

### Isolated dbus-daemon test

Ran the same game for approximately **10 seconds** under `dbus-run-session`. The game stayed running, the private daemon answered statistics queries, and the game exited with status 0 after SIGTERM. The isolated bus emitted warnings because a full GNOME session was not running on that bus; it was not a test of the entire desktop.

### Not tested

- A 30-minute or longer development/gameplay session.
- RAM, allocator, mmap, or GPU-memory growth over time.
- Repeated compiler builds, development relaunches, script hot reloads, or editor sessions.
- Extensive movement, terrain generation, mesh regeneration, particles, inventory, or machine simulation.
- Behavior of earlier builds or intermittent failures.
- A fresh build of the current sources; the existing executable was tested.
- Full desktop operation after activating the workaround.

**These tests cannot clear J3, its compiler, SDL, or the game of memory leaks.**

## Workaround staged on this machine

No newer broker candidate appeared in the configured apt package metadata. The standard `dbus-daemon` was already installed, with a distribution-provided service command in `/usr/lib/systemd/user/dbus.service`.

Created this user override:

```text
/home/jacoby/.config/systemd/user/dbus-broker.service.d/90-use-dbus-daemon.conf
```

Its service settings are:

```ini
[Service]
NotifyAccess=main
ExecStart=
ExecStart=/usr/bin/dbus-daemon --session --address=systemd: --nofork --nopidfile --systemd-activation --syslog-only
ExecReload=
ExecReload=/usr/bin/dbus-send --print-reply --session --type=method_call --dest=org.freedesktop.DBus / org.freedesktop.DBus.ReloadConfig
```

The unit keeps the name `dbus-broker.service`, but starts `dbus-daemon` when next launched. This affects this user's primary session-bus service; it does not replace the system bus or the separately launched accessibility bus.

Validation completed:

- `systemd-analyze --user verify dbus-broker.service` passed.
- `systemctl --user daemon-reload` loaded the override.
- `systemctl --user show` displayed the expected future `ExecStart` and override path.
- A private dbus-daemon started successfully and answered `GetId`.

**As of 12:26 CDT, the current primary desktop bus was still the old broker, PID 61980 for its launcher.** The change was staged, not active. The user was instructed to save work and reboot once. No reboot, logout, or desktop-bus restart was performed by the agent.

This is a workaround for the verified broker failure mechanism, not proof that the underlying resource buildup is solved. dbus-daemon also has resource limits.

### Verify after reboot

```bash
systemctl --user show dbus-broker.service -p MainPID -p ExecStart -p DropInPaths
ps -C dbus-daemon,dbus-broker-launch,dbus-broker -o pid,comm,args
```

Check the executable of the unit's actual main PID. The configured `ExecStart` alone does not prove a previously running service has switched. System/accessibility broker processes can legitimately remain.

### Rollback

Remove only the custom `90-use-dbus-daemon.conf` file, reload the user systemd configuration, and reboot after saving work. Preserve other overrides, including the distribution's `10-cosmic-nofile.conf`.

**Do not restart or stop the active desktop message bus during normal work.** Doing so can disconnect applications and terminate the session.

## If another failure occurs

Record the exact local time, whether the machine truly rebooted, whether the game was open, how long it had run, and what happened immediately beforehand. Include whether Firefox was playing, a media key was pressed, the game was rebuilt/relaunched, or scripts were hot reloaded.

Before changing settings, preserve evidence. Adapt the date/time window below to the recurrence:

```bash
date --iso-8601=seconds
uptime
last -x -n 12
journalctl --list-boots --no-pager
journalctl --since '2026-10-04 12:00:00' --until '2026-10-04 12:08:00' -o short-precise --no-pager
journalctl -k --since '2026-10-04 12:00:00' --until '2026-10-04 12:08:00' --no-pager
```

If a full reboot occurred, inspect `journalctl -b -1` as well. If the desktop merely logged out, the relevant journal is likely still in the current boot. Preserve complete output to files before filtering, since a large wave of shutdown errors can hide the first failure.

For the original incident, the most informative retrieval commands are:

```bash
journalctl --since '2026-10-04 12:03:55' --until '2026-10-04 12:03:58' _PID=48342 -o verbose --no-pager
journalctl --since '2026-10-04 12:06:15' --until '2026-10-04 12:06:16' _PID=48342 -o verbose --no-pager
```

## Deeper investigation plan

1. **Confirm the active bus implementation first.** Determine whether a recurrence used the old broker or the staged daemon workaround. Do not assume it activated.
2. **Reproduce the user's actual development cycle.** Measure ordinary gameplay, terrain exploration, editing/hot reload, repeated relaunches, and compiler runs separately. Record exact binaries, source revisions/diffs, environment variables, and launch commands. Do not overwrite uncommitted work or saves.
3. **Sample resources while the processes are still alive.** Record game/compiler/editor PID, RSS, anonymous memory, thread count, file-descriptor count, and GPU process memory where available. `/proc/PID/status`, `/proc/PID/smaps_rollup`, `/proc/PID/fd`, and `nvidia-smi` are useful starting points. Distinguish reserved address space, committed memory, caching, and a sustained unreclaimed increase.
4. **Sample desktop-bus accounting over time.** Capture `busctl --user list` to map unique connection names to PIDs, along with statistics. For the broker, this call returned per-peer and per-user accounting:

   ```bash
   busctl --user call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus.Debug.Stats GetStats
   ```

   Compare `MatchBytes`, `Matches`, `ReplyObjects`, `IncomingBytes`, and `OutgoingBytes`. The daemon's statistics format differs; inspect its debug interface and use connection statistics if available. Peer names are session-specific and cannot be identified later from a new session's bus list.
5. **Identify the accumulating owner before blaming an application.** If queues grow, map recipients and senders while still connected. Use targeted, bounded D-Bus monitoring if accounting alone is insufficient; avoid indiscriminately recording sensitive desktop traffic or generating huge logs.
6. **Audit J3 ownership and concurrency where measurements point.** Examine arena lifetime/rewind behavior, mmap/munmap balance, mesh-job buffers, worker-pool shutdown, queue bounds, script reload ownership, SDL handles, and GL resource destruction. In-process memory growth and bus growth are separate measurements.
7. **Use targeted leak instrumentation.** Consider sanitizers or Valgrind for a small reproducer that retains the problematic behavior, accounting for J3's custom allocation paths and emitted LLVM/C. Check emitted native call signatures and thread safety when justified by evidence. A successful compiler check or ordinary game test is not a leak test.
8. **Compare controlled runs.** Compare with/without the game, Firefox playback, repeated builds, and hot reload; use the same workload and duration. Stop before exhaustion if growth is clear. Avoid reproducing a destructive desktop logout just to confirm an already visible trend.

Success means identifying the owner and mechanism of growth, fixing it with a focused change, and demonstrating stable resources under the previously failing workload for longer than its usual failure interval. A short idle run or merely switching the bus implementation is not sufficient evidence that J3 has no leak.
