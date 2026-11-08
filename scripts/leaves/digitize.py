"""Digitizes leaves cut from free photos, each marked by hand: a point inside its blade, where its stalk joins the
blade and its tip. Writes each blade's reach from near its base at evenly spaced angles from the
tip, as a share of the furthest, and its colour from the middle out to the edge."""
# The photos, all free to use without credit (Wikimedia Commons, at their 1280 px thumbnails), saved as the names here:
#   09.jpg  Oak leaves with autumn coloration.jpg (CC0)
#   05.jpg  Canadian Maple Leaf.JPG (public domain)
#   12.jpg  Red maple leaf on a white background.jpg (CC0)
#   03.jpg  SNF Tofte FC102213 fallen leaf on snow (10427181996).jpg (public domain, US Forest Service)
#   06.jpg  Leaf leaves branch maple autumn nightshot studio.jpg (public domain)
#
#     python3 -I scripts/leaves/digitize.py <photos> <out>   # <out>/leaves.json and <out>/sheet.png to check them
#     python3 -I scripts/leaves/shapes.py <out>/leaves.json > linux/src/shaders/leaves.glsl
import json
import math
import sys
from collections import deque

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

SAMPLES = 96

LEAVES = [
    # file, kind, name, inside, attach, tip, hole (centre, radius)
    ("09.jpg", "white", "oak-green", (340, 200), (380, 440), (330, 35), ((380, 470), 48)),
    ("09.jpg", "white", "oak-brown", (150, 490), (355, 475), (10, 490), ((380, 470), 48)),
    ("09.jpg", "white", "oak-yellow", (600, 460), (405, 465), (745, 465), ((380, 470), 48)),
    ("09.jpg", "white", "oak-long", (380, 750), (385, 500), (395, 945), ((380, 470), 48)),
    ("05.jpg", "red", "maple-sharp", (420, 400), (440, 588), (290, 28), None),
    ("12.jpg", "red", "maple-red", (500, 350), (497, 455), (490, 165), None),
    ("03.jpg", "snow", "aspen", (520, 360), (660, 300), (340, 395), None),
    ("06.jpg", "black", "maple-field", (330, 640), (318, 600), (328, 690), None),
]


def mask_of(a, kind):
    r, g, b = a[..., 0], a[..., 1], a[..., 2]
    mx, mn = a.max(-1), a.min(-1)
    sat = (mx - mn) / np.maximum(mx, 1e-3)
    if kind == "red":
        return (r > 0.22) & (r > g * 1.35) & (sat > 0.3)
    if kind == "snow":
        return (sat > 0.2) & (b < np.maximum(r, g) * 0.97) & (mx > 0.2)
    if kind == "black":
        return mx > 0.22
    return (sat > 0.18) | (mx < 0.62)


def morph(m, size, op):
    f = ImageFilter.MinFilter(size) if op == "min" else ImageFilter.MaxFilter(size)
    return np.asarray(Image.fromarray(m.astype(np.uint8) * 255).filter(f)) > 0


def flood(m, seed):
    h, w = m.shape
    out = np.zeros_like(m)
    x0, y0 = seed
    if not m[y0, x0]:
        ys, xs = np.nonzero(m)
        k = np.argmin((xs - x0) ** 2 + (ys - y0) ** 2)
        x0, y0 = xs[k], ys[k]
    q = deque([(y0, x0)])
    out[y0, x0] = True
    while q:
        y, x = q.popleft()
        for dy, dx in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            yy, xx = y + dy, x + dx
            if 0 <= yy < h and 0 <= xx < w and m[yy, xx] and not out[yy, xx]:
                out[yy, xx] = True
                q.append((yy, xx))
    return out


def main(src, outdir):
    result, previews = [], []
    for file, kind, name, inside, attach, tip, hole in LEAVES:
        img = Image.open(f"{src}/{file}").convert("RGB")
        img.thumbnail((1000, 1000))
        a = np.asarray(img, dtype=np.float32) / 255
        h, w = a.shape[:2]
        mask = morph(morph(mask_of(a, kind), 3, "max"), 3, "min")
        if hole:
            (cx, cy), r = hole
            yy, xx = np.mgrid[0:h, 0:w]
            mask &= (yy - cy) ** 2 + (xx - cx) ** 2 > r * r
            # The leaves lie in a cross, a quarter turn apart: each keeps to its own quarter round the middle.
            ang = np.arctan2(yy - cy, xx - cx)
            toward = math.atan2(tip[1] - cy, tip[0] - cx)
            mask &= np.abs((ang - toward + math.pi) % (2 * math.pi) - math.pi) < math.radians(44)
        # Stalks and twigs are thinner than this: opening the mask leaves only blades.
        opened = morph(morph(mask, 9, "min"), 9, "max")
        blade = flood(opened, inside)
        attach, tip = np.array(attach, float), np.array(tip, float)
        length = np.linalg.norm(tip - attach)
        axis = (tip - attach) / length
        origin = attach + axis * length * 0.12
        reach = []
        for i in range(SAMPLES):
            t = 2 * math.pi * i / SAMPLES
            d = np.array([axis[0] * math.cos(t) - axis[1] * math.sin(t), axis[0] * math.sin(t) + axis[1] * math.cos(t)])
            far = 0.0
            for s in np.arange(0, length * 1.6, 0.5):
                x, y = origin + d * s
                xi, yi = int(round(x)), int(round(y))
                if 0 <= xi < w and 0 <= yi < h and blade[yi, xi]:
                    far = s
            reach.append(far)
        reach = np.array(reach)
        scale = reach.max()
        dist = np.hypot(np.arange(w)[None, :] - origin[0], np.arange(h)[:, None] - origin[1]) / scale
        rings = [np.median(a[blade & (dist >= lo) & (dist < hi)], axis=0).round(3).tolist() for lo, hi in [(0, 0.35), (0.35, 0.7), (0.7, 1.3)]]
        result.append({"name": name, "reach": (reach / scale).round(4).tolist(), "rings": rings,
                       "base": round(float(np.linalg.norm(origin - attach) / scale), 4)})
        prev = img.copy()
        dr = ImageDraw.Draw(prev)
        pts = []
        for i, r in enumerate(reach):
            t = 2 * math.pi * i / SAMPLES
            d = np.array([axis[0] * math.cos(t) - axis[1] * math.sin(t), axis[0] * math.sin(t) + axis[1] * math.cos(t)])
            pts.append(tuple(origin + d * r))
        dr.line(pts + [pts[0]], fill=(0, 140, 255), width=4)
        dr.line([tuple(attach), tuple(tip)], fill=(255, 0, 255), width=3)
        ys, xs = np.nonzero(blade)
        pad = 30
        prev = prev.crop((max(0, xs.min() - pad), max(0, ys.min() - pad), min(w, xs.max() + pad), min(h, ys.max() + pad)))
        prev.thumbnail((260, 260))
        canvas = Image.new("RGB", (260, 260), (40, 40, 40))
        canvas.paste(prev, ((260 - prev.width) // 2, (260 - prev.height) // 2))
        previews.append(canvas)
    sheet = Image.new("RGB", (260 * len(previews), 260 + 280), (40, 40, 40))
    for k, p in enumerate(previews):
        sheet.paste(p, (260 * k, 0))
        l = result[k]
        dr = ImageDraw.Draw(sheet)
        cx, cy = 130 + 260 * k, 260 + 150
        pts = [(cx - math.sin(2 * math.pi * i / SAMPLES) * r * 115, cy - math.cos(2 * math.pi * i / SAMPLES) * r * 115) for i, r in enumerate(l["reach"])]
        dr.polygon(pts, fill=tuple(int(255 * c) for c in l["rings"][1]), outline=(255, 255, 255))
        dr.line([(cx, cy + l["base"] * 115), (cx, cy + l["base"] * 115 + 30)], fill=(255, 0, 255), width=3)
        dr.text((cx - 50, 268), l["name"], fill=(255, 255, 255))
    sheet.save(f"{outdir}/sheet.png")
    json.dump(result, open(f"{outdir}/leaves.json", "w"), indent=1)


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
