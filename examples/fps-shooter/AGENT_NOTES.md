Scene rules for this game (the `check` tool runs tools/check.j3s for each scene):
- Biomes are scenes/biomes/<name>.j3scene, centred on 0,0 inside a 160 m tile (+-80); main.j3scene holds the hub, cliffs and ledges. src/level.j3 merges them.
- Behaviour still comes from name prefixes in src/level.j3: `Deco*` no collision (`DecoFog`, `DecoCorn`: bullets pass too), `JumpPadNN` launches at NN, `Lava*` burns, `Ice*` slides, `ShrineSpot*` groups mark shrine spots. A typo silently changes gameplay. Tags (scene_set tags=...) exist but the game does not read them yet.
- check errors: a JumpPad with no floor 1.3 m around it at its height; a ShrineSpot resting on nothing. Warnings: something solid crowding a pad, content outside the tile.
- Scene files store rotation in radians. Several older objects (LavaRiver*, DecoBit*, DecoPath*) hold degrees; check warns on them.
- Render interiors with at/yaw/pitch (barns, tombs, the silo), hide roofs with hide="BarnRoof*". Keep one biome under 60 distinct looks (scene_stats); snap tints to a few values.
- tools/arena.j3s regenerates a comparison copy in target/, it no longer writes scenes/.
