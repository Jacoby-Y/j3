#!/usr/bin/env python3
"""Writes the tiny PNGs of speculative/55_ui_imagery/assets (run from that folder: python3 tools/make_assets.py)."""
import struct, zlib

def png(path, w, h, px):
    raw = b"".join(b"\x00" + bytes(c for x in range(w) for c in px(x, y)) for y in range(h))
    def chunk(t, d):
        c = struct.pack(">I", len(d)) + t + d
        return c + struct.pack(">I", zlib.crc32(t + d) & 0xffffffff)
    open(path, "wb").write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))

DARK, MID, LIGHT, GOLD, RED = (24, 28, 52, 255), (58, 70, 128, 255), (150, 170, 230, 255), (240, 190, 60, 255), (210, 70, 70, 255)

def frame(x, y):
    # 24 x 24, slice 8: gold corners, striped edges with a 4 px period, a checkered center
    cx, cy = x // 8, y // 8
    if cx != 1 and cy != 1:
        return RED if (x % 8 in (0, 7) or y % 8 in (0, 7)) else GOLD
    if cy != 1:
        return LIGHT if (x // 2) % 2 == 0 else MID
    if cx != 1:
        return LIGHT if (y // 2) % 2 == 0 else MID
    return DARK if ((x // 2) + (y // 2)) % 2 == 0 else (32, 38, 70, 255)

def dots(x, y):
    # 16 x 16, transparent with a round-ish dot
    dx, dy = x - 7.5, y - 7.5
    return (255, 255, 255, 120) if dx * dx + dy * dy < 14 else (0, 0, 0, 0)

def stripes(x, y):
    # 8 x 8 diagonal stripes
    return (255, 200, 60, 150) if (x + y) % 8 < 3 else (0, 0, 0, 0)

png("assets/frame.png", 24, 24, frame)
png("assets/dots.png", 16, 16, dots)
png("assets/stripes.png", 8, 8, stripes)
