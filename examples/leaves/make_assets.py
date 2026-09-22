#!/usr/bin/env python3
"""Generate the yard meshes, textures, and one-shot sounds for examples/leaves.

Original geometry and images for this example. Run from anywhere:

    python3 examples/leaves/make_assets.py
"""

import math
import os
import random
import struct
import wave
import zlib

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "assets")

HALF_X = 4.0
HALF_Z = 3.2
WALL_Y = 3.5
RIDGE_Y = 6.05
OH_Z = 0.55
OH_X = 0.42
BASE_Y = 0.42


def roof_y(z):
    return RIDGE_Y + (WALL_Y - RIDGE_Y) * (abs(z) / HALF_Z)


def write_png(path, w, h, rgba):
    """rgba is top-row-first. Rows are flipped so GL's first byte is the bottom."""

    def chunk(tag, data):
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    raw = bytearray()
    stride = w * 4
    for y in range(h - 1, -1, -1):
        raw.append(0)
        raw.extend(rgba[y * stride : (y + 1) * stride])
    ihdr = struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)
    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr)
    png += chunk(b"IDAT", zlib.compress(bytes(raw), 9))
    png += chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(png)


def hsh(ix, iy):
    n = (int(ix) * 374761393 + int(iy) * 668265263) & 0xFFFFFFFF
    n = ((n ^ (n >> 13)) * 1274126177) & 0xFFFFFFFF
    return (n & 0xFFFFFF) / 16777215.0


def fade(t):
    return t * t * (3.0 - 2.0 * t)


def vnoise(x, y, period):
    x = x % period
    y = y % period
    x0 = int(math.floor(x)) % period
    y0 = int(math.floor(y)) % period
    x1 = (x0 + 1) % period
    y1 = (y0 + 1) % period
    tx = fade(x - math.floor(x))
    ty = fade(y - math.floor(y))
    a = hsh(x0, y0) * (1 - tx) + hsh(x1, y0) * tx
    b = hsh(x0, y1) * (1 - tx) + hsh(x1, y1) * tx
    return a * (1 - ty) + b * ty


def put(img, w, x, y, r, g, b, a=255):
    i = (y * w + x) * 4
    img[i] = r
    img[i + 1] = g
    img[i + 2] = b
    img[i + 3] = a


def clamp_b(v):
    return max(0, min(255, int(v)))


def fill(w, h, fn):
    img = bytearray(w * h * 4)
    for y in range(h):
        for x in range(w):
            r, g, b, a = fn(x, y)
            put(img, w, x, y, clamp_b(r), clamp_b(g), clamp_b(b), clamp_b(a))
    return img


def grass_px(x, y, n):
    u = x / n * 8.0
    v = y / n * 8.0
    p = 8.0
    a = vnoise(u, v, p)
    b = vnoise(u * 2.1 + 3.0, v * 2.1 + 1.0, p * 2)
    t = a * 0.65 + b * 0.35
    r = 78 + t * 46
    g = 118 + t * 62
    bcol = 48 + t * 24
    if hsh(x * 3 + 1, y * 5 + 2) > 0.988:
        r, g, bcol = 196, 118, 42
    elif hsh(x * 7, y * 3) > 0.993:
        r, g, bcol = 214, 176, 62
    return r, g, bcol, 255


def siding_px(x, y, n):
    board = 18
    row = y % board
    seam = row < 2 or row > board - 3
    grain = vnoise(x / n * 6, y / n * 2, 6)
    r = 214 + grain * 18
    g = 196 + grain * 16
    b = 150 + grain * 14
    if seam:
        r, g, b = r * 0.72, g * 0.70, b * 0.66
    if x % 96 < 2:
        r, g, b = r * 0.85, g * 0.85, b * 0.85
    return r, g, b, 255


def roof_px(x, y, n):
    row_h = 22
    row = y // row_h
    ox = x + (row_h // 2 if row % 2 else 0)
    col = ox % 36
    seam = col < 2 or (y % row_h) < 2
    shade = 0.82 + 0.18 * hsh(ox // 36, row)
    r = 92 * shade
    g = 108 * shade
    b = 96 * shade
    if seam:
        r, g, b = r * 0.55, g * 0.55, b * 0.55
    return r, g, b, 255


def wood_px(x, y, n):
    grain = vnoise(x / n * 3.0, y / n * 14.0, 14)
    line = 1.0 if (y % 7) == 0 else 0.0
    r = 168 + grain * 30 - line * 28
    g = 132 + grain * 22 - line * 22
    b = 86 + grain * 16 - line * 14
    return r, g, b, 255


def concrete_px(x, y, n):
    t = vnoise(x / n * 5, y / n * 5, 5)
    slab = 128
    joint = (x % slab) < 3 or (y % slab) < 3
    r = 148 + t * 18
    g = 146 + t * 16
    b = 138 + t * 14
    if joint:
        r, g, b = 118, 116, 110
    return r, g, b, 255


def asphalt_px(x, y, n):
    t = hsh(x, y)
    r = 58 + t * 18
    g = 60 + t * 16
    b = 64 + t * 14
    if hsh(x * 2, y * 3) > 0.97:
        r, g, b = r + 30, g + 30, b + 28
    return r, g, b, 255


def bark_px(x, y, n):
    groove = 1.0 if (x % 9) < 2 else 0.0
    t = vnoise(x / n * 2, y / n * 8, 8)
    r = 92 + t * 28 - groove * 30
    g = 64 + t * 18 - groove * 22
    b = 42 + t * 12 - groove * 14
    return r, g, b, 255


def brick_px(x, y, n):
    row_h = 16
    row = y // row_h
    ox = x + (20 if row % 2 else 0)
    mortar = (y % row_h) < 2 or (ox % 40) < 2
    shade = 0.85 + 0.15 * hsh(ox // 40, row)
    if mortar:
        return 186, 178, 168, 255
    return 158 * shade, 74 * shade, 58 * shade, 255


def mail_px(x, y, n):
    u = x / (n - 1)
    v = y / (n - 1)
    band = 0.62 < v < 0.78
    if band:
        return 232, 208, 96, 255
    shade = 0.9 + 0.1 * vnoise(u * 4, v * 4, 4)
    return 46 * shade, 92 * shade, 64 * shade, 255


def leaf_image(n, palette):
    img = bytearray(n * n * 4)
    cx = (n - 1) / 2
    for y in range(n):
        for x in range(n):
            u = (x - cx) / (n * 0.36)
            v = (y / (n - 1)) * 2.0 - 0.15
            serr = 0.06 * math.sin(v * 28.0)
            half = math.sin(max(0.0, min(1.0, v / 1.7)) * math.pi) * (0.55 + serr)
            stem = v < 0.08 and abs(u) < 0.06
            inside = abs(u) <= half and 0.0 <= v <= 1.72
            if stem:
                put(img, n, x, y, 92, 64, 36, 255)
                continue
            if not inside:
                continue
            t = v / 1.72
            r0, g0, b0 = palette[0]
            r1, g1, b1 = palette[1]
            r = r0 + (r1 - r0) * t
            g = g0 + (g1 - g0) * t
            b = b0 + (b1 - b0) * t
            vein = abs(u) < 0.035 or (
                abs(abs(u) - half * 0.45) < 0.02 and (int(v * 18) % 2 == 0)
            )
            if vein:
                r, g, b = r * 0.72, g * 0.62, b * 0.5
            edge = abs(u) > half * 0.92
            a = 180 if edge else 255
            put(img, n, x, y, clamp_b(r), clamp_b(g), clamp_b(b), a)
    return img


class Mesh:
    def __init__(self):
        self.v = []

    def tri(self, a, b, c, uva, uvb, uvc, col=(1, 1, 1), emi=0.0):
        ax, ay, az = a
        bx, by, bz = b
        cx, cy, cz = c
        nx = (by - ay) * (cz - az) - (bz - az) * (cy - ay)
        ny = (bz - az) * (cx - ax) - (bx - ax) * (cz - az)
        nz = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax)
        length = math.sqrt(nx * nx + ny * ny + nz * nz)
        if length < 1e-8:
            return
        nx, ny, nz = nx / length, ny / length, nz / length
        r, g, bcol = col
        for p, uv in ((a, uva), (b, uvb), (c, uvc)):
            self.v.append(
                (
                    p[0],
                    p[1],
                    p[2],
                    nx,
                    ny,
                    nz,
                    r,
                    g,
                    bcol,
                    emi,
                    uv[0],
                    uv[1],
                )
            )

    def quad(self, p0, p1, p2, p3, uv, col=(1, 1, 1), emi=0.0):
        self.tri(p0, p1, p2, uv[0], uv[1], uv[2], col, emi)
        self.tri(p0, p2, p3, uv[0], uv[2], uv[3], col, emi)

    def quad_out(self, p0, p1, p2, p3, uv, outward, col=(1, 1, 1), emi=0.0):
        ax, ay, az = p0
        bx, by, bz = p1
        cx, cy, cz = p2
        nx = (by - ay) * (cz - az) - (bz - az) * (cy - ay)
        ny = (bz - az) * (cx - ax) - (bx - ax) * (cz - az)
        nz = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax)
        if nx * outward[0] + ny * outward[1] + nz * outward[2] < 0:
            p1, p3 = p3, p1
            uv = (uv[0], uv[3], uv[2], uv[1])
        self.quad(p0, p1, p2, p3, uv, col, emi)

    def save(self, path):
        lines = ["# j3mesh 2", str(len(self.v))]
        for vert in self.v:
            lines.append(" ".join(f"{n:.4f}" for n in vert))
        with open(path, "w", encoding="utf-8") as f:
            f.write("\n".join(lines) + "\n")


def add_box(m, x0, y0, z0, x1, y1, z1, tu=1.0, tv=1.0, col=(1, 1, 1)):
    uv = ((0, 0), (tu, 0), (tu, tv), (0, tv))
    m.quad_out((x0, y1, z0), (x1, y1, z0), (x1, y1, z1), (x0, y1, z1), uv, (0, 1, 0), col)
    m.quad_out((x0, y0, z1), (x1, y0, z1), (x1, y0, z0), (x0, y0, z0), uv, (0, -1, 0), col)
    m.quad_out((x0, y0, z1), (x1, y0, z1), (x1, y1, z1), (x0, y1, z1), uv, (0, 0, 1), col)
    m.quad_out((x1, y0, z0), (x0, y0, z0), (x0, y1, z0), (x1, y1, z0), uv, (0, 0, -1), col)
    m.quad_out((x1, y0, z1), (x1, y0, z0), (x1, y1, z0), (x1, y1, z1), uv, (1, 0, 0), col)
    m.quad_out((x0, y0, z0), (x0, y0, z1), (x0, y1, z1), (x0, y1, z0), uv, (-1, 0, 0), col)


def add_quad_y(m, x0, x1, z0, z1, y, u0, u1, v0, v1):
    m.quad_out(
        (x0, y, z0),
        (x1, y, z0),
        (x1, y, z1),
        (x0, y, z1),
        ((u0, v0), (u1, v0), (u1, v1), (u0, v1)),
        (0, 1, 0),
    )


def add_cyl(m, y0, y1, r0, r1, segs, u_scale=2.0):
    for i in range(segs):
        a0 = math.tau * i / segs
        a1 = math.tau * (i + 1) / segs
        c0, s0 = math.cos(a0), math.sin(a0)
        c1, s1 = math.cos(a1), math.sin(a1)
        p0 = (c0 * r0, y0, s0 * r0)
        p1 = (c1 * r0, y0, s1 * r0)
        p2 = (c1 * r1, y1, s1 * r1)
        p3 = (c0 * r1, y1, s0 * r1)
        mid = (a0 + a1) * 0.5
        outward = (math.cos(mid), 0.2, math.sin(mid))
        u0 = u_scale * i / segs
        u1 = u_scale * (i + 1) / segs
        m.quad_out(p0, p1, p2, p3, ((u0, 0), (u1, 0), (u1, 1), (u0, 1)), outward)


def add_card(m, yaw, y0, y1, half_w, radius):
    c = math.cos(yaw)
    s = math.sin(yaw)

    def rot(x, z):
        return (x * c - z * s, x * s + z * c)

    x0, z0 = rot(-half_w, radius)
    x1, z1 = rot(half_w, radius)
    p0 = (x0, y0, z0)
    p1 = (x1, y0, z1)
    p2 = (x1, y1, z1)
    p3 = (x0, y1, z0)
    uv = ((0, 0), (1, 0), (1, 1), (0, 1))
    m.quad(p0, p1, p2, p3, uv)
    m.quad(p0, p3, p2, p1, (uv[0], uv[3], uv[2], uv[1]))


def build_ground():
    m = Mesh()
    add_quad_y(m, -28, 28, -22, 22, 0, 0, 10, 0, 8)
    return m


def build_street():
    m = Mesh()
    add_quad_y(m, -28, 28, 12.7, 20.5, 0.012, 0, 14, 0, 2)
    return m


def build_walk():
    m = Mesh()
    add_quad_y(m, -28, 28, 10.95, 12.55, 0.02, 0, 7, 0, 0.45)
    add_box(m, -28, 0.02, 12.45, 28, 0.12, 12.72, 12, 0.3)
    return m


def build_drive():
    m = Mesh()
    add_quad_y(m, -1.45, 1.45, 5.45, 11.15, 0.028, 0, 1, 0, 3)
    return m


def build_stripe():
    m = Mesh()
    z0 = 15.9
    z1 = 16.35
    x = -24
    while x < 26:
        add_quad_y(m, x, x + 1.6, z0, z1, 0.03, 0, 1, 0, 1)
        x += 3.4
    return m


def build_base():
    m = Mesh()
    add_box(m, -4.18, 0.0, -3.38, 4.18, BASE_Y, 3.38, 4, 0.4)
    return m


def build_walls():
    m = Mesh()
    y0 = BASE_Y
    y1 = WALL_Y
    x0, x1 = -HALF_X, HALF_X
    z0, z1 = -HALF_Z, HALF_Z
    uv_f = ((0, 0), (4, 0), (4, 2), (0, 2))
    m.quad_out(
        (x0, y0, z1),
        (x1, y0, z1),
        (x1, y1, z1),
        (x0, y1, z1),
        uv_f,
        (0, 0, 1),
    )
    m.quad_out(
        (x1, y0, z0),
        (x0, y0, z0),
        (x0, y1, z0),
        (x1, y1, z0),
        uv_f,
        (0, 0, -1),
    )
    uv_s = ((0, 0), (3, 0), (3, 2), (0, 2))
    m.quad_out(
        (x1, y0, z1),
        (x1, y0, z0),
        (x1, y1, z0),
        (x1, y1, z1),
        uv_s,
        (1, 0, 0),
    )
    m.quad_out(
        (x0, y0, z0),
        (x0, y0, z1),
        (x0, y1, z1),
        (x0, y1, z0),
        uv_s,
        (-1, 0, 0),
    )
    for sign, outward in ((1, (1, 0, 0)), (-1, (-1, 0, 0))):
        x = HALF_X * sign
        a = (x, y1, z1)
        b = (x, y1, z0)
        c = (x, RIDGE_Y, 0)
        ax, ay, az = a
        bx, by, bz = b
        cx, cy, cz = c
        nx = (by - ay) * (cz - az) - (bz - az) * (cy - ay)
        ny = (bz - az) * (cx - ax) - (bx - ax) * (cz - az)
        nz = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax)
        if nx * outward[0] + ny * outward[1] + nz * outward[2] < 0:
            b, a = a, b
        m.tri(a, b, c, (0, 0), (1, 0), (0.5, 1))
    return m


def build_roof():
    m = Mesh()
    ex = HALF_X + OH_X
    ez = HALF_Z + OH_Z
    ey = roof_y(ez)
    thick = 0.12

    def slope(z_sign):
        z_e = ez * z_sign
        y_e = ey
        p0 = (-ex, y_e, z_e)
        p1 = (ex, y_e, z_e)
        p2 = (ex, RIDGE_Y, 0)
        p3 = (-ex, RIDGE_Y, 0)
        outward = (0, 1, 0.4 * z_sign)
        uv = ((0, 0), (4, 0), (4, 2), (0, 2))
        m.quad_out(p0, p1, p2, p3, uv, outward)
        n = _norm(p0, p1, p2)
        if n[0] * outward[0] + n[1] * outward[1] + n[2] * outward[2] < 0:
            n = (-n[0], -n[1], -n[2])
        q0 = (p0[0] - n[0] * thick, p0[1] - n[1] * thick, p0[2] - n[2] * thick)
        q1 = (p1[0] - n[0] * thick, p1[1] - n[1] * thick, p1[2] - n[2] * thick)
        q2 = (p2[0] - n[0] * thick, p2[1] - n[1] * thick, p2[2] - n[2] * thick)
        q3 = (p3[0] - n[0] * thick, p3[1] - n[1] * thick, p3[2] - n[2] * thick)
        m.quad_out(q0, q3, q2, q1, uv, (-outward[0], -outward[1], -outward[2]))
        m.quad_out(p0, q0, q1, p1, uv, (0, -0.2, z_sign))
        m.quad_out(p1, q1, q2, p2, ((0, 0), (1, 0), (1, 1), (0, 1)), (1, 0, 0))
        m.quad_out(p3, p2, q2, q3, ((0, 0), (1, 0), (1, 1), (0, 1)), (-1, 0, 0))

    slope(1)
    slope(-1)
    return m


def _norm(a, b, c):
    ax, ay, az = a
    bx, by, bz = b
    cx, cy, cz = c
    nx = (by - ay) * (cz - az) - (bz - az) * (cy - ay)
    ny = (bz - az) * (cx - ax) - (bx - ax) * (cz - az)
    nz = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax)
    length = math.sqrt(nx * nx + ny * ny + nz * nz) or 1
    return (nx / length, ny / length, nz / length)


def build_porch():
    m = Mesh()
    add_box(m, -2.85, 0.22, HALF_Z - 0.05, 2.85, 0.46, 5.55, 2, 1)
    for sx in (-2.45, 2.45):
        add_box(m, sx - 0.12, 0.46, 5.25, sx + 0.12, 3.02, 5.49, 0.4, 2)
    add_box(m, -3.05, 2.95, 3.05, 3.05, 3.18, 5.85, 2, 1)
    add_box(m, -0.62, 0.46, HALF_Z, 0.62, 2.62, HALF_Z + 0.08, 0.5, 1.2)
    return m


def build_steps():
    m = Mesh()
    add_box(m, -1.7, 0.0, 5.5, 1.7, 0.16, 6.55, 1, 0.4)
    add_box(m, -1.55, 0.16, 5.5, 1.55, 0.32, 6.15, 1, 0.3)
    add_box(m, -1.4, 0.32, 5.5, 1.4, 0.46, 5.8, 1, 0.25)
    return m


def build_door():
    m = Mesh()
    z = HALF_Z + 0.05
    m.quad_out(
        (-0.48, 0.46, z),
        (0.48, 0.46, z),
        (0.48, 2.5, z),
        (-0.48, 2.5, z),
        ((0, 0), (1, 0), (1, 1), (0, 1)),
        (0, 0, 1),
    )
    return m


def build_glass():
    m = Mesh()

    def window(x, y, z, w, h, axis):
        if axis == "z":
            m.quad_out(
                (x, y, z),
                (x + w, y, z),
                (x + w, y + h, z),
                (x, y + h, z),
                ((0, 0), (1, 0), (1, 1), (0, 1)),
                (0, 0, 1 if z > 0 else -1),
            )
        else:
            sign = 1 if x > 0 else -1
            m.quad_out(
                (x, y, z),
                (x, y, z + w * sign),
                (x, y + h, z + w * sign),
                (x, y + h, z),
                ((0, 0), (1, 0), (1, 1), (0, 1)),
                (sign, 0, 0),
            )

    window(-2.75, 1.5, HALF_Z + 0.05, 1.15, 1.3, "z")
    window(1.6, 1.5, HALF_Z + 0.05, 1.15, 1.3, "z")
    window(-2.15, 1.55, -HALF_Z - 0.05, 1.15, 1.2, "z")
    window(1.0, 1.55, -HALF_Z - 0.05, 1.15, 1.2, "z")
    window(HALF_X + 0.05, 1.55, -0.55, 1.15, 1.2, "x")
    window(-HALF_X - 0.05, 1.55, 0.55, 1.15, 1.2, "x")
    return m


def build_chimney():
    m = Mesh()
    add_box(m, 2.15, 4.3, -1.15, 3.15, 7.15, -0.15, 1, 2)
    add_box(m, 2.05, 7.15, -1.25, 3.25, 7.4, -0.05, 1, 0.3)
    return m


def build_fence():
    m = Mesh()

    def post(x, z):
        add_box(m, x - 0.07, 0, z - 0.07, x + 0.07, 1.22, z + 0.07, 0.3, 1)

    def rail(x0, x1, z0, z1, y):
        if abs(x1 - x0) >= abs(z1 - z0):
            add_box(m, x0, y, z0 - 0.04, x1, y + 0.08, z0 + 0.04, 2, 0.2)
        else:
            add_box(m, x0 - 0.04, y, z0, x0 + 0.04, y + 0.08, z1, 2, 0.2)

    x = -11.4
    while x <= 11.45:
        post(x, -13.2)
        x += 1.9
    z = -11.3
    while z <= -3.4:
        post(-11.4, z)
        post(11.4, z)
        z += 1.9
    rail(-11.4, 11.4, -13.2, -13.2, 0.42)
    rail(-11.4, 11.4, -13.2, -13.2, 0.92)
    rail(-11.4, -11.4, -13.2, -3.5, 0.42)
    rail(-11.4, -11.4, -13.2, -3.5, 0.92)
    rail(11.4, 11.4, -13.2, -3.5, 0.42)
    rail(11.4, 11.4, -13.2, -3.5, 0.92)
    return m


def build_trunk():
    m = Mesh()
    add_cyl(m, 0, 1.4, 0.2, 0.16, 8)
    add_cyl(m, 1.4, 2.7, 0.16, 0.12, 8)
    return m


def build_canopy():
    m = Mesh()
    for i in range(5):
        add_card(m, math.tau * i / 5, 2.15, 4.7, 1.55, 0.15)
    add_card(m, 0.4, 3.3, 3.55, 1.35, 0.0)
    return m


def build_bush():
    m = Mesh()
    for i in range(4):
        add_card(m, math.tau * i / 4, 0.02, 0.85, 0.48, 0.05)
    return m


def foliage_image(n):
    img = bytearray(n * n * 4)
    rng = random.Random(3)
    for _ in range(70):
        cx = rng.uniform(n * 0.18, n * 0.82)
        cy = rng.uniform(n * 0.12, n * 0.88)
        rx = rng.uniform(n * 0.08, n * 0.18)
        ry = rx * rng.uniform(1.3, 2.1)
        ang = rng.uniform(0, math.tau)
        pal = rng.choice(
            [(214, 168, 42), (196, 96, 36), (168, 62, 34), (120, 92, 28), (210, 140, 48)]
        )
        for y in range(n):
            for x in range(n):
                dx = x - cx
                dy = y - cy
                ca, sa = math.cos(ang), math.sin(ang)
                u = (dx * ca + dy * sa) / rx
                v = (-dx * sa + dy * ca) / ry
                if u * u + v * v > 1:
                    continue
                i = (y * n + x) * 4
                a = 230
                if img[i + 3] > 0:
                    a = 255
                img[i] = pal[0]
                img[i + 1] = pal[1]
                img[i + 2] = pal[2]
                img[i + 3] = a
    return img


def build_leaf():
    m = Mesh()
    stem = (0.0, 0.0, -0.04)
    left = (-0.28, 0.04, 0.22)
    tip = (0.0, 0.09, 0.78)
    right = (0.28, 0.04, 0.22)
    uv_s = (0.5, 0.02)
    uv_l = (0.12, 0.38)
    uv_t = (0.5, 0.98)
    uv_r = (0.88, 0.38)
    m.tri(stem, right, tip, uv_s, uv_r, uv_t)
    m.tri(stem, tip, left, uv_s, uv_t, uv_l)
    m.tri(stem, tip, right, uv_s, uv_t, uv_r)
    m.tri(stem, left, tip, uv_s, uv_l, uv_t)
    return m


def build_post():
    m = Mesh()
    add_box(m, -0.06, 0, -0.06, 0.06, 0.92, 0.06, 0.3, 1)
    return m


def build_box():
    m = Mesh()
    add_box(m, -0.16, 0.78, -0.1, 0.34, 1.08, 0.28, 1, 1)
    return m


def write_wav(path, samples, rate=22050):
    data = bytearray()
    for s in samples:
        v = max(-1.0, min(1.0, s))
        data += struct.pack("<h", int(v * 32767))
    with wave.open(path, "w") as handle:
        handle.setnchannels(1)
        handle.setsampwidth(2)
        handle.setframerate(rate)
        handle.writeframes(data)


def env_tone(freq, dur, vol, decay, rate=22050):
    n = int(rate * dur)
    out = []
    for i in range(n):
        t = i / rate
        out.append(math.sin(math.tau * freq * t) * math.exp(-t * decay) * vol)
    return out


def build_sounds():
    rate = 22050
    write_wav(os.path.join(ROOT, "pickup.wav"), env_tone(740, 0.09, 0.35, 28) )
    buy = env_tone(520, 0.12, 0.28, 16)
    buy += env_tone(780, 0.14, 0.22, 14)
    write_wav(os.path.join(ROOT, "buy.wav"), buy)
    win = []
    for freq in (523, 659, 784, 1046):
        win += env_tone(freq, 0.16, 0.3, 8)
    write_wav(os.path.join(ROOT, "win.wav"), win)
    n = rate * 8
    pad = []
    for i in range(n):
        t = i / rate
        s = 0.0
        s += math.sin(math.tau * 220 * t) * 0.08
        s += math.sin(math.tau * 277.18 * t) * 0.05
        s += math.sin(math.tau * 329.63 * t) * 0.04
        s += math.sin(math.tau * 3 * t) * 0.02
        # fade ends so the loop is less of a click
        edge = min(t / 0.05, (8 - t) / 0.05, 1.0)
        pad.append(s * edge)
    write_wav(os.path.join(ROOT, "theme.wav"), pad)


def main():
    os.makedirs(ROOT, exist_ok=True)
    n = 256
    write_png(os.path.join(ROOT, "grass.png"), n, n, fill(n, n, lambda x, y: grass_px(x, y, n)))
    write_png(os.path.join(ROOT, "siding.png"), n, n, fill(n, n, lambda x, y: siding_px(x, y, n)))
    write_png(os.path.join(ROOT, "roof.png"), n, n, fill(n, n, lambda x, y: roof_px(x, y, n)))
    write_png(os.path.join(ROOT, "wood.png"), n, n, fill(n, n, lambda x, y: wood_px(x, y, n)))
    write_png(os.path.join(ROOT, "concrete.png"), n, n, fill(n, n, lambda x, y: concrete_px(x, y, n)))
    write_png(os.path.join(ROOT, "asphalt.png"), n, n, fill(n, n, lambda x, y: asphalt_px(x, y, n)))
    write_png(os.path.join(ROOT, "bark.png"), n, n, fill(n, n, lambda x, y: bark_px(x, y, n)))
    write_png(os.path.join(ROOT, "brick.png"), 128, 128, fill(128, 128, lambda x, y: brick_px(x, y, 128)))
    write_png(os.path.join(ROOT, "door.png"), 128, 256, fill(128, 256, lambda x, y: door_px_rect(x, y, 128, 256)))
    write_png(os.path.join(ROOT, "glass.png"), 128, 160, fill(128, 160, lambda x, y: glass_px_rect(x, y, 128, 160)))
    write_png(os.path.join(ROOT, "mail.png"), 128, 128, fill(128, 128, lambda x, y: mail_px(x, y, 128)))
    write_png(
        os.path.join(ROOT, "leaf_a.png"),
        128,
        128,
        leaf_image(128, ((214, 176, 48), (196, 92, 36))),
    )
    write_png(
        os.path.join(ROOT, "leaf_b.png"),
        128,
        128,
        leaf_image(128, ((176, 64, 40), (120, 42, 36))),
    )
    write_png(os.path.join(ROOT, "foliage.png"), 128, 128, foliage_image(128))
    build_ground().save(os.path.join(ROOT, "ground.j3mesh"))
    build_street().save(os.path.join(ROOT, "street.j3mesh"))
    build_walk().save(os.path.join(ROOT, "walk.j3mesh"))
    build_drive().save(os.path.join(ROOT, "drive.j3mesh"))
    build_stripe().save(os.path.join(ROOT, "stripe.j3mesh"))
    build_base().save(os.path.join(ROOT, "base.j3mesh"))
    build_walls().save(os.path.join(ROOT, "wall.j3mesh"))
    build_roof().save(os.path.join(ROOT, "roof.j3mesh"))
    build_porch().save(os.path.join(ROOT, "porch.j3mesh"))
    build_steps().save(os.path.join(ROOT, "steps.j3mesh"))
    build_door().save(os.path.join(ROOT, "door.j3mesh"))
    build_glass().save(os.path.join(ROOT, "glass.j3mesh"))
    build_chimney().save(os.path.join(ROOT, "chimney.j3mesh"))
    build_fence().save(os.path.join(ROOT, "fence.j3mesh"))
    build_trunk().save(os.path.join(ROOT, "trunk.j3mesh"))
    build_canopy().save(os.path.join(ROOT, "canopy.j3mesh"))
    build_bush().save(os.path.join(ROOT, "bush.j3mesh"))
    build_leaf().save(os.path.join(ROOT, "leaf.j3mesh"))
    build_post().save(os.path.join(ROOT, "post.j3mesh"))
    build_box().save(os.path.join(ROOT, "box.j3mesh"))
    # stripe uses yellow vertex color
    tint_vertex_color(os.path.join(ROOT, "stripe.j3mesh"), (0.95, 0.82, 0.28))
    build_sounds()
    print("wrote", ROOT)


def door_px_rect(x, y, w, h):
    u = x / (w - 1)
    v = 1.0 - y / (h - 1)
    frame = u < 0.08 or u > 0.92 or v < 0.04 or v > 0.96
    if frame:
        return 232, 224, 206, 255
    panel = (0.18 < u < 0.82) and ((0.1 < v < 0.46) or (0.54 < v < 0.9))
    if panel:
        return 42, 86, 66, 255
    return 30, 64, 52, 255


def glass_px_rect(x, y, w, h):
    u = x / (w - 1)
    v = 1.0 - y / (h - 1)
    frame = u < 0.12 or u > 0.88 or v < 0.1 or v > 0.9
    mull = abs(u - 0.5) < 0.04 or abs(v - 0.5) < 0.035
    if frame or mull:
        return 236, 232, 220, 255
    return 150, 186, 204, 255


def tint_vertex_color(path, rgb):
    with open(path, encoding="utf-8") as f:
        lines = f.read().splitlines()
    out = lines[:2]
    for line in lines[2:]:
        if not line.strip():
            continue
        p = line.split()
        p[6], p[7], p[8] = f"{rgb[0]:.4f}", f"{rgb[1]:.4f}", f"{rgb[2]:.4f}"
        out.append(" ".join(p))
    with open(path, "w", encoding="utf-8") as f:
        f.write("\n".join(out) + "\n")


if __name__ == "__main__":
    main()
