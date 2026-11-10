# Desk app

A small desktop-style app in `.j3ui`: sidebar nav, a keyed sortable and filterable file table, a details pane, a rename modal, toasts and a settings page with saved preferences. It is the proof app of plan 023 (`plans/023-j3ui-app-readiness.md`, stage 7): copy its `ui/kit/` and its test hooks into your own project. Learn J3 from `j3c` only (`j3c doc j3ui`, `j3c doc css`, `j3c doc testing`); never open a window (`SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy`, `timeout`).

## Map

- `src/state.j3`: all data and rules, no drawing. Fake files (deterministic), `view()` (filter then stable sort, rebuilt only when an input changed), `sortBy`, rename rules (`problem`, `commitRename`), toasts, preferences fields. A file id is its index and never changes: it is the row key.
- `src/prefs.j3`: `load`/`store`/`signature` over `j3.data.save`. Every value is clamped; a missing or garbage file gives the defaults. `main.j3` writes the file 20 idle frames after the last change (a quit inside that window loses it: `App` has no shutdown hook).
- `src/main.j3`: boot, `persist`, test hooks (env): `DESK_GOLDEN=<browser|filter|rename|settings|toast>` fixed state and no prefs file; `DESK_PREFS=path`; `DESK_TEST=1` fake-input run (`src/selftest.j3`, PASS/FAIL lines); `DESK_SNAPSHOT=N` HTML snapshot on frame N.
- `ui/Shell.j3ui` root (`provide state`, `{#match}` page switch, nav, modal, toasts, `onFrame`), `ui/Details.j3ui`, `ui/RenameModal.j3ui` (payload event `rename(name)`), `ui/pages/Browser.j3ui`, `ui/pages/Settings.j3ui`.
- `ui/kit/`: slot-based `Card` (title, `actions`, `footer`), `Section`, `Field` (`hint` slot), `Toolbar` (`end` slot), `Table` (`head`, `empty`, rows), `Th` (sort header, `sort` event). Columns are styled by the page, because head and rows are the page's markup.

## Check

`j3c golden examples/desk-app` (`tools/golden.sh` is a one-line wrapper) builds, compares the states in `golden.toml` (`golden/*.png`, exact), then runs the steady check (`J3_STEADY=60:120`) and the self test (`j3.ui.testkit`); exit 0 means all passed. `j3c golden examples/desk-app --bless` rewrites the PNGs: look at them first. Run it twice after a UI change: a second run that differs means a golden is not deterministic.

## Rules that bit

- A kit component takes `class` / `style:` on its single root, so size it from outside; a component with several roots cannot (compile error).
- `emit` works in a handler and in a script `fn` (call that fn from a handler, not from markup).
- A function that returns a string allocates; do not call one per row per frame. Build row text once (`relabel`) and read the field.
- A bound text box does not take `End Backspace` as one `simKeys`: send keys on separate frames.
- Clicking a row moves focus off the filter box; click it again before typing.
