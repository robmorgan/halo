"""Build crates/halo/assets/fonts/HaloIcons.ttf: Halo's own icon glyphs.

egui draws icons as font glyphs, so Halo's icons that no font has live
here, in the Supplementary Private Use Area-A (U+F0000 on: Inter fills
most of the BMP's Private Use Area with its own glyphs), drawn on a
14 x 14 pixel grid as rectangles. `theme::install_fonts` puts the font after each family's own
faces, ahead of egui's emoji fonts (which use the Private Use Area
too), so a key label like "\\u{F0000}  MAP" just works.

Usage: uv run --no-project --with fonttools python scripts/make-icon-font.py

To add an icon: add its rectangles to ICONS under the next code point,
rerun, and name the code point in theme.rs.
"""

from pathlib import Path

from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen

# Each icon: its code point, glyph name, and rectangles (x, y, w, h) on
# a 14 x 14 grid, y down, as in an SVG.
ICONS = [
    # MAP: the channel map, blocks of channels in rows.
    (0xF0000, "map", [(1, 1.5, 7, 3), (9, 1.5, 4, 3), (1, 5.5, 3, 3), (5, 5.5, 8, 3), (1, 9.5, 5, 3)]),
    # LAYOUT: a truss over fixtures spread across the stage.
    (0xF0001, "layout", [(1, 1, 12, 1.5), (2, 4, 3, 3), (9, 4, 3, 3), (5.5, 7, 3, 3), (1, 10, 3, 3), (10, 10, 3, 3)]),
]

EM = 1000
# The icons are drawn 14 px tall beside 12 px key labels: 14/12 of the em,
# centred on the labels' cap height (about 0.7 em).
GRID = 14
UNIT = EM * 14 / 12 / GRID
TOP = round(0.35 * EM + GRID * UNIT / 2)
ADVANCE = round(GRID * UNIT)


def glyph(rects):
    pen = TTGlyphPen(None)
    for x, y, w, h in rects:
        x0, x1 = round(x * UNIT), round((x + w) * UNIT)
        y1, y0 = round(TOP - y * UNIT), round(TOP - (y + h) * UNIT)
        # Clockwise, as TrueType wants outer contours.
        pen.moveTo((x0, y1))
        pen.lineTo((x1, y1))
        pen.lineTo((x1, y0))
        pen.lineTo((x0, y0))
        pen.closePath()
    return pen.glyph()


def main():
    names = [".notdef"] + [name for _, name, _ in ICONS]
    fb = FontBuilder(EM, isTTF=True)
    fb.setupGlyphOrder(names)
    fb.setupCharacterMap({code: name for code, name, _ in ICONS})
    glyphs = {".notdef": TTGlyphPen(None).glyph()}
    glyphs.update({name: glyph(rects) for _, name, rects in ICONS})
    fb.setupGlyf(glyphs)
    metrics = {name: (ADVANCE, 0) for name in names}
    metrics[".notdef"] = (0, 0)
    fb.setupHorizontalMetrics(metrics)
    descent = TOP - ADVANCE
    fb.setupHorizontalHeader(ascent=TOP, descent=descent)
    fb.setupNameTable({"familyName": "Halo Icons", "styleName": "Regular"})
    fb.setupOS2(
        sTypoAscender=TOP,
        sTypoDescender=descent,
        usWinAscent=TOP,
        usWinDescent=-descent,
    )
    fb.setupPost()
    out = Path(__file__).resolve().parent.parent / "crates/halo/assets/fonts/HaloIcons.ttf"
    fb.save(out)
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
