#!/usr/bin/env python3
"""Draw the xmas-tui app icon: a Christmas tree made of terminal glyphs in a
dark terminal tile. Writes assets/icon/xmas-tui-<size>.png, xmas-tui.ico and
xmas-tui.png (512). Small sizes are drawn as pixel art (glyphs don't survive
16 px). Needs Pillow:  python3 packaging/icons/make-icons.py"""
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont, ImageFilter

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "assets" / "icon"
MONO = str(ROOT / "assets" / "fonts" / "DejaVuSansMono.ttf")
SANS = str(ROOT / "assets" / "fonts" / "DejaVuSans.ttf")

BG_TOP, BG_BOT = (16, 22, 48), (8, 11, 26)
BORDER = (58, 76, 130)
GREENS = [(30, 120, 60), (44, 150, 74), (62, 176, 90)]
GOLD = (255, 205, 80)
RED, BLUE, PINK = (240, 70, 70), (90, 150, 255), (255, 120, 200)
TRUNK = (130, 80, 40)
SNOW = (235, 240, 255)


def tile(n, radius):
    """The rounded dark terminal tile (n x n, supersampled 4x)."""
    s = 4
    big = Image.new("RGBA", (n * s, n * s), (0, 0, 0, 0))
    grad = Image.new("RGBA", (n * s, n * s))
    gd = ImageDraw.Draw(grad)
    for y in range(n * s):
        k = y / (n * s - 1)
        c = tuple(int(BG_TOP[i] + (BG_BOT[i] - BG_TOP[i]) * k) for i in range(3))
        gd.line([(0, y), (n * s, y)], fill=c + (255,))
    mask = Image.new("L", (n * s, n * s), 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, n * s - 1, n * s - 1], radius * s, fill=255)
    big.paste(grad, (0, 0), mask)
    d = ImageDraw.Draw(big)
    w = max(1, n * s // 96)
    d.rounded_rectangle([w // 2, w // 2, n * s - 1 - w // 2, n * s - 1 - w // 2], radius * s, outline=BORDER + (255,), width=w)
    return big.resize((n, n), Image.LANCZOS)


def glyph_icon(n):
    """Large sizes: the tree drawn with real glyphs (▲ ● ★ █)."""
    im = tile(n, n * 0.19)
    # a title bar, like a terminal window
    d = ImageDraw.Draw(im)
    r = n * 0.022
    for i, c in enumerate([(255, 95, 86), (255, 189, 46), (39, 201, 63)]):
        cx, cy = n * (0.12 + i * 0.065), n * 0.10
        d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=c + (255,))
    d.line([(n * 0.06, n * 0.165), (n * 0.94, n * 0.165)], fill=BORDER + (160,), width=max(1, n // 256))
    rows = [
        "    ★    ",
        "   ▲▲▲   ",
        "  ▲●▲▲▲  ",
        " ▲▲▲▲●▲▲ ",
        "▲●▲▲▲▲▲●▲",
        "    █    ",
    ]
    ornament = {(2, 3): RED, (3, 6): GOLD, (4, 1): BLUE, (4, 7): PINK}
    size = int(n * 0.118)
    font = ImageFont.truetype(MONO, size)
    star_font = ImageFont.truetype(SANS, int(size * 1.25))
    cw = font.getlength("▲")
    lh = size * 0.96
    x0 = (n - cw * 9) / 2
    y0 = n * 0.215
    layer = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    ld = ImageDraw.Draw(layer)
    for ry, row in enumerate(rows):
        for cx, ch in enumerate(row):
            if ch == " ":
                continue
            x, y = x0 + cx * cw, y0 + ry * lh
            if ch == "★":
                ld.text((x + cw / 2, y + lh * 0.55), ch, font=star_font, fill=GOLD + (255,), anchor="mm")
            elif ch == "●":
                ld.text((x, y), ch, font=font, fill=ornament.get((ry, cx), RED) + (255,))
            elif ch == "█":
                ld.text((x, y), ch, font=font, fill=TRUNK + (255,))
            else:
                ld.text((x, y), ch, font=font, fill=GREENS[(cx + ry) % 3] + (255,))
    # a soft glow around the star and the ornaments
    glow = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    gd = ImageDraw.Draw(glow)
    sx, sy = x0 + 4.5 * cw, y0 + lh * 0.55
    gd.ellipse([sx - n * 0.09, sy - n * 0.09, sx + n * 0.09, sy + n * 0.09], fill=GOLD + (110,))
    glow = glow.filter(ImageFilter.GaussianBlur(n * 0.05))
    im = Image.alpha_composite(im, glow)
    im = Image.alpha_composite(im, layer)
    # a little snow along the bottom
    d = ImageDraw.Draw(im)
    for i, (fx, fy) in enumerate([(0.15, 0.30), (0.84, 0.36), (0.10, 0.62), (0.88, 0.70), (0.22, 0.85), (0.79, 0.88)]):
        r = n * (0.010 + 0.004 * (i % 2))
        d.ellipse([n * fx - r, n * fy - r, n * fx + r, n * fy + r], fill=SNOW + (200,))
    return im


# small sizes: hand-drawn pixel trees ('.' tile, 'g'/'G' greens, 'y' star,
# 'r'/'b'/'o' ornaments, 't' trunk)
PIX16 = [
    "................",
    "................",
    ".......y........",
    "......yyy.......",
    ".......g........",
    "......gGg.......",
    ".....gGrGg......",
    "......gGg.......",
    ".....gGbGGg.....",
    "....gGGGGoGg....",
    ".....gGGgGg.....",
    "....grGGGGbGg...",
    "...gGGGoGGGGGg..",
    ".......tt.......",
    ".......tt.......",
    "................",
]
PIX32 = [
    "................................",
    "................................",
    "................................",
    "...............y................",
    "..............yyy...............",
    "............yyyyyyy.............",
    "..............yyy...............",
    ".............yy.yy..............",
    "...............g................",
    "..............gGg...............",
    ".............gGGGg..............",
    "............gGrGGGg.............",
    "...........gGGGGGbGg............",
    ".............gGGGg..............",
    "............gGGGGGg.............",
    "...........gGGoGGGGg............",
    "..........gGGGGGGrGGg...........",
    ".........gGbGGGGGGGGGg..........",
    "...........gGGGGGGGg............",
    "..........gGGGGGoGGGg...........",
    ".........gGrGGGGGGGGGg..........",
    "........gGGGGGGGbGGGGGg.........",
    ".......gGGGoGGGGGGGGrGGg........",
    "......gGGGGGGGGGGGGGGGGGg.......",
    "..............ttt...............",
    "..............ttt...............",
    "..............ttt...............",
    "................................",
    "................................",
    "................................",
    "................................",
    "................................",
]
PAL = {"g": GREENS[0], "G": GREENS[2], "y": GOLD, "r": RED, "b": BLUE, "o": GOLD, "t": TRUNK}


def pixel_icon(n, art):
    im = tile(n, n * 0.2)
    k = n // len(art)
    off = (n - k * len(art)) // 2
    d = ImageDraw.Draw(im)
    for y, row in enumerate(art):
        for x, ch in enumerate(row):
            if ch in PAL:
                d.rectangle([off + x * k, off + y * k, off + x * k + k - 1, off + y * k + k - 1], fill=PAL[ch] + (255,))
    return im


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    imgs = {}
    for n in (16, 24, 32, 48, 64, 128, 256, 512):
        if n <= 24:
            im = pixel_icon(16, PIX16).resize((n, n), Image.NEAREST) if n != 16 else pixel_icon(16, PIX16)
        elif n <= 48:
            im = pixel_icon(32, PIX32) if n == 32 else pixel_icon(32, PIX32).resize((n, n), Image.NEAREST if n % 32 == 0 else Image.LANCZOS)
        else:
            im = glyph_icon(n)
        imgs[n] = im
        im.save(OUT / f"xmas-tui-{n}.png")
    imgs[512].save(OUT / "xmas-tui.png")
    imgs[256].save(OUT / "xmas-tui.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)], append_images=[imgs[s] for s in (16, 24, 32, 48, 64, 128)])
    print("wrote", ", ".join(str(n) for n in imgs), "→", OUT)


if __name__ == "__main__":
    main()
