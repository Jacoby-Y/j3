# Voxel editor feedback

## Furnace authoring trial

Created `assets/models/furnace.j3voxel` through `vox run --script`, matching the existing furnace recipe. Verified all 3,732 occupied cells and rounded palette colors; inspected four-view and rear-port renders. Canvas is 24 × 32 × 24, with the existing local X/Z origin shifted by +12. Game integration has not changed.

### Fixed and rechecked: recoloring through Add loses undo

From the industry project root:

```sh
../../tools/voxel-editor/vox run assets/models/furnace.j3voxel \
  "box 4 0 4 4 0 4 brick; undo" --no-save
```

The corner starts as iron. The box replaces its color with brick without changing occupied-cell count. The tool reports `note: no voxel was added`, then `error: nothing to undo` and exits 1. The saved furnace remains untouched because this reproduction uses `--no-save`.

In `tools/voxel-editor/src/cmd.j3`, the successful Add-command branch compares occupied-cell counts and calls `model.discardSnapshot` when they match. Count equality does not imply unchanged cell colors. The original furnace batch produced 13 such notices despite correctly applying the palette replacements.

Suggested improvement: detect actual cell changes, preserve undo for recoloring, and distinguish recoloring from a true no-op in the diagnostic. Add a regression check that changing the color of an occupied voxel with Add can be undone and redone.

The upstream fix was rechecked during migration: Add recoloring followed by undo, redo, and undo succeeds with `--no-save`. The palette-token fix is also present upstream and covered by the industry loader tests. All 13 model files were subsequently authored or verified with the editor CLI and rendered successfully.

No editor code was changed. Native window interaction was not tested.

## Additional runtime validation gaps

The current upstream runtime loader was probed during migration and accepts palette index 257 (wraps to 1), an undefined palette reference, a negative coordinate (minus ignored, then clipped), and a 65-cell dimension. These confirmed findings and suggested regression checks are recorded in the voxel-editor README's top Issues section. Industry's local copy validates numeric tokens, dimensions, coordinates/runs, palette indices, and references before use.
