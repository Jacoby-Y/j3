# Arcane Bash

## The game

A first-person, Vampire-Survivors-style roguelike with a wand as the only weapon. Endless: survive as long as you can.

- **Wand.** Hold left click to cast the primary spell (costs mana, has a cooldown). The secondary spell is cast free from wherever the primary ends: it fades, or hits an enemy, a shrine, or the ground. Spells live in an inventory (Tab): left click sets the primary, right click the secondary.
- **Shrines** spawn more often the longer you live. Break one with spells to add a random spell to the inventory.
- **Enemies** swarm in growing numbers (slimes, then bats, then brutes) and drop XP gems. Leveling up offers three bonuses: speed, jump height, damage, size (area), max mana, mana regen, max health, and extra damage per element (arcane, fire, ice, earth, spark).
- **Elements.** Fire burns, ice slows, earth shoves and explodes, spark chains between enemies.

Code: one module per system in `src/` (`player`, `wand` spell table and inventory, `spells` projectiles and chaining, `enemies` and gems, `shrines`, `stats` bonuses, `bits` particles, `hud`, `art` the shared cube renderer). Everything that is not the baseplate is drawn as instanced cubes through `art`.

## Visual style

- Simple, plastic, cartoon graphics.
- Everything has a square, voxel look: blocky shapes, hard edges, no smooth curves.
- Objects and enemies look cute and cartoony, built from voxel-like pieces.

# This project

Learn J3 from `j3c` only: `j3c guide`, `j3c doc`, `j3c help`. Do not search the machine, home directory, or other projects to figure out the language.

- Start from the task: `j3c doc` lists recipes and modules by what you're building; `j3c doc recipes <name>` prints a whole program that checks.
- Find a name: `j3c doc search <word>`, or `j3c doc <name>`. A module on one screen: `j3c doc <module> --short`.
- Check your work: `j3c check`, then `j3c run --frames 120 --screenshot shot.png --headless` and look at the picture (`j3c doc recipes check`).
- 3D scenes and the editor: `j3c doc recipes scene3d`, `j3c doc editor`. `j3c editor` on this project adds the scene-tool guide here.

## Shape

- `src/main.j3` is the entry. `fn main` sets up the app and returns. Game code lives in other files.
- One file, one job. Split a module when it starts doing two things.
- One way to do a thing. No copied blocks, no long if/else chains. Name the case and call it.
- Build the smallest thing that works, and leave a place to add detail later. Do not invent extra systems, settings, or polish.

## Scripts

Per-object logic that you edit while the game runs goes in a `.j3s` script (`j3c doc lang`, Scripts). A script is J3 with a `props` block (read-only, set by whoever makes the instance), per-instance `let` state, `shared` values, and `pub fn`s. Make instances from J3 with `const Door = @script("scripts/door.j3s");` and `Door { hinge: h }`, or attach a script to an object in a scene (`script=scripts/door.j3s` on its line, props as `param.<prop>=value` beside it) and run them with `j3.scene3d.scripts` (`start`, `update`, `scene.script::<Door>(node)`).

`j3c run` never recompiles the game for a script edit, and swaps new script code into the running game. Keep engine-level code (drawing, input plumbing, data structures) in `.j3`; keep tunable gameplay in scripts.

## Screen

The window can resize, including fullscreen. Pick one and keep it:

- Fixed: draw into a constant pixel size. `setMinSize` / `setMaxSize` can lock it (`j3c doc j3.game`).
- Ratio: that same layout, scaled to the window (`j3c doc j3.game windowSize`).
- Fluid: place things from the current size so windowed and fullscreen both read well.

Do not lock drawing to the size passed to `App.create` and ignore later resizes.

## Programmatic art

Use `j3c doc j3.gfx.canvas` for `Canvas`. Generate sprites, tiles, icons, and textures in J3 without a window. `Mode.Pixel` keeps exact pixel edges; `Mode.Smooth` antialiases circles, lines, and triangles. Build once during startup, call `game.textureFromCanvas`, draw the resulting texture each frame, and destroy it at shutdown. For changing art, call `game.updateTextureFromCanvas` with only the changed rectangle. `Canvas.savePng` writes a PNG from a headless J3 program into an existing directory.

    use j3.gfx.canvas;
    use j3.gfx.canvas.{ Canvas, Mode };
    use j3.game;
    use j3.game.Texture;
    use j3.memory.Arena;

    fn makeSprite(arena: &Arena): Texture {
        let sprite = Canvas.create(arena, 64, 64, Mode.Pixel);
        sprite.clear(canvas.rgba(0, 0, 0, 0));
        sprite.fillCircle(32, 32, 20, canvas.rgb(235, 176, 75));
        return game.textureFromCanvas(sprite);
    }

`j3.gfx.canvas` colors are for image pixels; `j3.game` colors are for screen drawing. The texture keeps its uploaded pixels after the canvas arena is released.

Make graphics readable at the size they appear in game. Start with a clear silhouette and a small color palette. Pick one light direction; add a darker side, a lit side, and a few highlights. Separate the subject from the background with value and contrast. Add noise sparingly after the main shapes read well. Use a fixed seed so regeneration is repeatable. Preview at native size and at the size used in game. Avoid rebuilding or uploading a static texture every frame.

For large generated images, build with `[emit] opt = 2` and measure the drawing work before using `--fast`. Normal builds keep pixel bounds checks.

## UI

Write each screen as its own `@j3ui` component so `draw` stays a short list of calls. Which names take a body: `j3c doc j3ui`. Each widget (`label`, `button`, `buttonOff`, `toggle`, `check`, `slider`, `range`, `stepper`, `scroll`, `card`, and the rest): `j3c doc j3.ui`.

    @j3ui title(ui: &Ui) {
        panel(280, 120) {
            label("title");
            button("play") {
                start();
            }
        }
    }

    fn draw(w: &World) {
        let ui = Ui.begin(w.font, x, y);
        ui.theme = Theme.dusk();
        title(&ui);
    }

`begin` opens a downward column (width 220, row height 28). Inside `@j3ui`, a bare widget name is a method on the first parameter. `panel`, `row`, `scroll`, and `card` close themselves. `button` runs its body on the click. `buttonOff` never clicks. Set `ui.theme` after `begin` (`Theme.dusk`, `Theme.yard`, or a few colors of your own). Place `x` and `y` from `windowSize` so a panel stays put when the window changes. Two widgets with the same caption need `pushId` / `popId`.

## Fonts

`Ui.begin` takes a font. Load a `.ttf` that lives in the project (`assets/font.ttf`).

`tryLoadFont` returns an empty font when the file is missing or SDL_ttf is not installed. Labels still draw. `loadFont` panics. The same split holds for images (`tryLoadTexture` / `loadTexture`) and sounds (`tryLoadSound` / `loadSound`).

`j3c check` does not run the game. It fails when a library or a file the program loads is missing on this machine. `j3c run` uses that check and does not launch when it fails. `j3c: program exited` is a failed run, even when a window appeared. Read stderr.

## 3D scenes and the editor

Scenes are `scenes/*.j3scene` text files that `j3c editor` edits and the game loads with `j3.scene3d` (`j3c doc recipes scene3d`, `j3c doc editor`). Edit them through the tools below, not by hand: they keep ids, parents, and undo correct. Nothing here needs the editor window open.

MCP server (once): `claude mcp add -s user j3 -- j3c -e mcp`. Its startup instructions and tool descriptions say when to use each tool. Without MCP, the same operations are `j3c -e scene --help`, `j3c -e render --help`, `j3c -e gen`, and `j3c -e mcp --tools` (every tool's arguments next to its CLI twin).

Loop: change, look, check.

1. Learn the scene: `scene_list` (glob `filter`, `group_by=prefix`), `scene_get`.
2. Build with bulk tools, not many single calls: `place` (`ring`, `wall` with a door gap, `stairs`, `spiral`, `grid`, `scatter`, `ground`, `ontop`), `scene_add_many`, and globs in `scene_set` / `scene_delete` (`DecoTree*`). Meshes built in code: `assets/gen/*.j3` (`j3c doc recipes meshgen`).
3. Look from where the player stands: `render` with `at` / `yaw` / `pitch`, `views` for several angles in one image, `hide` (`Roof*`) to see inside a building, `view=top` with `ortho` for a plan. Render after every few edits, not once at the end.
4. Verify: `check` (scene problems, this project's own rules, and code), `scene_stats` (draw cost: above 60 distinct looks is slow, so snap tints to a few values), `scene_diff` (review your own edits against git HEAD), `play` (runs the game headless and returns a frame).

Facts that cost time:

- A scene file is one line per object (`Name kind key=value #id`, indent for children, defaults left out) and rotations are degrees there and in the tools; `j3c doc editor` has the format. Edit through the tools; after a hand edit run `j3c -e scene fmt`. `check` warns on a rotation over two turns. Yaw 0 looks toward -z, 90 toward -x. A slab pointing along `(dx, dz)` needs `rot` y `= -atan2(dz, dx)` degrees.
- With `j3c editor` open, tool edits are live undo steps there and unsaved until the person saves. With it closed they write the file and `undo` restores it (20 kept). Every reply says which.
- Give objects `tags` (`scene_set tags=nocollide,launch=30`) and read them with `scene3d.hasTag` / `tagValue`, instead of matching name prefixes.
- Put this project's rules in a check script so a mistake names the object instead of showing up in play: `check = "tools/check.j3s"` under `[editor]` in `j3.editor.toml`. It runs as `j3c run tools/check.j3s <scene>` and prints `error: ...` or `warning: ...` lines (read the scene with `scene3d.read`). Project-specific notes for agents go in a file named by `notes = "AGENT_NOTES.md"` there; the MCP server adds it to its instructions.
