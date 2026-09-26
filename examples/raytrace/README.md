# Room raytracing

Run from `j3c/`:

```sh
cargo run -- run ../examples/raytrace
```

The sample uses `j3.gfx.urp.raytrace`, an optional OpenGL 3.3 fullscreen renderer.
It traces a room with square ceiling lights and up to 16 spheres, including
plastic, metal, and tinted glass. It uses one sampleable color texture on an
offscreen framebuffer, then draws that image to the window. The implementation
and public API are documented by `j3c doc j3.gfx.urp.raytrace`.

Press **H** to hide the controls, drag outside the panel to orbit, scroll to
zoom, and press **Esc** to quit. The panel changes render scale, bounce count,
area-light samples, antialiasing, and the sphere materials.

The renderer approximates each square light's sphere visibility with an
equal-area disk. It supports refracted and reflected paths through spheres,
but does not compute focused caustics or exact refracted area-light shadows.
Increasing render scale, bounces, light samples, or antialiasing raises GPU
cost.
