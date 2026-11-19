In an editor project, each `assets/gen/<name>.j3` is a small program that builds a mesh
and saves `assets/gen/<name>.j3mesh`. `j3c -e gen` runs the scripts that changed, and
render, play, and build run it too. Use the result as a mesh asset:
`j3c -e scene add mesh name=Tree asset=assets/gen/tree.j3mesh`.

- A script reruns when it or j3c changed. It does not see changes in other project files
  it imports; `j3c -e gen --force` reruns everything.
- Coordinates are meters, y up; `push`/`pop` save the transform, `color` sets vertex color.
- Next: `j3c doc j3.gfx.meshgen --short`.
