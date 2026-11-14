#!/usr/bin/env python3
"""Lower the picture resolution the jina vision tower works at.

The v5-omni vision projector carries its own idea of how large a picture
should be: `clip.vision.image_min_pixels` (512x512) and
`clip.vision.image_max_pixels`. Those decide how many patches the vision
tower sees, and the tower is the expensive part: on this box's little cores
one picture costs about 88 seconds at the shipped 256-token setting.

Halving the budget quarters the work, which is the difference between an
index that finishes in an afternoon and one that finishes next week. The
cost is detail in the picture, and for "find me a photo of a red apple" the
coarse shape is enough.

The value is written in place: the key stays the same length, so only the
four bytes of the number change and the rest of the file is untouched.
"""

import argparse
import shutil
import struct
import sys
from pathlib import Path

KEY_MIN = b"clip.vision.image_min_pixels"
KEY_MAX = b"clip.vision.image_max_pixels"

# A token is one merged patch: patch_size (16) squared, times the 2x2 merge.
PIXELS_PER_TOKEN = 16 * 16 * 2 * 2


def _read_u32(data: bytes, offset: int) -> int:
    return struct.unpack_from("<I", data, offset)[0]


def _read_u64(data: bytes, offset: int) -> int:
    return struct.unpack_from("<Q", data, offset)[0]


def _find_value_offset(data: bytes, key: bytes) -> int:
    """Where the u32 value of `key` sits in the file.

    The metadata section stores each key as its length, its bytes, the type
    of the value and then the value. Finding the key by its bytes and
    stepping over the header is enough -- the key is unique in the file.
    """
    pos = data.find(key)
    if pos < 0:
        raise KeyError(f"{key.decode()} 不在这个文件里")
    key_len = _read_u64(data, pos - 8)
    if key_len != len(key):
        raise ValueError(f"{key.decode()} 前面不是它的长度字段")
    type_offset = pos + key_len
    value_type = _read_u32(data, type_offset)
    if value_type != 4:  # GGUF_TYPE_UINT32
        raise ValueError(f"{key.decode()} 的类型是 {value_type}，不是 u32")
    return type_offset + 4


def patch(src: Path, dst: Path, tokens: int) -> None:
    pixels = tokens * PIXELS_PER_TOKEN
    with open(src, "rb") as f:
        data = bytearray(f.read())
    for key in (KEY_MIN, KEY_MAX):
        offset = _find_value_offset(data, key)
        old = _read_u32(data, offset)
        struct.pack_into("<I", data, offset, pixels)
        print(f"  {key.decode()}: {old} -> {pixels} 像素（{tokens} token）")
    dst.parent.mkdir(parents=True, exist_ok=True)
    with open(dst, "wb") as f:
        f.write(data)
    print(f"已写入 {dst}（{dst.stat().st_size / 1e6:.0f} MB）")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("src", type=Path, help="原始 mmproj")
    ap.add_argument("dst", type=Path, help="输出的 mmproj")
    ap.add_argument("--tokens", type=int, default=64,
                    help="图片的 token 预算（默认 64，约 256x256）")
    args = ap.parse_args()

    if not args.src.exists():
        print(f"找不到 {args.src}", file=sys.stderr)
        return 1
    if args.src.resolve() == args.dst.resolve():
        print("输入和输出不能是同一个文件", file=sys.stderr)
        return 1
    if args.tokens < 16:
        print("token 预算太小了（至少 16）", file=sys.stderr)
        return 1

    print(f"把 {args.src} 的分辨率降到 {args.tokens} token")
    patch(args.src, args.dst, args.tokens)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
