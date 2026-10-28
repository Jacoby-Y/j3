`j3c editor --new mygame` writes these files and opens the editor; writing them yourself
gives the same project without a window. `j3c new mygame --template fps` is this with a
first-person walker. Needs SDL2 and GL.

- `boot`: `scene3d.load(path, mem)` reads the scene into the app arena, where it lives until
  quit. `scripts.scene()` is the scene the editor's Play picked, else j3.editor.toml's.
- `startup` runs once GL exists (`app.useGl()` asks for it): `gfx.init()`, then
  `scene3d.upload(&scene)` sends meshes and textures to the GPU, then `scripts.start`.
- Every frame: `scripts.update` in an update system. In draw, in this order:
  `environment`, `shadows`, `lights`, `beginEffects`, the camera pass, `scene3d.draw`,
  `endEffects`. Quitting frees GL through `game.destroy()`.
- `j3.scene3d.scripts` is made from the scenes' script lines: `j3c doc j3.scene3d.scripts`
  in the project shows it. Edit scenes with `j3c -e scene` (`j3c doc editor`), and look
  with `j3c -e render`.
