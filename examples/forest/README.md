# Autumn forest

A 3D J3 scene for testing an asset-aware URP pass. Run it from `j3c/`:

```sh
cargo run -- run ../examples/forest
```

Requires SDL2, SDL2_image, SDL2_ttf, and OpenGL 3.3. The image and font
libraries are provided through `../dungeon/lib` by `j3.toml`.

Drag outside the HUD to orbit, scroll to zoom, press **H** to hide the HUD,
and **Esc** to quit. The HUD adjusts sun height, volumetric light, exposure,
glass index of refraction, sphere size, and metal roughness.

## What the render path tests

The broadleaf tree's trunk and branch geometry is baked from a real glTF model.
Compact alpha-cutout leaf cards are placed at positions sampled from the
model's foliage geometry. The same canopy shape is rendered into a 2048 px
directional shadow map and the camera color/depth buffers. The forest URP pass
marches camera rays through that shadow map, scattering warm light in visible
shafts beneath the canopy. It also intersects two analytic spheres against the
mesh depth buffer. Refracted and reflected rays march through the same shadowed
air, so the shafts appear inside both spheres. Scene samples on those paths
also include sphere shadows. Glass refracts the live mesh scene; metal blends
screen-space hits with the forest environment. The roughness slider selects
blurrier environment mip levels.

This is a real-time hybrid demo. Its glass refraction is screen-space, so
surfaces outside the camera view cannot appear behind the sphere. The metal
uses the live camera image where a reflected ray finds visible mesh geometry
and the forest environment elsewhere. Each volumetric ray has a fixed 20-step
budget.
Glass dims the shafts it crosses; focused caustics are outside this pass.

## Public assets

The runtime assets in `assets/` are derived from these **CC0** Poly Haven
sources. Poly Haven's [license](https://polyhaven.com/license) permits including
and modifying the assets; attribution is not required, but the sources are
listed here for reproducibility.

| Runtime files | Source | Use |
| --- | --- | --- |
| `trunk.bin`, `branch.bin`, `leaves.bin`, `trunk.png`, `branch.png`, `leaves.png` | [Tree Small 02](https://polyhaven.com/a/tree_small_02), by Rico Cilliers | 3D tree, bark, alpha-cutout autumn foliage |
| `ground.png` | [Leaves Forest Ground](https://polyhaven.com/a/leaves_forest_ground), by Dario Barresi and Dimitrios Savva | Photographic leaf litter |
| `autumn_forest.jpg` | [Autumn Forest 01](https://polyhaven.com/a/autumn_forest_01), by Andreas Mischok | Distant forest and sphere environment |

`make_assets.py` converts the original 1K glTF and image downloads into the
checked-in runtime data. Give it a directory containing
`tree_small_02_1k.gltf`, `tree_small_02.bin`, `leaf.jpg`, `alpha.png`,
`bark.jpg`, `trunk.jpg`, `ground.jpg`, and `env.jpg`. The file links are
available from the [Poly Haven API](https://api.polyhaven.com) under
`/files/tree_small_02`, `/files/leaves_forest_ground`, and
`/files/autumn_forest_01`. The original 95 MB model buffer is only needed to
regenerate assets; the demo loads the compact files in this directory.
