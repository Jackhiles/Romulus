"""Draw the artwork Romulus gives its Steam shortcut.

    python tools/steam-art/make.py path/to/Inter-Bold.ttf

Inter (SIL Open Font License) is the UI's font: https://github.com/rsms/inter.
Writes src-tauri/icons/steam/{capsule,wide,hero,logo}.png; the shortcut's icon is
src-tauri/icons/icon.png. Steam's sizes: capsule 600x900, wide 920x430, hero 1920x620,
logo up to 1280x720 on a transparent background.
"""

import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "src-tauri" / "icons" / "steam"
ICON = Image.open(ROOT / "src-tauri" / "icons" / "icon.png").convert("RGBA")

BG = (18, 20, 27)  # the icon's frame
WARM = (251, 115, 71)  # the icon's gradient, top left
DEEP = (198, 31, 89)  # bottom right
NAME = "Romulus"


def mix(a, b, t):
    return tuple(round(x + (y - x) * t) for x, y in zip(a, b))


def gradient(w, h, a, b):
    """Diagonal gradient from a (top left) to b (bottom right)."""
    small = Image.new("RGB", (64, 64))
    px = small.load()
    for y in range(64):
        for x in range(64):
            px[x, y] = mix(a, b, (x + y) / 126)
    return small.resize((w, h), Image.BICUBIC)


def glow(w, h, cx, cy, r, color, alpha):
    """A soft coloured light at (cx, cy) on a transparent layer."""
    layer = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    ImageDraw.Draw(layer).ellipse((cx - r, cy - r, cx + r, cy + r), fill=color + (alpha,))
    return layer.filter(ImageFilter.GaussianBlur(r * 0.6))


def tiles(w, h, size, gap, fade_from):
    """Faint cover outlines in staggered rows, fading in from fade_from (0..1 of w)."""
    layer = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    step = size + gap
    tall = size * 4 // 3
    for row, y in enumerate(range(-tall // 2, h, tall + gap)):
        shift = (row % 2) * step // 2
        for x in range(-step + shift, w, step):
            t = (x + size / 2) / w
            t = 1.0 if fade_from <= 0 else max(0.0, (t - fade_from) / (1 - fade_from))
            a = round(30 * min(t, 1) ** 1.4)
            if a:
                d.rounded_rectangle((x, y, x + size, y + tall), size // 7, outline=(255, 255, 255, a), width=3)
    return layer


def icon(size):
    return ICON.resize((size, size), Image.LANCZOS)


def shadowed(base, im, xy, blur=24, alpha=150):
    sh = Image.new("RGBA", base.size, (0, 0, 0, 0))
    mask = im.getchannel("A").point(lambda v: v * alpha // 255)
    sh.paste((0, 0, 0, 255), (xy[0], xy[1] + blur // 2), mask)
    base.alpha_composite(sh.filter(ImageFilter.GaussianBlur(blur)))
    base.alpha_composite(im, xy)


def text(draw, xy, s, font, anchor="la"):
    draw.text(xy, s, font=font, fill=(255, 255, 255), anchor=anchor)


def background(w, h, light_x, light_y):
    im = Image.new("RGBA", (w, h), BG + (255,))
    r = max(w, h)
    cx, cy = int(w * light_x), int(h * light_y)
    im.alpha_composite(glow(w, h, cx, cy, r // 2, DEEP, 150))
    im.alpha_composite(glow(w, h, cx, cy, r // 4, WARM, 120))
    return im


def capsule(font_path):
    w, h = 600, 900
    im = background(w, h, 0.5, 0.36)
    im.alpha_composite(tiles(w, h, 120, 36, 0))
    shadowed(im, icon(300), ((w - 300) // 2, 170))
    d = ImageDraw.Draw(im)
    text(d, (w // 2, 600), NAME, ImageFont.truetype(font_path, 104), "mm")
    im.paste(gradient(160, 8, WARM, DEEP), ((w - 160) // 2, 680))
    return im.convert("RGB")


def wide(font_path):
    w, h = 920, 430
    im = background(w, h, 0.2, 0.5)
    im.alpha_composite(tiles(w, h, 90, 28, 0.35))
    shadowed(im, icon(230), (90, (h - 230) // 2))
    d = ImageDraw.Draw(im)
    text(d, (370, h // 2 + 4), NAME, ImageFont.truetype(font_path, 112), "lm")
    return im.convert("RGB")


def hero():
    w, h = 1920, 620
    im = background(w, h, 0.78, 0.55)
    im.alpha_composite(tiles(w, h, 150, 44, 0.25))
    # A big tilted tile echoing the icon, off to the right.
    big = gradient(560, 560, WARM, DEEP).convert("RGBA")
    mask = Image.new("L", big.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, 559, 559), 110, fill=255)
    big.putalpha(mask)
    big = big.rotate(-12, resample=Image.BICUBIC, expand=True)
    # Keep the left side calm: Steam puts the logo there. The fade's alpha gets a little
    # noise, or its 8-bit steps show as vertical bands.
    fade = Image.linear_gradient("L").rotate(-90).resize((w, h), Image.BILINEAR)
    noise = Image.effect_noise((w, h), 40)
    fade = Image.blend(fade, noise, 0.05).point(lambda v: round(v * 0.6))
    calm = Image.new("RGBA", (w, h), BG + (255,))
    calm.putalpha(fade)
    im.alpha_composite(calm)
    shadowed(im, big, (1330, 90), blur=40, alpha=180)
    return im.convert("RGB")


def logo(font_path):
    w, h = 1280, 360
    im = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    font = ImageFont.truetype(font_path, 210)
    d = ImageDraw.Draw(im)
    x0, _, x1, _ = d.textbbox((0, 0), NAME, font=font, anchor="lm")
    size, gap = 280, 56
    left = (w - (size + gap + x1 - x0)) // 2
    shadowed(im, icon(size), (left, (h - size) // 2), blur=18, alpha=140)
    tx = left + size + gap
    shade = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    ImageDraw.Draw(shade).text((tx, h // 2 + 8), NAME, font=font, fill=(0, 0, 0, 150), anchor="lm")
    im.alpha_composite(shade.filter(ImageFilter.GaussianBlur(10)))
    text(ImageDraw.Draw(im), (tx, h // 2), NAME, font, "lm")
    return im


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    font = sys.argv[1]
    OUT.mkdir(parents=True, exist_ok=True)
    for name, im in [("capsule", capsule(font)), ("wide", wide(font)), ("hero", hero()), ("logo", logo(font))]:
        im.save(OUT / f"{name}.png", optimize=True)
        print(name, im.size, (OUT / f"{name}.png").stat().st_size // 1024, "KB")


if __name__ == "__main__":
    main()
