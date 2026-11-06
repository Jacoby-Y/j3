# Voxel editor (agents)

Make, look at, edit, rig, animate, dress, and export voxel models without the window: read `README.md`
(sections **Agents**, **Rigging**, **Overlays**). The short version:

```
tools/voxel-editor/vox run models/NAME.j3voxel "COMMANDS; ..." --render /tmp/NAME.png --view all
tools/voxel-editor/vox run models/NAME.j3voxel --script models/NAME.vox.txt --render /tmp/NAME.png
```

then read the PNG, check layers with `"dump y N"`, and repeat. `vox run X "help"` lists every command
(file, draw, edit, palette, rig, overlay, look). A failed command stops the batch and saves nothing; the
error says what to fix. Run `tools/voxel-editor/vox selftest` after changing `src/`; it prints `ok`.
Models are `.j3voxel` text (see `README.md`: `size`, `p`, `v`; rigs add `part`, `vp`, `pose`, `clip`;
overlays add `ov`, `ot`), `runtime/j3voxel.j3` loads a plain one in a game, and `runtime/j3rig.j3` loads a
rigged one (parts, poses, clips, overlays).

**Rigging a character:** build the body part by part (`part new NAME parent P`, draw while it is active,
`part pivot-auto NAME`), or draw it all and `rig humanoid`; look with `rig tree`, `rig check`, and
`--parts` renders; set `part limit` on elbows and knees; `pose set`, `pose save`, `clip new`; render with
`pose load NAME` or `clip at NAME T` first. **Dressing it:** `overlay new NAME PART grow N` (the model needs
N voxels of margin around the part: the error says which `size`/`shift`), `overlay box ... all COLOR`,
`overlay erase`, or `overlay export` / `overlay import` a PNG net. Worked examples, run with `--script`:
`models/blocky.vox.txt` (rig, poses, clip), `models/blocky-armor.vox.txt` and `models/blocky-knight.vox.txt`
(overlays, PNG). Characters face +z; `.L` is +x.

Traps: `;` splits commands but a `#` comment ends the line (so a script comment may contain one); tools
and picking act on the unposed model, so `pose reset` before painting in the window; a `.j3voxel` made by
hand needs the `size` line first; `export-mesh` is a static mesh (no parts, poses, overlays).

Code notes: one module per file under `src/` (big ones have part files in a folder beside them: `src/cmd/`, `src/model/`, ...), every edit goes through `model.apply` (so mirrored
editing works for every tool), and `cmd.j3` is the single command language the CLI, the window's
console, and agents share: add a feature there first (and its line in `helpText`, `editing()` and the
README). Rig and overlay logic is in `src/rig.j3` and `src/rig/`, `src/overlay.j3`, `src/overlaypng.j3`. A name can be
declared once per function. Do not launch the window to check feel; the person running it does that.
