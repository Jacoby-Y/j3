## 3D scenes and the editor

Scenes are `scenes/*.j3scene` text files that `j3c editor` edits and the game loads with `j3.scene3d` (`j3c doc recipes scene3d`, `j3c doc editor`). Edit them through the tools below, not by hand: they keep ids, parents, and undo correct. Nothing here needs the editor window open.

MCP server (once): `claude mcp add -s user j3 -- j3c -e mcp`. Its startup instructions and tool descriptions say when to use each tool. Without MCP, the same operations are `j3c -e scene --help`, `j3c -e render --help`, `j3c -e gen`, and `j3c -e mcp --tools` (every tool's arguments next to its CLI twin).

Loop: change, look, check.

1. Learn the scene: `scene_list` (glob `filter`, `group_by=prefix`, `tree` for the scene file's own lines), `scene_get` (an object and its children as file lines).
2. Build with bulk tools, not many single calls: `place` (`ring`, `wall` with a door gap, `stairs`, `spiral`, `grid`, `scatter`, `ground`, `ontop`), `scene_add_many`, `scene_apply` (many varied objects, or a subtree you copied from `scene_get`, written as scene lines), and globs in `scene_set` / `scene_delete` (`DecoTree*`). `scene_split` / `scene_inline` move objects into their own scene file and back. Meshes built in code: `assets/gen/*.j3` (`j3c doc recipes meshgen`).
3. Look from where the player stands: `render` with `at` / `yaw` / `pitch`, `views` for several angles in one image, `hide` (`Roof*`) to see inside a building, `view=top` with `ortho` for a plan. Render after every few edits, not once at the end.
4. Verify: `check` (scene problems, this project's own rules, and code), `scene_stats` (draw cost: above 60 distinct looks is slow, so snap tints to a few values), `scene_diff` (review your own edits against git HEAD), `play` (runs the game headless and returns a frame).

Facts that cost time:

- A scene file is one line per object (`Name kind key=value #id`, indent for children, defaults left out) and rotations are degrees there and in the tools; `j3c doc editor` has the format. Edit through the tools; after a hand edit run `j3c -e scene fmt`. `check` warns on a rotation over two turns. Yaw 0 looks toward -z, 90 toward -x. A slab pointing along `(dx, dz)` needs `rot` y `= -atan2(dz, dx)` degrees.
- With `j3c editor` open, tool edits are live undo steps there and unsaved until the person saves. With it closed they write the file and `undo` restores it (20 kept). Every reply says which.
- Give objects `tags` (`scene_set tags=nocollide,launch=30`) and read them with `scene3d.hasTag` / `tagValue`, instead of matching name prefixes.
- Put this project's rules in a check script so a mistake names the object instead of showing up in play: `check = "tools/check.j3s"` under `[editor]` in `j3.editor.toml`. It runs as `j3c run tools/check.j3s <scene>` and prints `error: ...` or `warning: ...` lines (read the scene with `scene3d.read`). Project-specific notes for agents go in a file named by `notes = "AGENT_NOTES.md"` there; the MCP server adds it to its instructions.
