"""Small deterministic synthetic PNGs; no files, credentials or external URLs."""

import base64
import struct
import zlib


def _png(rows):
    """Encode fixed 192x192 RGB scanlines without an image-library dependency."""
    def chunk(kind, data):
        return (
            struct.pack(">I", len(data)) + kind + data
            + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
        )

    png = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", 192, 192, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(b"".join(b"\x00" + row for row in rows)))
        + chunk(b"IEND", b"")
    )
    return "data:image/png;base64," + base64.b64encode(png).decode("ascii")


def color_png(rgb):
    """Create a flat-color 192x192 RGB PNG."""
    return _png([bytes(rgb) * 192] * 192)


def _text(protocol, value):
    return {"type": "text" if protocol == "chat" else "input_text", "text": value}


def _image(protocol, url):
    return (
        {"type": "image_url", "image_url": {"url": url}}
        if protocol == "chat"
        else {"type": "input_image", "image_url": url}
    )


def image_history(protocol):
    """Interleave two distinct images and text; answer depends on image order."""
    return [{"role": "user", "content": [
        _text(protocol, "Identify the dominant color of each image in order. Reply with exactly two lowercase English color names separated by a comma, no spaces or other text."),
        _image(protocol, color_png((255, 0, 0))),
        _text(protocol, "Now the second image:"),
        _image(protocol, color_png((0, 0, 255))),
    ]}]


def _grid_png(grid):
    rows = []
    colors = {"R": bytes((255, 0, 0)), "B": bytes((0, 0, 255))}
    for y in range(192):
        row = bytearray()
        for x in range(192):
            row.extend(
                colors[grid[y // 64][x // 64]]
                if 8 <= x % 64 < 56 and 8 <= y % 64 < 56
                else bytes((255, 255, 255))
            )
        rows.append(bytes(row))
    return _png(rows)


def visual_math_history(protocol):
    """Require independent visual counts before combining the two panels."""
    return [{"role": "user", "content": [
        _text(protocol, "Each image contains separate red and blue squares on a white background. Let R1 and B1 be the numbers of red and blue squares in the first image, and R2 and B2 those in the second image. Compute R1 * B2 - B1 * R2. Return only a JSON object with exactly one integer field answer."),
        _image(protocol, _grid_png(("RRB", "BRR", "BBR"))),
        _text(protocol, "Second image:"),
        _image(protocol, _grid_png(("BRB", "BBR", "RBB"))),
    ]}]
