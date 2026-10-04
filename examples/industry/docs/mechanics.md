# How the game works

Controls and the short version are in [../README.md](../README.md). This page is the full rules.

## World and chunks

The world has 81 chunks in a 9 × 9 square, each 48 × 132 × 48 voxels (a voxel is 0.5 m). Only the center chunk starts unlocked. Approach a dark boundary and press E to unlock the neighboring chunk for coins: (40 + 20 per chunk already open), +50% per ring beyond the first ring around the start. Every new game logs a random world seed; generation is independent of unlock order.

Square rings around the center control terrain: radius 0–1 is flat, radius 2 rises up to 3 blocks, radius 3 up to 6, and radius 4 up to 12. Hills blend across chunk edges.

The underground is 100 voxel blocks (50 m) deep, with a protected stone floor. Caves are carved from 3D noise: winding tunnels, wide low caverns and thin links, denser toward the outer rings. Inner rings retain solid cave roofs; radius-4 chunks have a 10% chance of a sinkhole shaft from the surface down into the cave layer.

## Ore

Every chunk has one persistent 2 × 2 × 2 **surface node** per ore, at its local ground height. Nodes never run out. Drills mine them; the pickaxe yields one item per two-second hold.

Each chunk also targets 48 finite underground **veins**, split evenly between ores. A vein is 1–3 touching single-voxel ore blocks, each 0.5 m across. Select the pickaxe and hold left click for two seconds to break one for 1–6 of its ore. The whole drop must fit in your inventory or the block remains. Drills ignore veins.

## Building

Press Tab to open the 6 × 6 inventory and the build menu on its right. The separated top row is the six-slot hotbar (keys 1–6 and the mouse wheel). New games start with one pickaxe in slot 1. Crafted items fill the next free slots, and you can rearrange them by clicking a stack and then another slot (including the pickaxe).

The text tabs in the build panel group recipes into Logistics (conveyor, container), Production (drill, furnace), and Trade (market). Hover a recipe to see its cost. Each structure recipe costs one copper ore and adds one structure to your inventory; placement consumes it only on success. Mine ore with the pickaxe to fund construction.

Hold left click to remove blocks and structures, right click to place, and R to rotate. All structures keep the selected compass direction until R changes it; looking around, switching tools, and placing never rotate it. The HUD shows the current direction. Removing terrain adds that exact block to your inventory; select it and right-click to place one voxel against the aimed face. Removing a structure recovers one structure item. A full inventory prevents block removal.

Ordinary ground placement snaps to the 0.5 m grid. Structures are two voxels wide and deep.

## Drill

Select a crafted drill and aim at copper or coal: the full ghost snaps beside the ore using the selected direction. R changes that direction. Right click to place when the footprint is green; a red footprint shows the reason it cannot be placed.

A drill produces one item every four seconds from the surface node it faces, holds up to ten, and stops mining when full. Aim at a drill to see its contents; press E to take everything it holds (what does not fit stays). The spinning bit, rear chute, and belt chevrons show the working direction.

## Conveyors

Select a crafted conveyor and aim at the placed drill: the ghost snaps to its blue rear output at the correct height. Use R to point the belt away from the drill. Step back if your body is in the way, then right click.

Aim at any belt in a line to extend its end, following the outputs through turns. The ghost shows that end; R selects the new segment's compass direction, including corners. Hold Shift while aiming at a belt to stack directly above it instead. Closed loops have no end and cannot be extended this way.

Each belt carries up to four items with fixed spacing and hands them to the next belt when it can accept them. A belt accepts from its rear or either side and rejects opposing outputs. Extend the line to clear a blocked drill output.

Iron ore (nodes and veins) exists only from the second chunk ring out; it smelts to iron ingots. Mk.2 conveyors cost iron ingots and carry three times as fast as the basic ones, and mix freely with them. A splitter takes cargo from any side and hands it round-robin to the three other sides, skipping any that is blocked; it can feed belts or machines. A lifter takes cargo from any side and passes it to the belt (or lifter) covering the cell above it; aiming a belt at a lifter places that belt on top. Splitters and lifters come in both tiers and cost copper ore (basic) or iron ingots (Mk.2).

## Furnace

A furnace occupies 2 × 2 × 4 voxels. Its orange intake and blue outlet are on opposite sides, one voxel above its base. The selected direction points from intake to outlet. Feed raw copper on a belt pointing into the intake, at the same base height as the furnace. A belt one voxel higher, lower, or on another side cannot feed it. Connect an outgoing belt at the outlet, at that same height and pointing away.

**It needs coal.** Each ore needs a lit coal, and one coal burns five smelts. The furnace holds up to ten ore and ten coal. Belts deliver both; aiming at the furnace, E adds one coal and F adds one ore from your inventory. Two seconds later one copper ore becomes one copper ingot; a blocked or disconnected output holds the finished batch and stops further intake.

## Container

A container occupies 2 × 2 × 3 voxels and stores up to 256 items shared across item types. Aim at the end of a conveyor to snap it onto the line. A belt pointing into any of its four sides can feed it, regardless of the container rotation. Keep belts at the same base height as the container; their cargo enters one voxel above the ground. Items remain on the belt when storage is full.

Aim at any part of the container and press E to open it. The generic inventory panel shows container stacks and carried stacks in two icon grids, with counts and hover labels. Click a stack to move one item to the other inventory. Shift-click or right-click moves the entire stack, up to the available space. Each inventory holds 256 items, and transfers stop at the available space. E, Escape, or Close returns to gameplay. Automation keeps running while the panel is open. Empty containers before removing them.

## Market

The market console is a storage machine that sells basic resources (copper ore, coal, copper ingots, stone, dirt, and grass) for coins. Structures, tools, and foundations cannot be sold; the sale queue only accepts sellable items. Items you deposit settle when you close the panel; belts pointing into the console sell immediately.

## Limits

8,192 machines, 128 storage machines (containers and markets). Past a limit the placement preview names the reason instead of placing.

## Rendering

Terrain meshes rebuild in 24 × 16 × 24 voxel sections, including shared boundaries and vertical AO neighbors; machine state is allocated only for placed machines. Nearby terrain uses detailed voxel faces and ambient occlusion. Beyond 32 m from a chunk's boundary, rendering switches to cached merged rectangles with simpler colors; detail returns within 24 m to prevent repeated switching at one distance. Both levels preserve the exact terrain shape, cave openings, material boundaries, and mined holes. Shadow rendering always uses the merged geometry. Sections are culled separately; fully visible chunks submit that same cached geometry in one shadow draw. Edits rebuild both mesh versions; walking only selects cached meshes. Structure and resource-node models keep their detail through exact face merging. Visible machines, animated parts, resource nodes, and cargo use instanced batches. Camera and shadow passes cull independently; offscreen automation continues updating.

Machines simulate at a fixed 60 steps per second, independent of frame rate.

Measurements are in [../benchmarks/PERFORMANCE.md](../benchmarks/PERFORMANCE.md).

## Electricity

Craft copper wire, posts, a coal generator, an electric drill and an electric smelter in the POWER tab (they cost copper ingots). Hold wire and right click a post or electric machine, then a second one: that connects them with one wire (up to 12 m). A wire always has a post at one end, so every machine needs its own wire to a post, and posts chain to each other. Clicking a wired pair again takes the wire back; after wiring a post the line continues from it. Breaking a machine drops its wires.

Everything joined by wires is one network. Each lit generator supplies 100 W, an electric drill working asks 30 W and an electric smelter working asks 50 W. When demand is above supply every machine on the network runs at the supplied share. Generators burn a coal (20 s) only while their network wants power, and take coal from a belt or from E. Powered machines run twice as fast as their fuel-driven versions; the smelter needs no coal. The numbers are props in `scripts/power.j3s`, `drill.j3s` and `furnace.j3s`.
