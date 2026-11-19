In an editor scene URP is a setting, and `scene3d.draw` traces it; the game needs no code:

    j3c -e scene set @scene urp=true urpBounces=2 urpShadows=true
    j3c -e render

Only surfaces with a mirror (`reflect`) or glass (`ior`) material trace rays; the rest draw
as usual. Traces use material colors, not textures, and ignore `.j3fx` effects. Moving
objects are cheap: each mesh keeps its own tree, and a frame rebuilds only the tree over
object boxes. Settings: `j3c doc editor`, URP.

Outside scene3d, `j3.gfx.urp.raytrace` (spheres in a room) and `j3.gfx.urp.forest` (a
composite over `j3.gfx` meshes) are renderers you drive yourself; `j3c doc urp` has their
pass order and GL lifetime.
