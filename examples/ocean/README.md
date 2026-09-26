# Ocean raytracing example

Run from the repository root:

```sh
cd j3c
cargo run -- run ../examples/ocean
```

The scene uses the optional `j3.gfx.urp.raytrace` GL 3.3 pipeline. The fragment shader
in `assets/ocean.frag` evaluates a directional, animated wave spectrum with
deep-water dispersion and sharpened crests. It intersects rays against the same
height field used for shading, so the glass sphere refracts the waves and the
metal and water reflect one another. The water uses Fresnel reflection,
refraction into a tinted body with depth absorption, sun shadows from the
spheres, and foam on steeper crests. Sky and clouds are procedural too.

Drag outside the panel to orbit, use the wheel to zoom, press **H** to hide the
panel, **Space** to pause the waves, and **Esc** to quit. The panel adjusts wind,
swell, crest shape, sun height, sea color, sphere materials, and tracing quality.

The ocean is a height field, so it does not model breaking or overturning waves.
The refracted underwater view uses a simple colored seabed and absorption model;
it does not trace underwater objects or compute focused caustics. Higher render
scale and antialiasing cost substantially more GPU time because each pixel may
intersect the wave field several times.
