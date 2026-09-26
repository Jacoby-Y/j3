# Procedural canvas example

From the repository root, run `j3c run examples/canvas` to generate `lantern.png` and `stone.png` in this directory. This works without SDL. The source in `src/art.j3` builds the two images with shapes, gradients, and seeded noise.

Then run `j3c run examples/canvas_preview` to display them. The preview also creates a texture from a canvas in memory and updates one rectangle. It needs SDL2 and SDL2_image; the preview manifest searches the repository's vendored SDL2_image library.

From `j3c/`, run `cargo run -- run examples/canvas_verify.j3` for pixel, alpha, clipping, smooth edge, blit, and seed checks. Run `cargo run -- run --opt 2 examples/canvas_bench.j3` for repeatable CPU timings of fills, circles, noise, and PNG export. The benchmark writes `j3c/target/canvas_bench.png`.

Use `j3c doc j3.gfx.canvas` for all canvas methods. `Mode.Pixel` preserves exact edges and uses nearest texture filtering; `Mode.Smooth` antialiases shapes and uses linear filtering. Keep image arenas alive while drawing or exporting. A texture owns its uploaded pixels, so the canvas memory can be released after upload.
