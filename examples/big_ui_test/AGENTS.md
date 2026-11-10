# This project

Learn J3 from `j3c` only: `j3c guide`, `j3c doc`, `j3c help`. Do not search the machine, home directory, or other projects to figure out the language.

- Start from the task: `j3c doc` lists recipes and modules by what you're building; `j3c doc recipes <name>` prints a whole program that checks.
- Find a name: `j3c doc search <word>`, or `j3c doc <name>`. A module on one screen: `j3c doc <module> --short`.
- Heavy work off the frame thread (meshing, generation, decoding): `j3c doc recipes threads`, then `j3c doc threads`. Window, GL, SDL, and `j3.gfx`/`j3.game`/`j3.scene3d` state are main-thread only; workers compute and hand results back.
- Check your work: `j3c check`, then `j3c run --frames 120 --screenshot shot.png --headless` and look at the picture (`j3c doc recipes check`, `j3c doc testing`). Never run the game without `--headless` (a bare `j3c run` opens a window); a built binary needs `SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy` on the same command. Wrap runs in `timeout`.
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

## .j3ui components (this project's UI)

This project's UI is `ui/*.j3ui` (markup + J3 + CSS, compiled by j3c), not the `@j3ui` immediate-mode section above. Guides: `j3c doc j3ui` (markup, props, events, bind, canvas), `j3c doc css` (what CSS works and what differs from a browser), `j3c doc recipes components`, `j3c doc recipes paint` (j3.gfx.paint straight from a game).

- `ui/Name.j3ui` becomes `Name` / `NameProps` in module `<package>.ui`. `src/main.j3` mounts the root and calls `root.render(windowSize(), props)` each frame. Game state lives in plain J3 (`src/state.j3`); components read it through `props`.
- Style in the component's `<style>` and the global `ui/theme.css`. Unsupported CSS warns at startup (`warning: file:line:col`); check it after a run.
- `j3c dump-ui ui` prints the generated J3; compile errors point at the `.j3ui` line.
- Look and test without a window: `j3c run --frames 30 --screenshot shot.png --headless`, then read the PNG. Never run the app without `--headless`. For a built binary: `SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy timeout 20 ./target/<name>`.
- Goldens: list states in `golden.toml` and run `j3c golden .` (`--bless` to accept; by hand `J3_CAPTURE=out.png:30 J3_FIXED_DT=0.016` then `j3c -e imgdiff golden/x.png out.png`). Idle-cost check: `J3_STEADY=60:120`. Fake clicks and typing: `j3.ui.testkit`. Layout as text: `j3.ui.domhtml.dumpHtml(&root.dom, "target/ui-snapshot.html")`. All of it: `j3c doc testing`.
- Put a `.ttf` at `assets/font.ttf` (theme.css loads it); text still lays out without one.
