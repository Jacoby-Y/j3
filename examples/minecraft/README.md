# Minecraft material demo

Run from `j3c` with `cargo run -- run ../examples/minecraft`. The example needs
SDL2, SDL2_image, SDL2_ttf, and OpenGL 3.3. Its UI font is the repository's
DejaVu Sans copy in `examples/dungeon/assets`.

| Control | Action |
| --- | --- |
| WASD, mouse, Space | Walk, look, jump |
| Left / right mouse | Break / place a block |
| 1–9 or wheel | Select a block; 8 is glass, 9 is metal |
| Esc | Release/capture the mouse and open/close render settings |
| O, F3, H, Q | Toggle corner shadows, profiler overlay, hide HUD, quit |

The 48 × 32 × 48 valley uses layered value noise for hills, surface bands,
tree height and placement, and underground copper. A small material pavilion
near the starting point shows glass beside polished metal. The block atlas is
generated locally with `python3 make_atlas.py`; it uses only the Python standard
library and is available under CC0. Its RGB values are stored as linear
albedo so the final post pass applies output gamma only once.

Opaque blocks use a textured `j3.gfx` mesh with vertex ambient occlusion and
the directional shadow map. The example's post pass traces glass and metal
boxes against that mesh's depth buffer. Reflected and refracted rays then trace
the *edited voxel grid* in a 3D texture, including blocks outside the camera
view. The same shadow map lights those secondary hits and the ray-marched
sunshafts. Glass uses two surface refractions, Fresnel reflection, and tinted
absorption. Metal uses a reflective steel response with adjustable roughness.
The render settings panel exposes these controls.

This is a small real-time hybrid renderer, with one traced secondary surface
hit. It does not simulate many-bounce global illumination or focused glass
caustics. Glass and metal placements are limited to 64 simultaneous blocks to
keep the GL 3.3 uniform-array path fast; the generated showcase uses fewer.
