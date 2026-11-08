"""Digitizes cherry blossom petals from free photos of whole flowers (CC0), each flower marked by hand: its middle and how
far from it its petals reach at most, so a neighbour stays out. Writes each petal's edge as its reach from the
flower's middle across the petal, from one neighbour to the other, and each flower's colours from the petals' base out
to their tips.

The photos (Wikimedia Commons, at their 1280 px thumbnails), saved as the names here:
  00.jpg  Cherry blossom flower.jpg (CC0)
  02.jpg  Pink Cherry Blossom.jpg (CC0), its colours only: its flowers overlap too much to take their outlines

    python3 -I scripts/leaves/petals.py <photos> <out>   # <out>/petals.json and <out>/petals.png to check them
"""
import json
import math
import sys

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

ANGLES = 720
SAMPLES = 32

FLOWERS = [
    # file, kind, name, middle, furthest a petal reaches, whether its petals' outlines are clean enough to take
    ("00.jpg", "white", "white", (500, 395), 215, True),
    ("02.jpg", "pink", "pink", (378, 540), 160, False),
    ("02.jpg", "pink", "pink-upper", (472, 268), 150, False),
]


def mask_of(a, kind):
    r, g, b = a[..., 0], a[..., 1], a[..., 2]
    mx = a.max(-1)
    if kind == "white":
        return mx > 0.55
    # Pink against a blue sky: redder than it is blue, and bright, unlike the twigs.
    return (r > b + 0.04) & (r > 0.5) & (mx > 0.5)


def main(src, out):
    result, sheet = [], []
    for file, kind, name, middle, furthest, outlines in FLOWERS:
        img = Image.open(f"{src}/{file}").convert("RGB")
        img.thumbnail((1000, 1000))
        a = np.asarray(img, dtype=np.float32) / 255
        mask = mask_of(a, kind)
        mask = np.asarray(Image.fromarray(mask.astype(np.uint8) * 255).filter(ImageFilter.MaxFilter(3)).filter(ImageFilter.MinFilter(3))) > 0
        h, w = mask.shape
        cx, cy = middle
        # The edge round the flower: how far out along each ray it reaches, the mask unbroken from the middle.
        reach = np.zeros(ANGLES)
        for i in range(ANGLES):
            t = 2 * math.pi * i / ANGLES
            dx, dy = math.cos(t), math.sin(t)
            for s in np.arange(furthest, 0, -0.5):
                x, y = int(round(cx + dx * s)), int(round(cy + dy * s))
                if 0 <= x < w and 0 <= y < h and mask[y, x]:
                    reach[i] = s
                    break
        # A ray now and then runs out to something behind the flower: the median of its neighbours stands in.
        reach = np.array([np.median(reach[(i + np.arange(-6, 7)) % ANGLES]) for i in range(ANGLES)])
        smooth = np.array([reach[(i + np.arange(-4, 5)) % ANGLES].mean() for i in range(ANGLES)])
        # The petals' tips: the five furthest reaches, at least 50 degrees apart.
        order = np.argsort(-smooth)
        tips = []
        for i in order:
            if all(min(abs(i - j), ANGLES - abs(i - j)) > ANGLES * 50 / 360 for j in tips):
                tips.append(int(i))
            if len(tips) == 5:
                break
        tips.sort()
        # Each petal runs from the dip between it and one neighbour to the dip between it and the other.
        dips = []
        for k in range(5):
            a0, a1 = tips[k], tips[(k + 1) % 5] + (ANGLES if k == 4 else 0)
            span = [(j % ANGLES) for j in range(a0, a1 + 1)]
            dips.append(span[int(np.argmin(smooth[span]))])
        scale = reach.max()
        petals = []
        for k in range(5):
            left, tip, right = dips[k - 1], tips[k], dips[k]
            if right < left:
                right += ANGLES
            if tip < left:
                tip += ANGLES
            # Across the petal from neighbour to neighbour, the tip in the middle of the samples.
            angles = np.concatenate([np.linspace(left, tip, SAMPLES // 2, endpoint=False), np.linspace(tip, right, SAMPLES // 2)])
            edge = np.interp(angles % ANGLES, np.arange(ANGLES + 1), np.append(reach, reach[0])) / reach[tip % ANGLES]
            half = [(tip - left) * 2 * math.pi / ANGLES, (right - tip) * 2 * math.pi / ANGLES]
            petals.append({"edge": edge.round(4).tolist(), "half": [round(v, 4) for v in half], "length": round(float(reach[tip % ANGLES] / scale), 4)})
        # Colours from the petals' base out to their tips, the stamens' dark and deep pink left out.
        yy, xx = np.mgrid[0:h, 0:w]
        dist = np.hypot(xx - cx, yy - cy) / scale
        bright = a.max(-1)
        petal_px = mask & (bright > (0.62 if kind == "white" else 0.5)) & (dist < 1.0)
        rings = [np.median(a[petal_px & (dist >= lo) & (dist < hi)], axis=0).round(3).tolist() for lo, hi in [(0.22, 0.45), (0.45, 0.72), (0.72, 1.0)]]
        result.append({"name": name, "petals": petals if outlines else [], "rings": rings})
        prev = img.copy()
        dr = ImageDraw.Draw(prev)
        pts = [(cx + math.cos(2 * math.pi * i / ANGLES) * reach[i], cy + math.sin(2 * math.pi * i / ANGLES) * reach[i]) for i in range(ANGLES)]
        dr.line(pts + [pts[0]], fill=(0, 200, 255), width=3)
        for t in tips:
            dr.line([(cx, cy), (cx + math.cos(2 * math.pi * t / ANGLES) * reach[t], cy + math.sin(2 * math.pi * t / ANGLES) * reach[t])], fill=(255, 255, 0), width=2)
        for d in dips:
            dr.line([(cx, cy), (cx + math.cos(2 * math.pi * d / ANGLES) * reach[d], cy + math.sin(2 * math.pi * d / ANGLES) * reach[d])], fill=(255, 0, 255), width=2)
        prev = prev.crop((cx - furthest, cy - furthest, cx + furthest, cy + furthest)).resize((320, 320))
        sheet.append(prev)
    canvas = Image.new("RGB", (330 * len(sheet), 320), (40, 40, 40))
    for k, p in enumerate(sheet):
        canvas.paste(p, (330 * k, 0))
    canvas.save(f"{out}/petals.png")
    json.dump(result, open(f"{out}/petals.json", "w"), indent=1)


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
