#!/usr/bin/env python3
"""Generate the PWA icon set into frontend/public/.

Kept as a script rather than committed binaries so the icons stay reviewable and
regenerable. Pure stdlib PNG writer -- no Pillow dependency.

    ./scripts/gen-icons.py
"""
import struct
import zlib
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "frontend" / "public"

INDIGO = (79, 70, 229)
WHITE = (255, 255, 255)


def png(width: int, height: int, pixels: bytes) -> bytes:
    def chunk(tag: bytes, data: bytes) -> bytes:
        return (struct.pack(">I", len(data)) + tag + data
                + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF))

    raw = b"".join(b"\x00" + pixels[y * width * 4:(y + 1) * width * 4]
                   for y in range(height))
    return (b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw, 9))
            + chunk(b"IEND", b""))


def rounded(x: float, y: float, w: float, h: float, r: float) -> bool:
    """Inside a rounded rect centred in a w x h box?"""
    cx = abs(x - w / 2) - (w / 2 - r)
    cy = abs(y - h / 2) - (h / 2 - r)
    if cx <= 0 and cy <= 0:
        return True
    dx, dy = max(cx, 0.0), max(cy, 0.0)
    return dx * dx + dy * dy <= r * r


def icon(size: int, *, bleed: bool, glyph: bool) -> bytes:
    """Icon canvas.

    `bleed` fills the whole square (Android maskable: the launcher may crop to
    any shape, so the safe zone has to stay clear of the glyph).
    `glyph` draws the folder mark; favicon keeps just the rounded tile.
    """
    px = bytearray(size * size * 4)
    radius = size * 0.22
    # Maskable icons must survive a circular crop of the inner 80%, so keep the
    # artwork inside ~60% of the canvas.
    box = size if bleed else size * 0.94
    ox = oy = (size - box) / 2

    for y in range(size):
        for x in range(size):
            i = (y * size + x) * 4
            if bleed or rounded(x - ox, y - oy, box, box, radius):
                px[i:i + 4] = bytes((*INDIGO, 255))

    if glyph:
        # folder tab
        tab_w, tab_h = size * 0.26, size * 0.07
        tab_x, tab_y = size * 0.30, size * 0.30
        # folder body
        body_x, body_y = size * 0.26, size * 0.38
        body_w, body_h = size * 0.44, size * 0.26
        r = size * 0.05
        for y in range(size):
            for x in range(size):
                if not (tab_x <= x <= tab_x + tab_w and tab_y <= y <= tab_y + tab_h):
                    continue
                if not (body_x <= x <= body_x + body_w
                        and body_y <= y <= body_y + body_h
                        and rounded(x - body_x, y - body_y, body_w, body_h, r)):
                    continue
                i = (y * size + x) * 4
                px[i:i + 4] = bytes((*WHITE, 255))
    return png(size, size, bytes(px))


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    targets = [
        ("icon-192.png", 192, dict(bleed=False, glyph=True)),
        ("icon-512.png", 512, dict(bleed=False, glyph=True)),
        # Android maskable: full-bleed background, glyph inside the safe zone.
        ("icon-maskable-512.png", 512, dict(bleed=True, glyph=True)),
        ("apple-touch-icon.png", 180, dict(bleed=True, glyph=True)),
        ("favicon.png", 64, dict(bleed=False, glyph=False)),
    ]
    for name, size, kw in targets:
        (OUT / name).write_bytes(icon(size, **kw))
        print(f"wrote {name} ({size}x{size})")


if __name__ == "__main__":
    main()