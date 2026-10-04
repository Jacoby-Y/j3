# Voxel editor (agents)

Make, look at, edit, and export voxel models without the window: read `README.md`, section
**Agents**. The short version:

```
tools/voxel-editor/vox run models/NAME.j3voxel "COMMANDS; ..." --render /tmp/NAME.png --view all
```

then read the PNG, check layers with `"dump y N"`, and repeat. `vox run X "help"` lists commands.
Run `tools/voxel-editor/vox selftest` after changing `src/`; it prints `ok`. Models are `.j3voxel`
text (`size`, `p` palette lines, `v` voxel lines), and `runtime/j3voxel.j3` loads one in a game.

Code notes: one module per file under `src/`, every edit goes through `model.apply` (so mirrored
editing works for every tool), and `cmd.j3` is the single command language the CLI, the window's
console, and agents share: add a feature there first. A name can be declared once per function.
Do not launch the window to check feel; the person running it does that.
