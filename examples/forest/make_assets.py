"""Bake Poly Haven's tree_small_02 glTF into compact J3 vertex buffers.

Run with a directory containing tree_small_02_1k.gltf, tree_small_02.bin,
leaf.jpg, alpha.png, bark.jpg, trunk.jpg, ground.jpg. Original URLs and
licenses are recorded in README.md. The runtime only needs assets/*.bin/png.
"""

import json
import math
import random
import struct
import sys
from pathlib import Path

from PIL import Image


SOURCE = Path(sys.argv[1] if len(sys.argv) > 1 else "/tmp/j3_forest_asset")
OUT = Path(__file__).parent / "assets"
OUT.mkdir(exist_ok=True)
gltf = json.loads((SOURCE / "tree_small_02_1k.gltf").read_text())
buf = (SOURCE / "tree_small_02.bin").read_bytes()


def item(accessor, index):
    desc = gltf["accessors"][accessor]
    view = gltf["bufferViews"][desc["bufferView"]]
    count = {"SCALAR": 1, "VEC2": 2, "VEC3": 3}[desc["type"]]
    code = {5123: "H", 5125: "I", 5126: "f"}[desc["componentType"]]
    size = struct.calcsize(code) * count
    offset = view.get("byteOffset", 0) + desc.get("byteOffset", 0)
    offset += index * view.get("byteStride", size)
    return struct.unpack_from("<" + code * count, buf, offset)


def bake_mesh(primitive, name):
    attributes = primitive["attributes"]
    indices = primitive["indices"]
    total = gltf["accessors"][indices]["count"]
    material = primitive["material"]
    group = 3
    with (OUT / name).open("wb") as file:
        vertices = 0
        # Every indexed triangle must be copied. Skipping triangles makes
        # literal holes in the trunk and branches, not a usable LOD.
        for start in range(0, total - group + 1, group):
            ids = [item(indices, start + j)[0] for j in range(group)]
            positions = [item(attributes["POSITION"], idx) for idx in ids]
            for idx, position in zip(ids, positions):
                normal = item(attributes["NORMAL"], idx)
                uv_accessor = attributes["TEXCOORD_1"] if material == 0 else attributes["TEXCOORD_0"]
                uv = item(uv_accessor, idx)
                if material == 0:
                    uv = (uv[0] * 3.0, 0.4 + uv[1] * 0.6)
                file.write(struct.pack("<12f", *position, *normal, 1.0, 1.0, 1.0, 0.0, *uv))
                vertices += 1
    print(name, vertices // 3, "triangles")


primitives = gltf["meshes"][0]["primitives"]
bake_mesh(primitives[0], "branch.bin")
bake_mesh(primitives[2], "trunk.bin")

# The full glTF foliage is almost two million triangles. Put compact,
# two-sided leaf cards at positions sampled from those real mesh vertices.
# This preserves its asymmetric crown and branch distribution at game scale.
leaf_positions = primitives[1]["attributes"]["POSITION"]
leaf_count = gltf["accessors"][leaf_positions]["count"]
rng = random.Random(433)
with (OUT / "leaves.bin").open("wb") as file:
    for card in range(9200):
        p = item(leaf_positions, rng.randrange(leaf_count))
        theta = rng.random() * math.tau
        c, s = math.cos(theta), math.sin(theta)
        width = rng.uniform(0.15, 0.28)
        height = width * rng.uniform(0.57, 0.82)
        u = (c * width, 0.0, s * width)
        v = (s * height * 0.52, height * 0.85, -c * height * 0.52)
        normal = (-s * 0.76, 0.52, c * 0.76)
        tone = rng.uniform(0.72, 1.08)
        tint = (tone, tone * rng.uniform(0.64, 1.12), tone * rng.uniform(0.49, 0.86))
        corners = []
        for a, b in ((-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)):
            corners.append(tuple(p[k] + a * u[k] + b * v[k] for k in range(3)))
        for corner, uv in ((0, (0, 0)), (1, (1, 0)), (2, (1, 1)),
                           (0, (0, 0)), (2, (1, 1)), (3, (0, 1))):
            file.write(struct.pack("<12f", *corners[corner], *normal, *tint, 0.0, *uv))
print("leaves.bin", 9200 * 2, "triangles")


def rgba(source, name):
    Image.open(SOURCE / source).convert("RGBA").save(OUT / name, optimize=True)


rgba("bark.jpg", "branch.png")
rgba("trunk.jpg", "trunk.png")
rgba("ground.jpg", "ground.png")

leaf = Image.open(SOURCE / "leaf.jpg").convert("RGB").crop((555, 434, 709, 539))
mask = Image.open(SOURCE / "alpha.png").convert("L").crop((555, 434, 709, 539))
pixels = leaf.load()
alpha = mask.load()
colored = Image.new("RGBA", leaf.size)
dest = colored.load()
for y in range(leaf.height):
    for x in range(leaf.width):
        r, g, b = pixels[x, y]
        # Shift photographed green foliage into amber/orange while preserving veins.
        luminosity = 0.45 * r + 0.5 * g + 0.05 * b
        variation = 0.5 + 0.5 * math.sin(x * 0.063 + y * 0.037)
        dest[x, y] = (
            min(255, int(luminosity * (1.62 + 0.26 * variation))),
            min(255, int(luminosity * (0.84 + 0.35 * variation))),
            min(255, int(luminosity * (0.18 + 0.16 * variation))),
            alpha[x, y],
        )
colored.save(OUT / "leaves.png", optimize=True)

Image.open(SOURCE / "env.jpg").convert("RGB").resize((2048, 1024), Image.Resampling.LANCZOS).save(
    OUT / "autumn_forest.jpg", quality=88, optimize=True
)
