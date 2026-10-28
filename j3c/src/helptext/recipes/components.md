`.j3ui` components are Svelte-like files: markup, a `<script>` in J3, and a scoped `<style>`.
`j3c` compiles every `ui/*.j3ui` to one J3 module `<package>.ui` (`j3c dump-ui` shows it).
Needs SDL2, SDL2_ttf, and libGL. `j3c new mygame --template ui` writes this project, including a
stock `assets/font.ttf` (theme.css loads it; add your own at that path when copying by hand). Full guide: `j3c doc j3ui`.

- A capitalised file is a component: `ui/Screen.j3ui` is `Screen` plus `ScreenProps`. The one nobody
  contains is the root; the host calls `root.render(windowSize(), props)` once per frame.
- `props { state: &State; }` is what the parent passes. A top-level `let` is per-instance state.
- Handlers are statements: `on:click={state.clicks += 1}`. `{expr}` in text and attributes is re-read
  every frame.
- `<canvas on:draw={...}>` draws with plain `j3.game` / `j3.gfx.paint` calls into its own box:
  `0,0` is its corner and `size` is its size. Children of a canvas are a HUD on top.
- `ui/theme.css` is global; a component's `<style>` applies to that component only. It sets the pointer
  cursor and a focus ring on buttons (`cursor`, `outline`); centre with `margin: 0 auto` or `margin: auto`
  in a flex box, and keep `aspect-ratio`, `:nth-child()`, `:not()` and `:last-child` for lists and cards.
- Dev runs (`j3c run`) reload styles when a `.j3ui` `<style>` or theme.css changes on disk. Markup,
  script and `.j3` edits need a rebuild: `j3c run --hot` does it and restarts (`j3c doc j3ui`, Dev loop).
- Check without a window: `j3c run --frames 5 --screenshot shot.png --headless`, then read the PNG.
  Goldens, `imgdiff`, and the HTML snapshot: `j3c doc testing`. CSS support: `j3c doc css`.
