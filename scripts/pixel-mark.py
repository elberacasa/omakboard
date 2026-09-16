#!/usr/bin/env python3
"""Draws Omakboard's pixel mark and wordmark as an animated SVG, for the README.

  scripts/pixel-mark.py > docs/images/omakboard.svg

The keyboard is the same 16 × 16 sprite as plugin/PixelMark.qml. Colours follow the reader's
light or dark theme. One key presses and springs back, the way rapid trigger re-presses; it
stays up for reduced motion.
"""

# k outline and keys, p the key that presses (drawn up here; one row lower when pressed).
KEYBOARD = [
    "................",
    "....k...........",
    "....k...........",
    "....k...........",
    ".kkkkkkkkkkkkkk.",
    ".k............k.",
    ".k.k.k.k.k.k..k.",
    ".k............k.",
    ".k..k.p.k.k.k.k.",
    ".k............k.",
    ".k.k.kkkkkk.k.k.",
    ".k............k.",
    ".kkkkkkkkkkkkkk.",
    "................",
    "................",
    "................",
]

# Lowercase on a 9-row grid: rows 0-1 ascender, 2-6 x-height, 7-8 descender.
GLYPHS = {
    "o": ["....", "....", ".##.", "#..#", "#..#", "#..#", ".##.", "....", "...."],
    "m": [".....", ".....", "##.#.", "#.#.#", "#.#.#", "#.#.#", "#.#.#", ".....", "....."],
    "a": ["....", "....", ".##.", "...#", ".###", "#..#", ".###", "....", "...."],
    "k": ["#...", "#...", "#..#", "#.#.", "##..", "#.#.", "#..#", "....", "...."],
    "b": ["#...", "#...", "###.", "#..#", "#..#", "#..#", "###.", "....", "...."],
    "r": ["....", "....", "#.##", "##..", "#...", "#...", "#...", "....", "...."],
    "d": ["...#", "...#", ".###", "#..#", "#..#", "#..#", ".###", "....", "...."],
}
WORD = "omakboard"
SCALE = 8
WORD_X = 18
WORD_Y = 5


def rect(x, y, cls="ink"):
    return f'<rect class="{cls}" x="{x}" y="{y}" width="1" height="1"/>'


def main():
    cells = []
    for y, row in enumerate(KEYBOARD):
        for x, ch in enumerate(row):
            if ch == "k":
                cells.append(rect(x, y))
            elif ch == "p":
                cells.append(rect(x, y, "key up"))
                cells.append(rect(x, y + 1, "key down"))

    x = WORD_X
    for letter in WORD:
        glyph = GLYPHS[letter]
        for y, row in enumerate(glyph):
            for dx, ch in enumerate(row):
                if ch == "#":
                    cells.append(rect(x + dx, WORD_Y + y))
        x += len(glyph[0]) + 1
    # One column of margin after the last letter.
    width = x

    print(f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} 16" width="{width * SCALE}" height="{16 * SCALE}" shape-rendering="crispEdges" role="img" aria-label="Omakboard">
<style>
  .ink {{ fill: #171A21; }}
  .key {{ fill: #15917F; }}
  @media (prefers-color-scheme: dark) {{
    .ink {{ fill: #E4E7EC; }}
    .key {{ fill: #3CC7B5; }}
  }}
  .up {{ animation: press 0.9s step-end infinite; }}
  .down {{ animation: press 0.9s step-end infinite reverse; }}
  @keyframes press {{ 0% {{ opacity: 1; }} 70% {{ opacity: 0; }} }}
  @media (prefers-reduced-motion: reduce) {{
    .up, .down {{ animation: none; }}
    .down {{ opacity: 0; }}
  }}
</style>
{chr(10).join(cells)}
</svg>''')


if __name__ == "__main__":
    main()
