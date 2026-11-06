# Standard Library Map

The standard library is written in J3 and ships inside `j3c`. This page is a map; for names and signatures, run `j3c doc <module>`.

## Core

| Module | What it's for |
|---|---|
| `j3.io` | Printing and files |
| `j3.sys` | Process and OS: arguments, environment, `panic`, which OS (`PLATFORM == PLATFORM_WINDOWS`, `PLATFORM_NAME`; which CPU: `ARCH == ARCH_AARCH64`, `ARCH_NAME`) |
| `j3.math` | `f32` math, `Vec2`, `Vec3`, `Quat`, `Mat4` |
| `j3.rand` | Seedable random numbers |
| `j3.memory` | `Arena`, `List<T>`, the scratch arena |
| `j3.text` | Building and editing text |
| `j3.option`, `j3.result` | `Option<T>` and `Result<T, E>` |
| `j3.thread` | Threads, pools, queues, mutexes |
| `j3.build` | `DEV` / `RELEASE` constants |
| `j3.proc` | Run other programs |

## Games

| Module | What it's for |
|---|---|
| `j3.app` | Game runner: startup, update and draw stages, fixed time step |
| `j3.game` | Window, input, 2D drawing, text, audio |
| `j3.game.input` | Named actions bound to keys and pad buttons |
| `j3.game.display` | Video settings: window mode, resolution, vsync |
| `j3.game.ecs` | Entities, components, commands, events |
| `j3.game.physics` | 2D bodies, and 3D bodies on voxels |
| `j3.game.collide` | AABB and circle tests, tile-grid sliding |
| `j3.game.space` | Grid for broadphase queries |
| `j3.game.path` | A* on a grid |
| `j3.game.anim` | Sprite clips and transform tracks |
| `j3.game.fx` | Timers, tweens, particle bursts |
| `j3.game.tile` | Tile maps |

## Graphics

| Module | What it's for |
|---|---|
| `j3.gfx` | OpenGL 3.3 forward renderer: meshes, lights, shadows |
| `j3.gfx.paint` | Batched 2D painter: shapes, text, clip, transforms |
| `j3.gfx.canvas` | Draw RGBA images in memory, no window; save PNGs |
| `j3.gfx.mesh`, `j3.gfx.meshgen` | Mesh files and meshes built in code |
| `j3.gfx.voxel` | Voxel grids |
| `j3.gfx.noise` | Tileable noise for textures |
| `j3.gfx.urp.*` | Optional ray-traced reflections, glass, and shadows |
| `j3.scene3d` | Scenes from the editor: load, script, draw |
| `j3.scene3d.fpwalk` | First-person controller |

## UI

| Module | What it's for |
|---|---|
| `j3.ui` | `.j3ui` components: markup, CSS, events (`j3c doc j3ui`, `j3c doc css`; supported HTML/CSS overview: [j3ui Feature Support](j3ui-support.html)) |
| `j3.i18n` | Translated text with plurals |

## Data and network

| Module | What it's for |
|---|---|
| `j3.data.save` | Simple key/value save files |
| `j3.data.assets` | Load-once asset cache |
| `j3.data.json` | JSON parse and write |
| `j3.net` | Non-blocking UDP |
| `j3.net.tcp`, `j3.net.dns` | TCP connections and name lookup |
| `j3.net.http`, `j3.net.tls` | HTTP/1.1 and HTTPS clients |
| `j3.steam`, `j3.steam.game`, `j3.steam.ui` | Steamworks: achievements, cloud, Deck, lobbies; no-op without Steam (`j3c doc steam`) |

Networking and processes work on Linux and Android.
