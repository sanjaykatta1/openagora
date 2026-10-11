#!/usr/bin/env python3
"""OpenAgora's app icon, made from one square picture.

    python3 desktop/build/make-icons.py [picture]

The picture defaults to icon-source.jpg next to this script. Writes, next to
this script:

  icon.icns      macOS: the picture in Apple's rounded square, 824 of 1024
                 pixels with a clear margin, as macOS draws app icons;
                 16 to 1024 pixels.
  icon.ico       Windows: the picture with gently rounded corners, 16 to 256
                 pixels, uncompressed, which every Windows tool reads (the
                 installer's too).
  icons/NxN.png  Linux: the same as Windows, 16 to 512 pixels. OpenAgora shows
                 icons/256x256.png on its windows.

Colours are converted to sRGB, the colours icons are shown in. electron-builder
takes these files as they are (package.json, "build"). Needs Pillow.
"""

import io
import struct
import sys
from pathlib import Path

from PIL import Image, ImageCms, ImageDraw

HERE = Path(__file__).resolve().parent
MASTER = 1024
# Apple's grid for macOS app icons: an 824-pixel rounded square, corner radius
# 185.4, in the middle of 1024.
MAC_BODY, MAC_RADIUS = 824, 185.4
# Windows and Linux: the whole square, its corners rounded by an eighth.
FULL_RADIUS = MASTER / 8
# What macOS reads from an icns: PNGs for 16 to 512 points at 1x and 2x.
ICNS_ENTRIES = [
    (b"ic11", 32), (b"ic12", 64), (b"ic07", 128), (b"ic13", 256),
    (b"ic08", 256), (b"ic14", 512), (b"ic09", 512), (b"ic10", 1024),
]
ICO_SIZES = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256]
LINUX_SIZES = [16, 24, 32, 48, 64, 128, 256, 512]
SUPERSAMPLE = 8


def srgb(picture: Image.Image) -> Image.Image:
    """The picture in sRGB (a phone's or Mac's picture may be in Display P3)."""
    profile = picture.info.get("icc_profile")
    picture = picture.convert("RGB")
    if not profile:
        return picture
    return ImageCms.profileToProfile(
        picture,
        ImageCms.ImageCmsProfile(io.BytesIO(profile)),
        ImageCms.createProfile("sRGB"),
        renderingIntent=ImageCms.Intent.RELATIVE_COLORIMETRIC,
        outputMode="RGB",
    )


def square(picture: Image.Image) -> Image.Image:
    side = min(picture.size)
    left = (picture.width - side) // 2
    top = (picture.height - side) // 2
    return picture.crop((left, top, left + side, top + side))


def rounded(picture: Image.Image, body: int, radius: float) -> Image.Image:
    """The picture as a rounded square body pixels wide, in the middle of a
    clear MASTER-pixel canvas, its edge smoothed."""
    mask = Image.new("L", (MASTER * SUPERSAMPLE,) * 2, 0)
    start = (MASTER - body) // 2 * SUPERSAMPLE
    end = start + body * SUPERSAMPLE - 1
    ImageDraw.Draw(mask).rounded_rectangle((start, start, end, end), radius=radius * SUPERSAMPLE, fill=255)
    canvas = Image.new("RGBA", (MASTER, MASTER), (0, 0, 0, 0))
    canvas.paste(picture.resize((body, body), Image.Resampling.LANCZOS), ((MASTER - body) // 2,) * 2)
    canvas.putalpha(mask.reduce(SUPERSAMPLE))
    return canvas


def sized(master: Image.Image, size: int) -> Image.Image:
    # Pillow resizes RGBA with premultiplied alpha, so edges stay clean.
    return master if size == master.width else master.resize((size, size), Image.Resampling.LANCZOS)


def png(image: Image.Image) -> bytes:
    out = io.BytesIO()
    image.save(out, "PNG", optimize=True)
    return out.getvalue()


def icns(master: Image.Image) -> bytes:
    pngs = {size: png(sized(master, size)) for size in {size for _, size in ICNS_ENTRIES}}
    entries = [kind + struct.pack(">I", 8 + len(pngs[size])) + pngs[size] for kind, size in ICNS_ENTRIES]
    toc = b"TOC " + struct.pack(">I", 8 + 8 * len(entries)) + b"".join(entry[:8] for entry in entries)
    body = toc + b"".join(entries)
    return b"icns" + struct.pack(">I", 8 + len(body)) + body


def ico_image(image: Image.Image) -> bytes:
    """One icon in an ICO: a 32-bit bottom-up bitmap, then the 1-bit mask
    (set where the icon is fully clear) that older readers use."""
    width, height = image.size
    pixels = image.tobytes("raw", "BGRA", 0, -1)
    alpha = image.getchannel("A").transpose(Image.Transpose.FLIP_TOP_BOTTOM).tobytes()
    stride = (width + 31) // 32 * 4
    mask = bytearray(stride * height)
    for y in range(height):
        for x in range(width):
            if alpha[y * width + x] == 0:
                mask[y * stride + x // 8] |= 0x80 >> (x % 8)
    header = struct.pack("<IiiHHIIiiII", 40, width, height * 2, 1, 32, 0, len(pixels) + len(mask), 0, 0, 0, 0)
    return header + pixels + bytes(mask)


def ico(master: Image.Image) -> bytes:
    images = [ico_image(sized(master, size)) for size in ICO_SIZES]
    offset = 6 + 16 * len(images)
    directory = struct.pack("<HHH", 0, 1, len(images))
    for size, data in zip(ICO_SIZES, images):
        directory += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
    return directory + b"".join(images)


def main() -> None:
    source = Path(sys.argv[1]) if len(sys.argv) > 1 else HERE / "icon-source.jpg"
    with Image.open(source) as opened:
        picture = square(srgb(opened))
    (HERE / "icon.icns").write_bytes(icns(rounded(picture, MAC_BODY, MAC_RADIUS)))
    full = rounded(picture, MASTER, FULL_RADIUS)
    (HERE / "icon.ico").write_bytes(ico(full))
    icons = HERE / "icons"
    icons.mkdir(exist_ok=True)
    for size in LINUX_SIZES:
        (icons / f"{size}x{size}.png").write_bytes(png(sized(full, size)))
    print(f"wrote icon.icns, icon.ico and icons/ from {source}")


if __name__ == "__main__":
    main()
