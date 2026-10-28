A headless program: no SDL, no window. `j3c run` writes coin.png in the project root
(a project runs with its root as the working directory).

- `Mode.Pixel` keeps hard pixel edges; `Mode.Smooth` antialiases circles, lines, and
  triangles. Canvas colors (`canvas.rgb`) are image pixels, not `j3.game` screen colors.
- The canvas lives in the arena you pass; keep the arena until you are done with it.
- In a game, `game.textureFromCanvas(c)` uploads once; the texture keeps its pixels after
  the arena goes. `updateTextureFromCanvas` re-uploads a changed rectangle. Destroy
  textures when done.
- Next: `j3c doc j3.gfx.canvas --short` (shapes, gradients, noise, blending).
