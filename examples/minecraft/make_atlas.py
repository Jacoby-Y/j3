"""Build the small CC0 procedural block atlas used by the Minecraft demo."""

from pathlib import Path
import struct
import zlib

SIZE = 32
COLS = 8
ROWS = 2
WIDTH = SIZE * COLS
HEIGHT = SIZE * ROWS


def noise(x, y, seed=0):
    n = (x * 374761393 + y * 668265263 + seed * 1442695041) & 0xFFFFFFFF
    n = ((n ^ (n >> 13)) * 1274126177) & 0xFFFFFFFF
    return ((n ^ (n >> 16)) & 255) / 255.0


def color(tile, x, y):
    n = noise(x, y, tile)
    large = noise(x // 4, y // 4, tile + 19)
    grain = (n - 0.5) * 22 + (large - 0.5) * 25
    if tile == 0:  # grass top
        base = (85, 135, 62) if n > 0.17 else (58, 111, 44)
        if large > 0.88:
            base = (153, 148, 72)
    elif tile == 1:  # grass side
        if y < 6 + int(noise(x, 0, 13) * 5):
            base = (74, 132, 57)
        else:
            base = (116, 87, 57)
            if n > 0.84:
                base = (145, 105, 69)
    elif tile == 2:  # dirt
        base = (111, 80, 52) if n < 0.83 else (149, 105, 65)
    elif tile == 3:  # stone
        base = (111, 119, 120)
        if large > 0.73:
            base = (135, 138, 132)
        if (x * 3 + y * 2) % 19 == 0 and n < 0.55:
            base = (66, 80, 83)
    elif tile == 4:  # sand
        base = (190, 170, 116)
        grain *= 0.65
    elif tile == 5:  # bark
        groove = abs((x + int(3 * noise(0, y // 3, 7))) % 7 - 3)
        base = (102, 71, 45) if groove > 1 else (59, 46, 35)
    elif tile == 6:  # log end
        r = max(abs(x - 16), abs(y - 16))
        base = (164, 126, 78) if r < 14 else (70, 52, 38)
        if r < 14 and r % 4 == 0:
            base = (119, 84, 54)
    elif tile == 7:  # leaves
        base = (69, 119, 63) if n > 0.35 else (41, 82, 51)
        if large > 0.8:
            base = (122, 139, 62)
    elif tile == 8:  # boards
        seam = y % 11
        base = (164, 118, 70) if seam else (80, 54, 39)
        if (x + 8 * (y // 11)) % 28 == 0:
            base = (89, 64, 45)
    elif tile == 9:  # gravel
        base = (118, 118, 108) if large > 0.35 else (78, 86, 81)
    elif tile == 10:  # copper ore
        base = (105, 111, 107)
        if large > 0.79 and n > 0.55:
            base = (184, 112, 58)
    elif tile == 11:  # glass icon
        base = (101, 159, 174) if x < 3 or y < 3 or x > 28 or y > 28 else (42, 91, 111)
        grain *= 0.25
    elif tile == 12:  # metal icon
        base = (178, 184, 179) if (x + y) % 8 < 4 else (128, 145, 146)
        grain *= 0.3
    else:
        base = (87, 97, 90)
    # gfx.tex uploads RGBA8 without automatic sRGB decoding. Store linear
    # albedo so the post pass can tone-map and gamma-encode the lit result once.
    def linear_byte(channel):
        srgb = max(0.0, min(1.0, (channel + grain) / 255.0))
        return int(srgb ** 2.2 * 255.0 + 0.5)

    return bytes(linear_byte(c) for c in base) + b"\xff"


def chunk(name, data):
    body = name + data
    return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))


def main():
    raw = bytearray()
    for y in range(HEIGHT):
        raw.append(0)
        for x in range(WIDTH):
            raw.extend(color(x // SIZE + COLS * (y // SIZE), x % SIZE, y % SIZE))
    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", WIDTH, HEIGHT, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(raw, 9))
    png += chunk(b"IEND", b"")
    target = Path(__file__).parent / "assets" / "blocks.png"
    target.parent.mkdir(exist_ok=True)
    target.write_bytes(png)


if __name__ == "__main__":
    main()
