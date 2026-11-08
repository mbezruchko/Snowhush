"""Turns the digitized leaves (`leaves.json`, from `digitize.py`) into the tables the shaders draw them from: each
blade's reach at evenly spaced angles from its tip, where its middle and stalk sit in the sprite, the angles of its
lobes for the veins, and its colours from the middle out to the edge.

    python3 -I scripts/leaves/shapes.py leaves.json glsl petals.json > linux/src/shaders/leaves.glsl
    python3 -I scripts/leaves/shapes.py leaves.json hlsl petals.json > windows/src/leaves.hlsl
    python3 -I scripts/leaves/shapes.py leaves.json metal petals.json > Sources/Snowhush/LeafShapes.swift

With `petals.json` (from `petals.py`) it writes the cherry blossom petals' tables too: the edge of each petal's tip,
across it from one shoulder to the other, as a share of its length, and each flower's colours from base to tip.
"""
import json
import math
import sys

# A pinnate leaf's veins branch off its midrib; a palmate one's spread from its base to the tips of its lobes.
PINNATE = ("oak", "aspen")

# How often each flower's colours show on a petal: mostly the palest, the deep pink rarely.
PETAL_ODDS = {"white": 0.5, "pink-upper": 0.35, "pink": 0.15}
PETAL_SAMPLES = 32

# How often each leaf falls: mostly golds, yellows and browns, as autumn mostly is; the red maples rarely, as they
# catch the eye far beyond their number.
ODDS = {
    "oak-green": 0.10,
    "oak-brown": 0.15,
    "oak-yellow": 0.15,
    "oak-long": 0.15,
    "maple-sharp": 0.06,
    "maple-red": 0.06,
    "aspen": 0.13,
    "maple-field": 0.20,
}


def frame(leaf):
    reach, base = leaf["reach"], leaf["base"]
    n = len(reach)
    pts = [(base + r * math.cos(2 * math.pi * i / n), r * math.sin(2 * math.pi * i / n)) for i, r in enumerate(reach)]
    # The stalk reaches back from the base: keep room for it in the sprite.
    xs = [p[0] for p in pts] + [-0.3]
    centre = (min(xs) + max(xs)) / 2
    scale = 0.97 / max(math.hypot(x - centre, y) for x, y in pts + [(-0.3, 0.0)])
    return centre, scale


def lobes(reach):
    n = len(reach)
    smooth = [sum(reach[(i + k) % n] for k in range(-2, 3)) / 5 for i in range(n)]
    tips = []
    for i in range(n):
        if smooth[i] > 0.42 and smooth[i] >= max(smooth[(i + k) % n] for k in range(-6, 7)):
            angle = 2 * math.pi * i / n
            angle = angle - 2 * math.pi if angle > math.pi else angle
            if all(abs(angle - t) > 0.4 for t in tips):
                tips.append(angle)
    tips.sort(key=lambda a: abs(a))
    return (tips + [9.0] * 6)[:6]


def fmt(v):
    return f"{v:.4f}".rstrip("0").rstrip(".") if v != int(v) else f"{int(v)}.0"


def main(path):
    leaves = json.load(open(path))
    n = len(leaves[0]["reach"])
    reach, frames, rings, veins = [], [], [], []
    for leaf in leaves:
        r = list(leaf["reach"])
        # Toward the stalk the rays can run down a stub of it left in the photo: keep the blade's own base there.
        window = math.radians(14 if leaf["name"].startswith(PINNATE) else 28)
        lo = int(round((math.pi - window) / (2 * math.pi) * n))
        hi = int(round((math.pi + window) / (2 * math.pi) * n)) % n
        for i in range(n):
            if abs(2 * math.pi * i / n - math.pi) < window:
                r[i] = min(r[i], max(r[lo], r[hi]) * 0.5 + min(r[lo], r[hi]) * 0.5)
        # A leaf still green in the photo: turned to the olive gold oaks go in autumn.
        colours = leaf["rings"]
        if leaf["name"] == "oak-green":
            gold = [0.72, 0.6, 0.22]
            colours = [[c * 0.4 + g * 0.6 for c, g in zip(ring, gold)] for ring in colours]
        # A crimson so blue it read as cherry blossom among the leaves: turned to scarlet.
        if leaf["name"] == "maple-red":
            colours = [[r, g + 0.06, b * 0.25] for r, g, b in colours]
        leaf = {**leaf, "rings": colours}
        centre, scale = frame(leaf)
        reach += [v * scale for v in r]
        pinnate = 1.0 if leaf["name"].startswith(PINNATE) else 0.0
        frames.append((leaf["base"] - centre) * scale)
        frames.append(-centre * scale)
        frames.append(pinnate)
        frames.append(scale)
        rings += leaf["rings"]
        veins += lobes(r)
    names = ", ".join(l["name"] for l in leaves)
    total, odds = 0.0, []
    for leaf in leaves:
        total += ODDS[leaf["name"]]
        odds.append(total)
    odds = [v / total for v in odds]
    lang = sys.argv[2] if len(sys.argv) > 2 else "glsl"
    vec2, vec3, vec4 = {"glsl": ("vec2", "vec3", "vec4"), "metal": ("float2", "float3", "float4"), "hlsl": ("float2", "float3", "float4")}[lang]

    def array(kind, name, values):
        if lang == "glsl":
            return f"const {kind} {name}[{len(values)}] = {kind}[](" + ", ".join(values) + ");"
        prefix = "constant" if lang == "metal" else "static const"
        return f"{prefix} {kind} {name}[{len(values)}] = {{" + ", ".join(values) + "};"

    const_int = "const int" if lang == "glsl" else ("constant int" if lang == "metal" else "static const int")
    const_float = const_int.replace("int", "float")
    fn = "" if lang == "glsl" else "static "
    out = [
        "// Leaves cut from free photos (CC0 and public domain) and digitized by scripts/leaves: " + names + ".",
        "// Written by scripts/leaves/shapes.py; not edited by hand.",
        f"{const_int} LEAF_KINDS = {len(leaves)};",
        f"{const_int} LEAF_SAMPLES = {n};",
        "// Each blade's reach from its middle, at evenly spaced angles from its tip, in the leaf's own coordinates.",
        array("float", "LEAF_REACH", [fmt(v) for v in reach]),
        "// Each blade's middle along its length, where its stalk joins it, whether its veins branch off the midrib, and",
        "// how much of the drawing a unit of the photo takes.",
        array(vec4, "LEAF_FRAME", [f"{vec4}(" + ", ".join(fmt(v) for v in frames[4 * k:4 * k + 4]) + ")" for k in range(len(leaves))]),
        "// Its colour in the middle, half way out and at the edge.",
        array(vec3, "LEAF_RINGS", [f"{vec3}(" + ", ".join(fmt(c) for c in ring) + ")" for ring in rings]),
        "// How far each blade reaches at most from its middle, and toward its tip: past the first nothing of it shows.",
        array(vec2, "LEAF_EXTENT", [f"{vec2}({fmt(max(reach[k * n:(k + 1) * n]))}, {fmt(reach[k * n])})" for k in range(len(leaves))]),
        "// No leaf reaches further than this from where it lies, in its own sizes, squared: a cell's leaf further off",
        "// can't cover the point, and its outline isn't looked up.",
        f"{const_float} LEAF_BOUND = {fmt(round(max((max(reach[k * n:(k + 1) * n]) + abs(frames[4 * k])) ** 2 for k in range(len(leaves))) * 1.05, 3))};",
        "// The directions of a palmate blade's main veins, from its base toward its lobes; none past its last lobe.",
        array(vec2, "LEAF_VEINS", [f"{vec2}({fmt(math.cos(a))}, {fmt(math.sin(a))})" if a < 8 else f"{vec2}(0.0, 0.0)" for a in veins]),
        "// How often each leaf falls, added up: a seed below the first draws the first, and so on.",
        array("float", "LEAF_ODDS", [fmt(v) for v in odds]),
        "// The angles from the tip toward a palmate blade's lobes, which its main veins follow; 9 for none.",
        array("float", "LEAF_LOBES", [fmt(v) for v in veins]),
        "",
        "/// How far a digitized blade reaches from its middle at `angle` from its tip.",
        f"{fn}float leafReach(int kind, float angle) {{",
        "    float f = fract(angle / 6.2831853) * float(LEAF_SAMPLES);" if lang != "hlsl" else "    float f = frac(angle / 6.2831853) * float(LEAF_SAMPLES);",
        "    int i = int(floor(f));",
        "    int j = i + 1 == LEAF_SAMPLES ? 0 : i + 1;",
        ("    return mix(" if lang != "hlsl" else "    return lerp(") + "LEAF_REACH[kind * LEAF_SAMPLES + i], LEAF_REACH[kind * LEAF_SAMPLES + j], f - floor(f));",
        "}",
        "",
        "/// Which of the digitized leaves a seed draws, each as often as `LEAF_ODDS` says.",
        f"{fn}int leafKind(float seed) {{",
        ("    float f = fract(seed * 13.7);" if lang != "hlsl" else "    float f = frac(seed * 13.7);"),
        "    for (int k = 0; k < LEAF_KINDS - 1; k++) {",
        "        if (f < LEAF_ODDS[k]) return k;",
        "    }",
        "    return LEAF_KINDS - 1;",
        "}",
    ]
    if len(sys.argv) > 3:
        flowers = json.load(open(sys.argv[3]))
        edges, tones, odds_p, total = [], [], [], 0.0
        for flower in flowers:
            for petal in flower["petals"]:
                e = petal["edge"]
                # A ray now and then strays; the median of its neighbours stands in, and the tip is the longest.
                e = [sorted(e[max(0, i - 2):i + 3])[len(e[max(0, i - 2):i + 3]) // 2] for i in range(len(e))]
                # Then smoothed over a few samples: the photo's grain, not the petal's, made the rest.
                e = [sum(e[max(0, i - 2):i + 3]) / len(e[max(0, i - 2):i + 3]) for i in range(len(e))]
                # Toward its shoulders the photo shows the dip between it and its neighbour, not its own edge: there the
                # petal's own body shapes it.
                n_ = len(e)
                def loose(i):
                    u = abs(2 * i / (n_ - 1) - 1)
                    k = min(1.0, max(0.0, (u - 0.45) / 0.35))
                    return k * k * (3 - 2 * k)
                e = [min(v + (1.0 - v) * loose(i), 1.0) for i, v in enumerate(e)]
                edges.append(e)
                edges.append(list(reversed(e)))
            # The photos are darker than the petals look: brought up so the tip is nearly white, the hue kept.
            lift = 0.97 / max(flower["rings"][2])
            # Under an overcast sky the white flower's photo turned lilac: warmed back toward the blush it has.
            warm = [1.0, 0.98, 0.93] if flower["name"] == "white" else [1.0, 1.0, 1.0]
            tones += [[min(c * lift * w, 1.0) for c, w in zip(ring, warm)] for ring in flower["rings"]]
            total += PETAL_ODDS[flower["name"]]
            odds_p.append(total)
        odds_p = [v / total for v in odds_p]
        frac_ = "fract" if lang != "hlsl" else "frac"
        mix_ = "mix" if lang != "hlsl" else "lerp"
        out += [
            "",
            "// The cherry blossom petals' tips, digitized: each one's edge across it from shoulder to shoulder, as a share",
            "// of the petal's length from its base, each taken both ways round.",
            f"{const_int} PETAL_SHAPES = {len(edges)};",
            f"{const_int} PETAL_SAMPLES = {PETAL_SAMPLES};",
            array("float", "PETAL_EDGE", [fmt(v) for e in edges for v in e]),
            f"{const_int} PETAL_TONES = {len(flowers)};",
            "// Each flower's colour at the base of its petals, half way out and at the tips.",
            array(vec3, "PETAL_RINGS", [f"{vec3}(" + ", ".join(fmt(c) for c in ring) + ")" for ring in tones]),
            "// How often each flower's colours show, added up.",
            array("float", "PETAL_ODDS", [fmt(v) for v in odds_p]),
            "",
            "/// How far along a digitized petal its tip reaches at `across`, from -1 at one shoulder to 1 at the other.",
            f"{fn}float petalEdge(int shape, float across) {{",
            "    float f = clamp(across * 0.5 + 0.5, 0.0, 1.0) * float(PETAL_SAMPLES - 1);",
            "    int i = min(int(floor(f)), PETAL_SAMPLES - 2);",
            f"    return {mix_}(PETAL_EDGE[shape * PETAL_SAMPLES + i], PETAL_EDGE[shape * PETAL_SAMPLES + i + 1], f - float(i));",
            "}",
            "",
            "/// Which of the digitized petals a seed draws.",
            f"{fn}int petalShape(float seed) {{",
            f"    return min(int({frac_}(seed * 5.3) * float(PETAL_SHAPES)), PETAL_SHAPES - 1);",
            "}",
            "",
            "/// Whose colours a seed's petal takes, each flower's as often as `PETAL_ODDS` says.",
            f"{fn}int petalTone(float seed) {{",
            f"    float f = {frac_}(seed * 7.13);",
            "    for (int k = 0; k < PETAL_TONES - 1; k++) {",
            "        if (f < PETAL_ODDS[k]) return k;",
            "    }",
            "    return PETAL_TONES - 1;",
            "}",
        ]
    text = "\n".join(out)
    if lang == "metal":
        text = "// Written by scripts/leaves/shapes.py; not edited by hand.\n/// The digitized leaves' tables, put into `shaderSource` ahead of the leaves' drawing.\nlet leafShapesSource = \"\"\"\n" + text + "\n\"\"\"\n"
    print(text)


if __name__ == "__main__":
    main(sys.argv[1])
