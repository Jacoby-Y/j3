# J3 URP renderers

The optional `j3.gfx.urp` modules use OpenGL 3.3 and leave the normal `j3.game`
2D renderer and `j3.gfx` mesh renderer opt-in. These are project-specific
render passes, not a general material or scene graph API. Set `App.useGl()`
before startup and link `GL`; SDL creates the GL context. Both passes compile
GLSL at startup and register cleanup to run before SDL deletes that context.

`j3c doc j3.gfx.urp.raytrace` and `j3c doc j3.gfx.urp.forest` show the public source
API. Example projects: `examples/raytrace`, `examples/ocean`, and
`examples/forest`.

## Raytraced room and ocean

`j3.gfx.urp.raytrace` is a fullscreen fragment-shader renderer. Importing it does
not change `j3.gfx` or open a GL context. Call `raytrace.init()` during startup
after `App.useGl()` has created the context. Each frame call
`raytrace.render(&scene, camera, quality)`, then draw the UI. It renders to a
scaled offscreen color target, presents to the window, and resizes that target
when the window changes size. The renderer registers its cleanup with
`j3.game`; `game.destroy()` runs it before deleting the GL context. Call
`raytrace.shutdown()` yourself only when you are managing the GL lifetime.

`Scene` holds room dimensions, a slice of `Sphere`, and a slice of `AreaLight`.
The room is open on its positive Z side. The renderer supports up to 16
spheres and 4 square ceiling lights; exceeding those limits reports an error.
Materials come from `Material.plastic`, `.metal`, and `.glass`. Colors use linear
RGB values. Glass tint absorbs light over the distance traveled inside it;
Fresnel reflection reduces transmission at each surface.

`Quality.low()`, `.medium()`, and `.high()` provide starting settings. Their
fields can be changed independently:

| Setting | Range | Effect |
| --- | --- | --- |
| `scale` | 0.25–1 | Fraction of window resolution traced before upscaling |
| `bounces` | 1–6 | Maximum surface crossings per path |
| `lightSamples` | 1–8 | Direct illumination samples per area light; sphere shadow edges stay smooth at every setting |
| `aaSamples` | 1–4 | Rays per pixel |

The built-in shader traces spheres and the five room surfaces. It estimates
shadows from the continuous overlap between a sphere and each square area light,
approximating the square with an equal-area disk. Glass bends the view and
transmits tinted light in this area visibility estimate. The shadow estimate
does not simulate focused caustics or exact refracted paths. The scene and
quality values are supplied independently from GL state.

Run the demo from the repository root with
`cd j3c && cargo run -- run ../examples/raytrace`. Press **H** to hide the HUD, drag outside it to orbit,
and scroll outside it to zoom.

`initOcean(source)` installs one alternate fragment shader after `init`; it
does not replace the built-in room shader. Pass its `Ocean` parameters to
`renderOcean(&scene, camera, quality, ocean)` each frame. The shader source
must use the fullscreen vertex interface and the uniforms supplied by this
module. See `examples/ocean/assets/ocean.frag` for the working GLSL contract,
and `examples/ocean/README.md` for its wave model and controls.

The fullscreen renderer changes GL state while drawing and expects to own the
scene pass for that frame. Draw screen-space UI after `render` or
`renderOcean`. `Quality` clamps scale to 0.25–1; bounces, light samples, and
antialiasing samples must stay within the ranges above. Cost rises quickly as
render scale and sample counts increase.

## Mesh-aware forest composite

`j3.gfx.urp.forest` is a mesh-aware composite pass for `j3.gfx`; it does not create
or initialize the mesh renderer. In startup, call `gfx.init()`, then
`forest.init(fragmentSource)` with the fragment shader source. Each frame:

1. Call `gfx.shadowPass` and draw shadow-casting geometry.
2. Call `forest.begin()` before `gfx.cameraPass`. This assigns a private,
   window-sized framebuffer to the mesh camera pass; `gfx` renders color and
   depth into it.
3. Draw the camera-visible meshes, then call `forest.present(...)` with the
   same camera and sun, material controls, elapsed time, and an environment
   texture.
4. Draw screen-space UI after `present`.

`present` uses the most recent `gfx` shadow texture and light matrix, and
composites mesh color and depth with volumetric sunlight plus two analytic
spheres. It expects a completed shadow pass and camera pass each frame. The
shader is supplied by the project because its scene-specific lighting model
is not built into `j3.gfx.urp.forest`; `examples/forest/assets/forest.frag` is the
reference shader. The composite uses screen-space scene hits, so glass cannot
show geometry outside the camera image. Metal falls back to the environment
texture when a reflected ray misses visible scene geometry. See
`examples/forest/README.md` for rendering limits, controls, and public asset
sources.

The framebuffer is resized by `begin()` when the window size changes. The
forest shutdown callback also shuts down `j3.gfx`; initialize both modules as
shown and let `game.destroy()` run registered GL cleanup while the context is
still alive.
