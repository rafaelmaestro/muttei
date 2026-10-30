import json
import math
from pathlib import Path

ROOT = Path(__file__).parent
OUT = ROOT / "logo"
WORD = json.loads((ROOT / "src" / "wordmark.json").read_text())
WORD_BBOX = (52, -786, 3558, 10)  # JetBrains Mono Bold "muttei", font units, y down

INK = "#282a36"
PAPER = "#f8f8f2"
RED = "#ff5555"

REGULAR = dict(top=16, cr=42, length=46, knob=22, uy=112, ri=66, w=20, stem_to=220, base_w=88)
REVERSED = dict(REGULAR, w=19, knob=23)
SMALL = dict(top=12, cr=46, length=36, knob=24, uy=108, ri=58, w=28, stem_to=214, base_w=100)


def f(v):
    return f"{v:.2f}".rstrip("0").rstrip(".")


def mark(top, cr, length, knob, uy, ri, w, stem_to, base_w):
    cx, cy = 128, top + cr + length
    capsule = (
        f"M{cx} {top}a{cr} {cr} 0 0 1 {cr} {cr}v{length}a{cr} {cr} 0 0 1-{2 * cr} 0V{top + cr}a{cr} {cr} 0 0 1 {cr}-{cr}Z"
        f"M{cx} {cy - knob}a{knob} {knob} 0 1 0 0 {2 * knob}a{knob} {knob} 0 1 0 0-{2 * knob}Z"
    )
    ro, hw = ri + w, w / 2
    dy = math.sqrt(ro * ro - hw * hw)
    base_l, base_r = cx - base_w / 2, cx + base_w / 2
    stand = (
        f"M{cx - ro} {uy}h{w}a{ri} {ri} 0 0 0 {2 * ri} 0h{w}"
        f"a{ro} {ro} 0 0 1-{f(ro - hw)} {f(dy)}V{stem_to}H{f(base_r)}v{w}H{f(base_l)}v-{w}H{f(cx - hw)}V{f(uy + dy)}"
        f"A{ro} {ro} 0 0 1 {cx - ro} {uy}Z"
    )
    return capsule, (cx, cy, knob), stand


def symbol(geo, ink, knob_fill=RED):
    capsule, (kx, ky, kr), stand = mark(**geo)
    dot = f'<circle fill="{knob_fill}" cx="{kx}" cy="{ky}" r="{kr}"/>' if knob_fill else ""
    return f'<path fill="{ink}" fill-rule="evenodd" d="{capsule}"/>{dot}<path fill="{ink}" d="{stand}"/>'


def wordmark(ink, ei, x, baseline, xh):
    s = xh / WORD["xh"]
    return (
        f'<g transform="translate({f(x - WORD_BBOX[0] * s)} {f(baseline)}) scale({s:.5f})">'
        f'<path fill="{ink}" d="{WORD["mutt"]}"/><path fill="{ei}" d="{WORD["ei"]}"/></g>'
    )


def svg(w, h, body, title="Muttei"):
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {f(w)} {f(h)}"><title>{title}</title>{body}</svg>\n'


def word_size(xh):
    s = xh / WORD["xh"]
    return (WORD_BBOX[2] - WORD_BBOX[0]) * s, -WORD_BBOX[1] * s


def horizontal(geo, ink, ei, knob):
    xh, gap = 96, 36
    ww, _ = word_size(xh)
    return svg(256 + gap + ww, 256, symbol(geo, ink, knob) + wordmark(ink, ei, 256 + gap, 176, xh))


def stacked(geo, ink, ei, knob):
    xh, gap = 64, 28
    ww, asc = word_size(xh)
    width = max(256, ww)
    sym = f'<g transform="translate({f((width - 256) / 2)} 0)">{symbol(geo, ink, knob)}</g>'
    return svg(width, 256 + gap + asc + 2, sym + wordmark(ink, ei, (width - ww) / 2, 256 + gap + asc, xh))


def word_only(ink, ei):
    xh = 96
    ww, asc = word_size(xh)
    return svg(ww, asc + 2, wordmark(ink, ei, 0, asc, xh))


def app_icon():
    inner = symbol(REGULAR, PAPER)
    return svg(256, 256, f'<rect width="256" height="256" rx="56" fill="{INK}"/><g transform="translate(38.4 36) scale(.7)">{inner}</g>')


def app_icon_maskable():
    inner = symbol(REGULAR, PAPER)
    return svg(256, 256, f'<rect width="256" height="256" fill="{INK}"/><g transform="translate(57.6 56) scale(.55)">{inner}</g>')


def tray(muted):
    if not muted:
        return svg(256, 256, symbol(SMALL, PAPER, knob_fill=None), "Muttei: no ar")
    inner = symbol(SMALL, "#ffffff", knob_fill=None)
    return svg(256, 256, f'<circle cx="128" cy="128" r="128" fill="{RED}"/><g transform="translate(51.2 51.2) scale(.6)">{inner}</g>', "Muttei: mudo")


FILES = {
    "symbol.svg": svg(256, 256, symbol(REGULAR, INK)),
    "symbol-dark.svg": svg(256, 256, symbol(REVERSED, PAPER)),
    "symbol-black.svg": svg(256, 256, symbol(REGULAR, "#000000", None)),
    "symbol-white.svg": svg(256, 256, symbol(REVERSED, "#ffffff", None)),
    "symbol-small.svg": svg(256, 256, symbol(SMALL, INK)),
    "symbol-small-dark.svg": svg(256, 256, symbol(SMALL, PAPER)),
    "horizontal.svg": horizontal(REGULAR, INK, RED, RED),
    "horizontal-dark.svg": horizontal(REVERSED, PAPER, RED, RED),
    "horizontal-black.svg": horizontal(REGULAR, "#000000", "#000000", None),
    "horizontal-white.svg": horizontal(REVERSED, "#ffffff", "#ffffff", None),
    "stacked.svg": stacked(REGULAR, INK, RED, RED),
    "stacked-dark.svg": stacked(REVERSED, PAPER, RED, RED),
    "wordmark.svg": word_only(INK, RED),
    "wordmark-dark.svg": word_only(PAPER, RED),
    "app-icon.svg": app_icon(),
    "app-icon-maskable.svg": app_icon_maskable(),
    "tray.svg": tray(False),
    "tray-muted.svg": tray(True),
}

if __name__ == "__main__":
    OUT.mkdir(exist_ok=True)
    for name, body in FILES.items():
        (OUT / name).write_text(body)
    print(f"{len(FILES)} files in {OUT}")
