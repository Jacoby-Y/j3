#!/usr/bin/env python3
"""Turn a glTF 2.0 JSON file plus its .bin into `# j3mesh 2`.

Prints `diffuse: <png>` for the first base-color image. Embedded images are
written next to the mesh. Vertex colors stay white when the file has none.
A length that is also a multiple of 10 gets one degenerate triangle so
`gfx.upload` sees a 12-float stride.

Usage: python3 tools/j3mesh_import.py model.gltf model.j3mesh
"""

import base64
import json
import os
import struct
import sys

COMP = {
    5120: ("b", 1),
    5121: ("B", 1),
    5122: ("<h", 2),
    5123: ("<H", 2),
    5125: ("<I", 4),
    5126: ("<f", 4),
}
NCOMP = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4}
UNSIGNED_MAX = {5120: 127, 5121: 255, 5122: 32767, 5123: 65535, 5125: 4294967295}


def die(msg):
    print(msg, file=sys.stderr)
    sys.exit(1)


def ident4():
    return [
        [1, 0, 0, 0],
        [0, 1, 0, 0],
        [0, 0, 1, 0],
        [0, 0, 0, 1],
    ]


def mul4(a, b):
    out = [[0, 0, 0, 0] for _ in range(4)]
    for c in range(4):
        for r in range(4):
            out[c][r] = (
                a[0][r] * b[c][0]
                + a[1][r] * b[c][1]
                + a[2][r] * b[c][2]
                + a[3][r] * b[c][3]
            )
    return out


def apply4(m, x, y, z, w):
    return (
        m[0][0] * x + m[1][0] * y + m[2][0] * z + m[3][0] * w,
        m[0][1] * x + m[1][1] * y + m[2][1] * z + m[3][1] * w,
        m[0][2] * x + m[1][2] * y + m[2][2] * z + m[3][2] * w,
    )


def quat_matrix(x, y, z, w):
    return [
        [1 - 2 * (y * y + z * z), 2 * (x * y + z * w), 2 * (x * z - y * w), 0],
        [2 * (x * y - z * w), 1 - 2 * (x * x + z * z), 2 * (y * z + x * w), 0],
        [2 * (x * z + y * w), 2 * (y * z - x * w), 1 - 2 * (x * x + y * y), 0],
        [0, 0, 0, 1],
    ]


def node_matrix(node):
    if "matrix" in node:
        m = node["matrix"]
        if len(m) != 16:
            die("node matrix")
        return [[m[c * 4 + r] for r in range(4)] for c in range(4)]
    t = node.get("translation", [0, 0, 0])
    r = node.get("rotation", [0, 0, 0, 1])
    s = node.get("scale", [1, 1, 1])
    rot = quat_matrix(r[0], r[1], r[2], r[3])
    scale = [
        [s[0], 0, 0, 0],
        [0, s[1], 0, 0],
        [0, 0, s[2], 0],
        [0, 0, 0, 1],
    ]
    trans = [
        [1, 0, 0, 0],
        [0, 1, 0, 0],
        [0, 0, 1, 0],
        [t[0], t[1], t[2], 1],
    ]
    return mul4(trans, mul4(rot, scale))


def invert3(m):
    a, b, c = m[0]
    d, e, f = m[1]
    g, h, i = m[2]
    det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g)
    if abs(det) < 1e-12:
        return None
    inv = 1.0 / det
    return [
        [(e * i - f * h) * inv, (c * h - b * i) * inv, (b * f - c * e) * inv],
        [(f * g - d * i) * inv, (a * i - c * g) * inv, (c * d - a * f) * inv],
        [(d * h - e * g) * inv, (b * g - a * h) * inv, (a * e - b * d) * inv],
    ]


def normal_matrix(world):
    linear = [
        [world[0][0], world[1][0], world[2][0]],
        [world[0][1], world[1][1], world[2][1]],
        [world[0][2], world[1][2], world[2][2]],
    ]
    inv = invert3(linear)
    if inv is None:
        return [[1, 0, 0], [0, 1, 0], [0, 0, 1]]
    return inv


def transform_normal(nmat, x, y, z):
    ox = nmat[0][0] * x + nmat[1][0] * y + nmat[2][0] * z
    oy = nmat[0][1] * x + nmat[1][1] * y + nmat[2][1] * z
    oz = nmat[0][2] * x + nmat[1][2] * y + nmat[2][2] * z
    length = (ox * ox + oy * oy + oz * oz) ** 0.5
    if length < 1e-12:
        return 0.0, 1.0, 0.0
    return ox / length, oy / length, oz / length


def load_buffers(gltf, base):
    out = []
    for buf in gltf.get("buffers", []):
        uri = buf.get("uri", "")
        if uri.startswith("data:"):
            comma = uri.find(",")
            if comma < 0:
                die("buffer uri")
            out.append(base64.b64decode(uri[comma + 1 :]))
        elif uri == "":
            die("buffer has no uri")
        else:
            path = os.path.join(base, uri)
            with open(path, "rb") as f:
                out.append(f.read())
    return out


def accessor_items(gltf, buffers, index):
    acc = gltf["accessors"][index]
    if "bufferView" not in acc:
        die("sparse accessor")
    view = gltf["bufferViews"][acc["bufferView"]]
    blob = buffers[view["buffer"]]
    ctype = acc["componentType"]
    if ctype not in COMP:
        die("component type")
    fmt, size = COMP[ctype]
    ncomp = NCOMP[acc["type"]]
    stride = view.get("byteStride", size * ncomp)
    start = view.get("byteOffset", 0) + acc.get("byteOffset", 0)
    count = acc["count"]
    norm = acc.get("normalized", False)
    rows = []
    for i in range(count):
        off = start + i * stride
        vals = []
        for k in range(ncomp):
            raw = struct.unpack_from(fmt, blob, off + k * size)[0]
            if norm and ctype != 5126:
                raw = float(raw) / float(UNSIGNED_MAX[ctype])
            else:
                raw = float(raw)
            vals.append(raw)
        rows.append(vals)
    return rows


def triangles(gltf, buffers, prim):
    mode = prim.get("mode", 4)
    if mode != 4:
        die("only triangle primitives")
    attrs = prim["attributes"]
    if "POSITION" not in attrs:
        die("POSITION")
    pos = accessor_items(gltf, buffers, attrs["POSITION"])
    nrm = None
    if "NORMAL" in attrs:
        nrm = accessor_items(gltf, buffers, attrs["NORMAL"])
    uv = None
    if "TEXCOORD_0" in attrs:
        uv = accessor_items(gltf, buffers, attrs["TEXCOORD_0"])
    col = None
    if "COLOR_0" in attrs:
        col = accessor_items(gltf, buffers, attrs["COLOR_0"])
    if "indices" in prim:
        idx = accessor_items(gltf, buffers, prim["indices"])
        ids = [int(row[0]) for row in idx]
    else:
        ids = list(range(len(pos)))
    if len(ids) % 3 != 0:
        die("index count")
    faces = []
    for t in range(0, len(ids), 3):
        tri = []
        for k in range(3):
            i = ids[t + k]
            p = pos[i]
            if nrm is None:
                n = [0.0, 1.0, 0.0]
            else:
                n = nrm[i]
            if uv is None:
                tcoord = [0.0, 0.0]
            else:
                tcoord = uv[i]
            if col is None:
                c = [1.0, 1.0, 1.0]
            else:
                c = col[i]
            tri.append((p, n, c, tcoord))
        if nrm is None:
            a, b, c = tri[0][0], tri[1][0], tri[2][0]
            ux, uy, uz = b[0] - a[0], b[1] - a[1], b[2] - a[2]
            vx, vy, vz = c[0] - a[0], c[1] - a[1], c[2] - a[2]
            nx = uy * vz - uz * vy
            ny = uz * vx - ux * vz
            nz = ux * vy - uy * vx
            length = (nx * nx + ny * ny + nz * nz) ** 0.5
            if length < 1e-12:
                face_n = (0.0, 1.0, 0.0)
            else:
                face_n = (nx / length, ny / length, nz / length)
            tri = [(p, face_n, colr, tcoord) for (p, _, colr, tcoord) in tri]
        faces.append(tri)
    return faces


def material_image(gltf, prim):
    mid = prim.get("material")
    if mid is None:
        return None
    mat = gltf["materials"][mid]
    pbr = mat.get("pbrMetallicRoughness", {})
    tex = pbr.get("baseColorTexture")
    if tex is None:
        return None
    return tex["index"]


def image_bytes(gltf, buffers, image):
    if "uri" in image:
        uri = image["uri"]
        if uri.startswith("data:"):
            comma = uri.find(",")
            if comma < 0:
                die("image uri")
            return base64.b64decode(uri[comma + 1 :]), None
        return None, uri
    if "bufferView" not in image:
        die("image")
    view = gltf["bufferViews"][image["bufferView"]]
    blob = buffers[view["buffer"]]
    off = view.get("byteOffset", 0)
    return blob[off : off + view["byteLength"]], None


def write_image(gltf, buffers, tex_index, out_path):
    tex = gltf["textures"][tex_index]
    image = gltf["images"][tex["source"]]
    data, uri = image_bytes(gltf, buffers, image)
    if uri is not None:
        return uri
    dest = os.path.splitext(out_path)[0] + ".png"
    with open(dest, "wb") as f:
        f.write(data)
    return os.path.basename(dest)


def fmt(v):
    if v < 0:
        return "-" + f"{-v:.6f}"
    return f"{v:.6f}"


def emit_vertex(world, nmat, corner):
    p, n, c, uv = corner
    x, y, z = apply4(world, p[0], p[1], p[2], 1.0)
    nx, ny, nz = transform_normal(nmat, n[0], n[1], n[2])
    r = c[0] if len(c) > 0 else 1.0
    g = c[1] if len(c) > 1 else 1.0
    b = c[2] if len(c) > 2 else 1.0
    return [x, y, z, nx, ny, nz, r, g, b, 0.0, uv[0], uv[1]]


def collect(gltf, buffers):
    verts = []
    diffuse = None
    nodes = gltf.get("nodes", [])
    scenes = gltf.get("scenes", [])
    if scenes and nodes:
        scene = gltf.get("scene", 0)
        roots = scenes[scene].get("nodes", [])

        def walk(index, parent):
            nonlocal diffuse
            node = nodes[index]
            world = mul4(parent, node_matrix(node))
            if "mesh" in node:
                nmat = normal_matrix(world)
                mesh = gltf["meshes"][node["mesh"]]
                for prim in mesh.get("primitives", []):
                    tex = material_image(gltf, prim)
                    if tex is not None and diffuse is None:
                        diffuse = tex
                    for tri in triangles(gltf, buffers, prim):
                        for corner in tri:
                            verts.append(emit_vertex(world, nmat, corner))
            for child in node.get("children", []):
                walk(child, world)

        for root in roots:
            walk(root, ident4())
    else:
        nmat = [[1, 0, 0], [0, 1, 0], [0, 0, 1]]
        world = ident4()
        for mesh in gltf.get("meshes", []):
            for prim in mesh.get("primitives", []):
                tex = material_image(gltf, prim)
                if tex is not None and diffuse is None:
                    diffuse = tex
                for tri in triangles(gltf, buffers, prim):
                    for corner in tri:
                        verts.append(emit_vertex(world, nmat, corner))
    return verts, diffuse


def pad(verts):
    n = len(verts)
    if n == 0:
        return verts
    if n % 5 != 0:
        return verts
    verts.append(verts[0])
    verts.append(verts[0])
    verts.append(verts[0])
    return verts


def main():
    if len(sys.argv) != 3:
        die("usage: j3mesh_import.py model.gltf model.j3mesh")
    src = sys.argv[1]
    dst = sys.argv[2]
    if src.endswith(".glb"):
        die("json gltf only")
    base = os.path.dirname(os.path.abspath(src))
    with open(src, "r", encoding="utf-8") as f:
        gltf = json.load(f)
    buffers = load_buffers(gltf, base)
    verts, diffuse = collect(gltf, buffers)
    if not verts:
        die("no triangles")
    verts = pad(verts)
    png = ""
    if diffuse is not None:
        png = write_image(gltf, buffers, diffuse, dst)
    lines = ["# j3mesh 2", "# diffuse: " + png, str(len(verts))]
    for v in verts:
        lines.append(" ".join(fmt(x) for x in v))
    text = "\n".join(lines) + "\n"
    with open(dst, "w", encoding="utf-8") as f:
        f.write(text)
    print("diffuse: " + png)


if __name__ == "__main__":
    main()
