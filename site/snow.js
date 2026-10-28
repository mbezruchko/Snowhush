(() => {
  // The same model and look as the app (SnowSimulation, SnowLayer, Shaders), ported to WebGL2.
  // Coordinates are CSS pixels with y pointing down; heights are measured up from each layer's bed.
  const COLUMN = 2;
  const REPOSE = 0.8;
  const ROUNDING = 0.15;
  const GRAVITY = 2500;
  const GRAIN_DRAG = 0.8;
  const CURSOR_RADIUS = 10;
  const CURSOR_OFFSET = [4, 8];
  const GROUND_DEPTH = 40;
  const LEDGE_DEPTH = 14;
  const THAW_DEPTH = 8;
  const LEDGE_THAW_DEPTH = 4;
  const THAW_RATE = 0.02;
  const THAW_TIME = 30;
  const THAW_BALANCE = 184;
  const THAW_EXPONENT = 0.74;
  // Snow builds up half again as fast as in the app, so a visitor sees a setting change the drift sooner;
  // it melts as much faster, so the drifts level off as high as in the app.
  const BUILDUP = 1.5;
  const DEPOSITION = 1.1 * BUILDUP;
  const MAX_GRAINS = 4000;
  const MAX_MOTES = 800;
  const PHASE_WRAP = 20 * Math.PI;
  const GUST_SWEEP = 0.6;
  const GUST_LENGTH = 2.2;
  const GUST_WIND = 1000;
  // The snowfall is caught this long after the drifts, so it trails behind their powder.
  const GUST_LAG = 0.3;
  const DENSITY = { off: 0, light: 150, steady: 380, heavy: 800, variable: 380 };
  const reduceMotion = matchMedia('(prefers-reduced-motion: reduce)');

  const settings = {
    snowfall: 'steady',
    theme: 'snow',
    themeAuto: false,
    accumulate: true,
    melt: true,
    windows: true,
    behind: false,
    screenshots: false,
    login: false,
    quit: false,
  };
  const variation = { strength: 1, target: 1, next: 0, clock: 0 };

  const THEME_INDEX = { snow: 0, leaves: 1, blossom: 2, candy: 3, drops: 4 };
  const THEME_SHAPE = { snow: 0, leaves: 1, blossom: 2, candy: 3, drops: 4 };
  const THEME_PALETTES = {
    leaves: [[0.78, 0.36, 0.14], [0.86, 0.55, 0.16], [0.64, 0.26, 0.16], [0.60, 0.52, 0.24], [0.72, 0.22, 0.16], [0.82, 0.66, 0.24]],
    blossom: [[0.96, 0.68, 0.77], [0.92, 0.55, 0.66], [0.98, 0.76, 0.83], [0.88, 0.58, 0.67]],
    candy: [[0.74, 0.50, 0.82], [0.94, 0.62, 0.30], [0.92, 0.80, 0.44], [0.56, 0.72, 0.86]],
    drops: [[0.66, 0.80, 0.93], [0.74, 0.86, 0.96], [0.58, 0.74, 0.90]],
  };

  const seasonalTheme = () => {
    const month = new Date().getMonth() + 1;
    return month >= 3 && month <= 5 ? 'blossom' : month >= 6 && month <= 8 ? 'drops' : month >= 9 && month <= 11 ? 'leaves' : 'snow';
  };
  const resolvedTheme = () => (settings.themeAuto ? seasonalTheme() : settings.theme);
  const piles = () => settings.accumulate && resolvedTheme() !== 'drops';

  /// The colour and silhouette a particle of `theme` takes.
  function look(theme) {
    const palette = THEME_PALETTES[theme];
    const tint = palette ? palette[Math.floor(Math.random() * palette.length)] : [1, 1, 1];
    return { tint, amount: palette ? 1 : 0, shape: THEME_SHAPE[theme] };
  }

  /// How a particle of `theme` falls and looks.
  function styleFor(theme, depth) {
    const lookAt = look(theme);
    const styles = {
      snow: { size: 1.4 + depth * 4.4, sizeJitter: 1, fallSpeed: 16 + depth * 50, fallJitter: 10, alpha: 0.4 + depth * 0.5, soft: 0.75 - depth * 0.45, sway: 4 + depth * 10, swayRate: [0.5, 1.4] },
      leaves: { size: 2.6 + depth * 5.2, sizeJitter: 1.8, fallSpeed: 20 + depth * 42, fallJitter: 14, alpha: 0.62 + depth * 0.34, soft: 0.5 - depth * 0.28, sway: 12 + depth * 22, swayRate: [0.7, 1.7] },
      blossom: { size: 1.7 + depth * 3.2, sizeJitter: 1.2, fallSpeed: 13 + depth * 30, fallJitter: 9, alpha: 0.55 + depth * 0.4, soft: 0.65 - depth * 0.35, sway: 14 + depth * 20, swayRate: [0.8, 2.0] },
      candy: { size: 2.2 + depth * 4.0, sizeJitter: 1.4, fallSpeed: 36 + depth * 78, fallJitter: 18, alpha: 0.7 + depth * 0.28, soft: 0.4 - depth * 0.25, sway: 3 + depth * 6, swayRate: [0.4, 1.0] },
      drops: { size: 1.1 + depth * 2.4, sizeJitter: 0.6, fallSpeed: 260 + depth * 300, fallJitter: 90, alpha: 0.42 + depth * 0.45, soft: 0.35 - depth * 0.2, sway: 1 + depth * 3, swayRate: [0.9, 2.2] },
    };
    return { ...styles[theme], ...lookAt };
  }

  const rand = (a, b) => a + Math.random() * (b - a);
  const smoothstep = (a, b, x) => {
    const t = Math.min(Math.max((x - a) / (b - a), 0), 1);
    return t * t * (3 - 2 * t);
  };

  // MARK: - Layers

  class Layer {
    constructor(depth, open) {
      this.depth = depth;
      this.open = open;
      this.x = 0;
      this.y = 0;
      this.h = new Float32Array(0);
      this.bed = new Float32Array(0);
      this.cap = new Float32Array(0);
      this.held = new Uint8Array(0);
      this.spill = [0, 0];
      this.parity = false;
      this.top = 0;
      this.bedDepth = 0;
      this.filled = false;
      this.phases = [rand(0, Math.PI * 2), rand(0, Math.PI * 2)];
    }

    get count() { return this.h.length; }
    get width() { return this.h.length * COLUMN; }

    place(x, y, width, radius = 0, exponent = 2.5) {
      const count = Math.max(4, Math.floor(width / COLUMN));
      if (count !== this.h.length) {
        // Stretched, not cropped: a window that reflows wider keeps its whole cap instead of a bare stretch at one end.
        const old = this.h;
        this.h = Float32Array.from({ length: count }, (_, i) => {
          if (old.length === 0) return 0;
          const at = (i + 0.5) * old.length / count - 0.5;
          const a = Math.max(0, Math.min(old.length - 1, Math.floor(at)));
          const b = Math.min(old.length - 1, a + 1);
          return old[a] + (old[b] - old[a]) * Math.max(0, Math.min(1, at - a));
        });
        this.held = new Uint8Array(count);
        this.cap = Float32Array.from({ length: count }, (_, i) => {
          const px = i * COLUMN;
          return this.depth * (0.95 + 0.12 * Math.sin(px * 0.0105 + this.phases[0]) + 0.07 * Math.sin(px * 0.037 + this.phases[1]));
        });
        this.data = new Float32Array(count * 2);
      }
      this.x = x + (width - count * COLUMN) / 2;
      this.y = y;
      const span = count * COLUMN;
      this.bed = Float32Array.from({ length: count }, (_, i) => {
        if (radius <= 0) return 0;
        const inset = Math.min((i + 0.5) * COLUMN, span - (i + 0.5) * COLUMN);
        if (inset >= radius) return 0;
        const u = 1 - inset / radius;
        return radius * (1 - Math.pow(Math.max(0, 1 - Math.pow(u, exponent)), 1 / exponent));
      });
      this.bedDepth = Math.max(0, ...this.bed);
      if (!this.filled) {
        this.filled = true;
        for (let i = 0; i < count; i++) this.h[i] = this.cap[i] * (this.open ? 0.55 : 0.5);
        this.refreshTop();
        for (let k = 0; k < 30; k++) this.relax(REPOSE);
        this.spill = [0, 0];
      }
      this.refreshTop();
    }

    index(x) {
      const i = Math.floor((x - this.x) / COLUMN);
      return i >= 0 && i < this.h.length ? i : -1;
    }

    clamped(x) {
      return Math.min(Math.max(Math.floor((x - this.x) / COLUMN), 0), this.h.length - 1);
    }

    centerX(i) { return this.x + (i + 0.5) * COLUMN; }
    floorY(i) { return this.y + this.bed[i]; }
    surface(i) { return this.y + this.bed[i] - this.h[i]; }
    fill(i) { return Math.max(0, 1 - this.h[i] / this.cap[i]); }

    deposit(area, x) {
      if (area <= 0) return 0;
      const center = Math.floor((x - this.x) / COLUMN);
      const amount = area / COLUMN;
      let missed = 0;
      for (const [offset, weight] of [[-2, 0.0625], [-1, 0.25], [0, 0.375], [1, 0.25], [2, 0.0625]]) {
        const i = center + offset;
        if (i >= 0 && i < this.h.length) this.h[i] += amount * weight;
        else missed += area * weight;
      }
      return missed;
    }

    clear() {
      this.h.fill(0);
      this.top = 0;
    }

    refreshTop() {
      let top = 0;
      for (let i = 0; i < this.h.length; i++) top = Math.max(top, this.h[i]);
      this.top = top;
    }

    // Slumps slopes steeper than the angle of repose and rounds crests, measured on the snow surface over the bed,
    // so snow slides off rounded corners. Open layers shed what slides past their ends into `spill`.
    relax(repose) {
      const n = this.h.length;
      if (n < 4 || this.top <= 0) return;
      const h = this.h, bed = this.bed, held = this.held;
      const limit = repose * COLUMN;
      const forwardFirst = this.parity;
      this.parity = !this.parity;
      for (let pass = 0; pass < 6; pass++) {
        const forward = (pass % 2 === 0) === forwardFirst;
        for (let k = 0; k < n - 1; k++) {
          const i = forward ? k : n - 2 - k;
          if (held[i] || held[i + 1]) continue;
          const difference = (h[i] - bed[i]) - (h[i + 1] - bed[i + 1]);
          if (difference > limit + 0.01) {
            const t = Math.min((difference - limit) * 0.5, h[i]);
            h[i] -= t;
            h[i + 1] += t;
          } else if (difference < -limit - 0.01) {
            const t = Math.min((-difference - limit) * 0.5, h[i + 1]);
            h[i] += t;
            h[i + 1] -= t;
          }
        }
        if (this.open) {
          if (!held[0] && h[0] > limit + 0.01) {
            const t = (h[0] - limit) * 0.5;
            h[0] -= t;
            this.spill[0] += t * COLUMN;
          }
          if (!held[n - 1] && h[n - 1] > limit + 0.01) {
            const t = (h[n - 1] - limit) * 0.5;
            h[n - 1] -= t;
            this.spill[1] += t * COLUMN;
          }
        }
      }
      // Pulls each slope toward its neighbours' average: rounds crests and notches, leaves even slopes alone.
      for (let i = 1; i < n - 2; i++) {
        if (held[i - 1] || held[i] || held[i + 1] || held[i + 2]) continue;
        const s0 = h[i - 1] - bed[i - 1], s1 = h[i] - bed[i], s2 = h[i + 1] - bed[i + 1], s3 = h[i + 2] - bed[i + 2];
        const kink = (s1 - s2) - 0.5 * ((s0 - s1) + (s2 - s3));
        if (Math.abs(kink) <= 0.02) continue;
        const t = kink > 0 ? Math.min(kink * ROUNDING, h[i]) : Math.max(kink * ROUNDING, -h[i + 1]);
        h[i] -= t;
        h[i + 1] += t;
      }
    }

    // Snow down to `floor` melts slowly; anything heaped above it settles back down over THAW_TIME seconds.
    thaw(dt, floor) {
      for (let i = 0; i < this.h.length; i++) {
        const height = this.h[i];
        if (height <= 0) continue;
        this.h[i] = height - Math.min(height, (THAW_RATE + Math.max(0, height - floor) / THAW_TIME) * dt);
      }
    }

    // Surface then bed, both measured up from the base, as the pile shader reads them.
    write() {
      const n = this.h.length, h = this.h, data = this.data;
      for (let i = 0; i < n; i++) {
        const left = h[Math.max(i - 1, 0)];
        const right = h[Math.min(i + 1, n - 1)];
        data[i] = -this.bed[i] + 0.25 * left + 0.5 * h[i] + 0.25 * right;
        data[n + i] = -this.bed[i];
      }
      return data;
    }
  }

  // MARK: - Shaders

  const LIGHT = `
    uniform vec3 uLamp;
    float lampCone(vec2 p) {
      float below = max(p.y - uLamp.y, 0.0);
      float across = abs(p.x - uLamp.x) / (30.0 + below * 0.5);
      float cone = (1.0 - smoothstep(0.6, 1.05, across)) / (1.0 + below / 700.0);
      vec2 d = p - uLamp.xy;
      float halo = exp(-dot(d, d) / 28000.0);
      return p.y < uLamp.y - 30.0 ? halo : min(1.0, cone + halo * 0.8);
    }
  `;

  const SPRITE_VERTEX = `#version 300 es
    in vec2 aPosition;
    in vec4 aLook;
    in vec4 aTint;
    in float aShape;
    uniform vec2 uView;
    uniform float uScale;
    uniform float uTime;
    uniform int uNight;
    ${LIGHT}
    out float vAlpha;
    out float vSoft;
    out float vLight;
    out float vGrow;
    out float vLump;
    out vec4 vTint;
    out float vShape;
    out float vSpin;
    void main() {
      float light = uNight == 1 && uLamp.z > 0.5 ? lampCone(aPosition) : 0.0;
      // A lump's ragged edge reaches past its nominal size.
      float grow = (1.0 + 1.6 * light) * (aLook.w > 0.0 ? 1.3 : 1.0);
      float pixels = aLook.x * uScale;
      gl_Position = vec4(aPosition.x / uView.x * 2.0 - 1.0, 1.0 - aPosition.y / uView.y * 2.0, 0.0, 1.0);
      gl_PointSize = max(pixels * grow, 2.0);
      vAlpha = aLook.y * min(pixels / 2.0, 1.0);
      vSoft = aLook.z;
      vLight = light;
      vGrow = grow;
      vLump = aLook.w;
      vTint = aTint;
      vShape = aShape;
      // Leaves and petals tumble as they fall; a lump of snow keeps its own slow roll.
      float seed = fract(aPosition.x * 0.017 + aPosition.y * 0.031);
      int kind = int(aShape + 0.5);
      float turn = kind == 1 ? 0.8 : kind == 2 ? 1.25 : kind == 3 ? 0.5 : 0.0;
      // A drop hangs from its own weight and points straight up; the others tumble as they fall.
      vSpin = kind == 4 ? 0.0 : (aLook.w > 0.0 ? uTime * (aLook.w - 0.5) * 9.0 + aLook.w * 40.0 : uTime * turn * (0.7 + seed) + seed * 6.283);
    }
  `;

  const SPRITE_FRAGMENT = `#version 300 es
    precision highp float;
    precision highp int;
    in float vAlpha;
    in float vSoft;
    in float vLight;
    in float vGrow;
    in float vLump;
    in vec4 vTint;
    in float vShape;
    in float vSpin;
    uniform int uNight;
    uniform vec3 uLamp;
    out vec4 outColor;
    void main() {
      vec2 p = gl_PointCoord - 0.5;
      float r = length(p) * 2.0;
      float shading = 0.0;
      int kind = int(vShape + 0.5);
      // The particle's own axes, so a leaf or a petal turns as it falls. Sized to the particle, not to its point.
      float ca = cos(vSpin), sa = sin(vSpin);
      vec2 q = vec2(ca * p.x - sa * p.y, sa * p.x + ca * p.y) * 2.0;

      if (vLump > 0.0) {
        float angle = atan(p.y, p.x) + vSpin;
        r /= 1.0 + 0.14 * sin(3.0 * angle) + 0.08 * sin(5.0 * angle + vLump * 21.0) + 0.05 * sin(2.0 * angle + vLump * 9.0);
        shading = clamp(dot(p * 2.0, vec2(0.45, 0.6)), 0.0, 1.0) * 0.4;
      } else if (kind == 1) {
        // A leaf: a pointed blade on a short stem, with a faint midrib.
        vec2 blade = vec2(q.x, q.y - 0.18);
        float body = length(vec2(blade.x / (0.46 * (1.0 - 0.26 * blade.y)), blade.y / 0.80));
        float stem = length(vec2(q.x / 0.055, (q.y + 0.62) / 0.20));
        r = min(body, stem);
        shading = smoothstep(0.055, 0.0, abs(q.x)) * 0.24;
      } else if (kind == 2) {
        // A blossom petal: soft and rounded, with a small notch at the tip.
        float notch = 0.18 * smoothstep(0.30, 0.95, q.y) * smoothstep(0.22, 0.0, abs(q.x));
        r = length(vec2(q.x / 0.74, q.y / 1.02)) + notch;
        shading = smoothstep(-0.1, -0.9, q.y) * 0.10;
      } else if (kind == 3) {
        // A wrapped sweet: a round centre pinched into two crinkled ends.
        float ax = abs(q.x);
        float flap = mix(0.40, 0.11, smoothstep(0.24, 0.70, ax)) * (1.0 + 0.16 * sin(ax * 15.0));
        float wrapEnd = length(vec2(q.x / 0.72, q.y / max(flap, 0.02)));
        float body = length(q / vec2(0.34, 0.40));
        r = min(body, wrapEnd);
        shading = smoothstep(0.34, 0.02, length(q - vec2(-0.14, 0.14))) * 0.30;
      } else if (kind == 4) {
        // A drop: round below, drawn up to a point, with a small highlight.
        float width = mix(0.74, 0.24, smoothstep(-0.15, 1.0, q.y));
        r = length(vec2(q.x / width, q.y / 1.05));
        shading = smoothstep(0.30, 0.0, length(q - vec2(-0.18, -0.18))) * 0.34;
      } else if (kind == 5) {
        // A ripple: a thin ring spreading out from where a drop landed.
        r = abs(length(p) * 2.0 - 0.86) * 7.0;
      }

      float a = (1.0 - smoothstep(mix(0.6, 0.0, vSoft), 1.0, r)) * vAlpha;
      vec3 color = mix(vec3(0.965, 0.975, 0.995), vec3(0.62, 0.67, 0.77), clamp(smoothstep(0.35, 1.0, r) * 0.65 + shading, 0.0, 1.0));
      color = mix(color, vTint.rgb, vTint.a);
      if (uNight == 1) {
        vec3 warm = vec3(1.0, 0.9, 0.74);
        float ambient = uLamp.z > 0.5 ? 0.22 : 0.55;
        color = mix(color * vec3(0.82, 0.86, 0.94), warm, vLight * 0.8);
        a *= ambient + (1.0 - ambient) * vLight;
        float halo = 1.0 - smoothstep(0.0, 1.0, r);
        halo = halo * halo * vLight * 0.45 * vAlpha;
        vec3 premultiplied = color * a + warm * halo * (1.0 - a);
        float alpha = a + halo * (1.0 - a);
        if (alpha < 0.002) discard;
        outColor = vec4(premultiplied, alpha);
        return;
      }
      if (a < 0.002) discard;
      outColor = vec4(color * a, a);
    }
  `;

  const PILE_VERTEX = `#version 300 es
    uniform vec4 uRect;
    uniform vec2 uView;
    out vec2 vPoint;
    void main() {
      vec2 corner = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1));
      vec2 p = mix(uRect.xy, uRect.zw, corner);
      vPoint = p;
      gl_Position = vec4(p.x / uView.x * 2.0 - 1.0, 1.0 - p.y / uView.y * 2.0, 0.0, 1.0);
    }
  `;

  const PILE_FRAGMENT = `#version 300 es
    precision highp float;
    precision highp int;
    in vec2 vPoint;
    uniform sampler2D uHeights;
    uniform int uCount;
    uniform vec2 uOrigin;
    uniform float uScale;
    uniform float uTime;
    uniform float uWidth;
    uniform int uNight;
    uniform int uTheme;
    ${LIGHT}
    out vec4 outColor;

    float heightAt(int row, float x) {
      float f = x / ${COLUMN.toFixed(1)} - 0.5;
      float base = floor(f);
      int i = clamp(int(base), 0, uCount - 1);
      int j = min(i + 1, uCount - 1);
      return mix(texelFetch(uHeights, ivec2(i, row), 0).r, texelFetch(uHeights, ivec2(j, row), 0).r, clamp(f - base, 0.0, 1.0));
    }

    float hash(vec2 p) {
      p = fract(p * vec2(123.34, 456.21));
      p += dot(p, p + 45.32);
      return fract(p.x * p.y);
    }

    void main() {
      float x = vPoint.x - uOrigin.x;
      float y = uOrigin.y - vPoint.y;
      float level = heightAt(0, x);
      float bed = heightAt(1, x);
      float depth = level - y;
      float coverage = clamp(depth * uScale + 0.5, 0.0, 1.0)
        * clamp((y - bed) * uScale + 0.5, 0.0, 1.0)
        * clamp((level - bed) / 0.75, 0.0, 1.0);
      if (coverage <= 0.0) discard;

      float slope = (heightAt(0, x + 2.0) - heightAt(0, x - 2.0)) * 0.25;
      float light = dot(normalize(vec2(-slope, 1.0)), normalize(vec2(-0.5, 1.0)));

      vec3 surface;
      vec3 shadow;
      vec3 highlight;
      if (uTheme == 1) {
        shadow = vec3(0.32, 0.19, 0.10);
        surface = vec3(0.69, 0.42, 0.21);
        highlight = vec3(0.86, 0.62, 0.34);
      } else if (uTheme == 2) {
        shadow = vec3(0.72, 0.50, 0.60);
        surface = vec3(0.96, 0.82, 0.86);
        highlight = vec3(1.0, 0.93, 0.95);
      } else if (uTheme == 3) {
        shadow = vec3(0.44, 0.28, 0.50);
        surface = vec3(0.82, 0.64, 0.84);
        highlight = vec3(0.96, 0.86, 0.96);
      } else {
        shadow = vec3(0.72, 0.78, 0.88);
        surface = vec3(0.925, 0.94, 0.965);
        highlight = vec3(0.975, 0.982, 0.995);
      }
      float shade = (1.0 - exp(-depth / 30.0)) * 0.32 + (1.0 - light) * 0.6 * exp(-depth / 5.0);
      vec3 color = mix(surface, shadow, clamp(shade, 0.0, 1.0));
      color = mix(color, highlight, exp(-depth * 0.9) * 0.6);
      color += (hash(floor(gl_FragCoord.xy)) - 0.5) * 0.02;

      float seed = hash(floor(gl_FragCoord.xy * 0.5) + 31.7);
      float twinkle = 0.0;
      if (seed > 0.992 && (uTheme == 0 || uTheme == 4)) {
        twinkle = pow(clamp(sin(uTime * (0.6 + seed * 1.8) + seed * 90.0), 0.0, 1.0), 28.0) * exp(-depth / 7.0);
        color = mix(color, vec3(1.0), twinkle);
      }

      if (uNight == 1) {
        vec3 warm = vec3(1.0, 0.86, 0.66);
        if (uLamp.z > 0.5) {
          float spread = max(uWidth * 0.28, 300.0);
          float dx = (vPoint.x - uLamp.x) / spread;
          float pool = exp(-dx * dx) / (1.0 + max(vPoint.y - uLamp.y, 0.0) / 1400.0);
          color = color * (vec3(0.16, 0.18, 0.23) + warm * pool * 1.15) + twinkle * pool * 0.6;
        } else {
          color = color * vec3(0.42, 0.46, 0.54);
        }
      }
      outColor = vec4(min(color, vec3(1.0)) * coverage, coverage);
    }
  `;

  function program(gl, vertex, fragment) {
    const compile = (type, source) => {
      const shader = gl.createShader(type);
      gl.shaderSource(shader, source);
      gl.compileShader(shader);
      if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(shader));
      return shader;
    };
    const p = gl.createProgram();
    gl.attachShader(p, compile(gl.VERTEX_SHADER, vertex));
    gl.attachShader(p, compile(gl.FRAGMENT_SHADER, fragment));
    gl.linkProgram(p);
    if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(p));
    const uniforms = {};
    for (let i = 0; i < gl.getProgramParameter(p, gl.ACTIVE_UNIFORMS); i++) {
      const name = gl.getActiveUniform(p, i).name;
      uniforms[name] = gl.getUniformLocation(p, name);
    }
    return { program: p, uniforms };
  }

  // MARK: - Scene

  // One snowy section of the page: flakes, loose grains, a drift along its bottom and a cap on every
  // [data-ledge] element in it. Night sections are lit by their street lamp, or faintly without one.
  class Scene {
    constructor(section) {
      this.section = section;
      this.night = section.dataset.scene === 'night';
      this.canvas = section.querySelector('canvas.snow');
      this.lightCanvas = section.querySelector('canvas.night-light');
      this.lamp = section.querySelector('.lamp-head');
      this.light = null;
      this.ground = new Layer(GROUND_DEPTH, false);
      this.ledges = [...section.querySelectorAll('[data-ledge]')].map(element => ({ element, layer: new Layer(LEDGE_DEPTH, true) }));
      this.flakes = [];
      this.grains = [];
      this.motes = [];
      this.cursor = null;
      this.cursorVelocity = [0, 0];
      this.gust = null;
      this.time = rand(0, 100);
      this.theme = 'snow';
      this.visible = false;
      this.width = 0;
      this.height = 0;
      this.setUpGL();
    }

    setUpGL() {
      const gl = this.canvas.getContext('webgl2', { premultipliedAlpha: true, antialias: false });
      if (!gl) {
        this.canvas.hidden = true;
        return;
      }
      try {
        this.sprite = program(gl, SPRITE_VERTEX, SPRITE_FRAGMENT);
        this.pile = program(gl, PILE_VERTEX, PILE_FRAGMENT);
      } catch (error) {
        console.warn('Snowhush: snow disabled,', error);
        this.canvas.hidden = true;
        return;
      }
      this.gl = gl;
      this.spriteBuffer = gl.createBuffer();
      this.spriteVAO = gl.createVertexArray();
      gl.bindVertexArray(this.spriteVAO);
      gl.bindBuffer(gl.ARRAY_BUFFER, this.spriteBuffer);
      const position = gl.getAttribLocation(this.sprite.program, 'aPosition');
      const look = gl.getAttribLocation(this.sprite.program, 'aLook');
      const tint = gl.getAttribLocation(this.sprite.program, 'aTint');
      const shape = gl.getAttribLocation(this.sprite.program, 'aShape');
      gl.enableVertexAttribArray(position);
      gl.vertexAttribPointer(position, 2, gl.FLOAT, false, 44, 0);
      gl.enableVertexAttribArray(look);
      gl.vertexAttribPointer(look, 4, gl.FLOAT, false, 44, 8);
      gl.enableVertexAttribArray(tint);
      gl.vertexAttribPointer(tint, 4, gl.FLOAT, false, 44, 24);
      gl.enableVertexAttribArray(shape);
      gl.vertexAttribPointer(shape, 1, gl.FLOAT, false, 44, 40);
      gl.bindVertexArray(null);
      this.pileVAO = gl.createVertexArray();
      this.textures = new Map();
      this.spriteData = new Float32Array(0);
      gl.enable(gl.BLEND);
      gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
      gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
    }

    get layers() {
      return ledgesActive() ? [this.ground, ...this.ledges.map(ledge => ledge.layer)] : [this.ground];
    }

    measure() {
      const rect = this.section.getBoundingClientRect();
      const dpr = Math.min(window.devicePixelRatio || 1, 2);
      const first = this.width === 0;
      this.width = rect.width;
      this.height = rect.height;
      this.dpr = dpr;
      for (const canvas of [this.canvas, this.lightCanvas]) {
        if (!canvas) continue;
        canvas.width = Math.round(rect.width * dpr);
        canvas.height = Math.round(rect.height * dpr);
      }
      // Sunk a little below the edge so no sliver of background shows under the drift after rounding.
      this.ground.place(0, rect.height + 2, rect.width);
      for (const { element, layer } of this.ledges) {
        const box = element.getBoundingClientRect();
        layer.place(box.left - rect.left, box.top - rect.top, box.width, Number(element.dataset.ledge), 2.5);
      }
      if (first) {
        const target = this.target();
        while (this.flakes.length < target) this.flakes.push(this.makeFlake(rand(-20, this.height)));
      }
      if (this.lamp) {
        const lamp = this.lamp.getBoundingClientRect();
        this.light = { x: lamp.left + lamp.width / 2 - rect.left, y: lamp.top + lamp.height * 0.66 - rect.top };
        if (this.lightCanvas) this.paintLight();
      }
      this.draw();
    }

    target() {
      if (settings.quit) return 0;
      return Math.round(DENSITY[settings.snowfall] * variation.strength * this.width * this.height / 1e6);
    }

    wind() {
      return this.breeze() + this.gustSpeed();
    }

    breeze() {
      const t = this.time;
      const breeze = 10 * Math.sin(t * 0.07) + 6 * Math.sin(t * 0.19 + 1.7) + 3 * Math.sin(t * 0.43 + 4.1);
      return breeze * (0.6 + 0.4 * variation.strength);
    }

    makeFlake(y) {
      const theme = resolvedTheme();
      const near = theme === 'snow' && Math.random() < 0.02;
      const depth = near ? 1 : Math.pow(Math.random(), 1.6);
      const style = styleFor(theme, depth);
      const fall = near ? rand(75, 100) : style.fallSpeed + rand(0, style.fallJitter);
      return {
        x: rand(-30, this.width + 30),
        y,
        vx: this.wind() * (0.35 + 0.65 * depth),
        vy: fall,
        fall,
        size: near ? rand(8, 13) : style.size + rand(0, style.sizeJitter),
        depth,
        alpha: near ? 0.28 : style.alpha,
        soft: near ? 0.95 : style.soft,
        phase: rand(0, PHASE_WRAP),
        swayRate: rand(style.swayRate[0], style.swayRate[1]),
        swayAmount: near ? rand(10, 20) : style.sway,
        settles: !near,
        tint: style.tint,
        tintAmount: near ? 0 : style.amount,
        shape: style.shape,
      };
    }

    /// Recolours what is already falling after a change of theme, and clears the drifts for water.
    setTheme(theme) {
      this.theme = theme;
      for (const flake of this.flakes) {
        const style = styleFor(theme, flake.depth);
        flake.tint = style.tint;
        flake.tintAmount = theme === 'snow' ? 0 : 1;
        flake.soft = flake.settles ? style.soft : flake.soft;
        flake.shape = style.shape;
      }
      if (theme === 'drops') {
        this.grains.length = 0;
        this.motes.length = 0;
        this.ground.clear();
        for (const { layer } of this.ledges) layer.clear();
      }
      this.draw();
    }

    /// A drop striking a surface: a ring spreads where it hit and a few beads hop up. Nothing settles.
    splash(x, y, size) {
      if (this.motes.length >= MAX_MOTES) return;
      const force = Math.min(Math.max(size / 4, 0.4), 1.4);
      const { tint } = look('drops');
      // Sit the ring just inside the view, so a splash on the bottom edge still reads.
      const center = y - 3;
      // Not every drop leaves a ring, or a heavy fall would carpet the edge.
      if (Math.random() < 0.45) {
        this.motes.push({ x, y: center, vx: 0, vy: 0, size: Math.max(3.5, size * 1.6), alpha: 0.4 * force, age: 0, life: rand(0.3, 0.55), rises: true, grow: 3.4, tint, tintAmount: 1, shape: 5 });
      }
      const beads = Math.min(MAX_MOTES - this.motes.length, Math.floor(1 + force * 2));
      for (let k = 0; k < beads; k++) {
        const angle = rand(0.25, Math.PI - 0.25);
        const speed = rand(45, 120) * force;
        this.motes.push({
          x: x + rand(-1.5, 1.5),
          y: center,
          vx: Math.cos(angle) * speed,
          vy: -Math.sin(angle) * speed * 0.9,
          size: rand(1.2, 2.4) * force,
          alpha: rand(0.4, 0.65),
          age: 0,
          life: rand(0.24, 0.5),
          rises: false,
          grow: 0,
          tint,
          tintAmount: 1,
          shape: 4,
        });
      }
    }

    paintLight() {
      const ctx = this.lightCanvas.getContext('2d');
      const { x, y } = this.light;
      ctx.setTransform(this.dpr, 0, 0, this.dpr, 0, 0);
      ctx.clearRect(0, 0, this.width, this.height);
      const halo = ctx.createRadialGradient(x, y, 0, x, y, 440);
      halo.addColorStop(0, 'rgba(255,214,150,0.30)');
      halo.addColorStop(0.35, 'rgba(255,214,150,0.10)');
      halo.addColorStop(1, 'rgba(255,214,150,0)');
      ctx.fillStyle = halo;
      ctx.fillRect(0, 0, this.width, this.height);
      const spread = 30 + (this.height - y) * 0.5;
      const beam = ctx.createLinearGradient(0, y, 0, this.height);
      beam.addColorStop(0, 'rgba(255,220,165,0.22)');
      beam.addColorStop(0.85, 'rgba(255,220,165,0.03)');
      beam.addColorStop(1, 'rgba(255,220,165,0)');
      ctx.filter = 'blur(16px)';
      ctx.fillStyle = beam;
      ctx.beginPath();
      ctx.moveTo(x - 26, y);
      ctx.lineTo(x + 26, y);
      ctx.lineTo(x + spread, this.height);
      ctx.lineTo(x - spread, this.height);
      ctx.closePath();
      ctx.fill();
      ctx.filter = 'none';
    }

    // MARK: Simulation

    step(dt, pointer) {
      this.time += dt;
      this.trackCursor(pointer, dt);
      this.blow(dt);
      this.advanceFlakes(dt);
      this.advanceGrains(dt);
      this.advanceMotes(dt);
      this.settle(dt);
      // No new snow falls until the gust has carried off the last flake, then the snowfall starts again from the top.
      const target = this.blowing ? 0 : this.target();
      while (this.flakes.length < target) this.flakes.push(this.makeFlake(-rand(0, this.height * 0.8)));
      for (const layer of this.layers) layer.refreshTop();
    }

    /// The highest snow surface crossed while falling from `from` to `to` at `x`.
    landing(x, from, to) {
      let hit = null;
      let highest = Infinity;
      if (ledgesActive()) {
        for (const { layer } of this.ledges) {
          const i = layer.index(x);
          if (i < 0) continue;
          const surface = layer.surface(i);
          if (from <= layer.floorY(i) && to >= surface && surface < highest) {
            hit = { layer, i };
            highest = surface;
          }
        }
      }
      if (hit) return hit;
      const i = this.ground.clamped(x);
      return to >= this.ground.surface(i) ? { layer: this.ground, i } : null;
    }

    advanceFlakes(dt) {
      const wind = this.breeze();
      const follow = Math.min(1, dt * 1.2);
      const settle = Math.min(1, dt * 1.6);
      const target = this.blowing ? 0 : this.target();
      let wake = null;
      if (this.cursor) {
        let [wx, wy] = this.cursorVelocity;
        const speed = Math.hypot(wx, wy);
        if (speed > 500) {
          wx *= 500 / speed;
          wy *= 500 / speed;
        }
        if (speed > 20) wake = { x: this.cursor.x, y: this.cursor.y, vx: wx, vy: wy };
      }

      const flakes = this.flakes;
      for (let i = flakes.length - 1; i >= 0; i--) {
        const f = flakes[i];
        if (f.blown) {
          // The nearer the flake, the faster the gust seems to carry it.
          f.vx += (f.blown * GUST_WIND * (1.3 + 1.3 * f.depth) - f.vx) * Math.min(1, dt * 2.5);
        } else {
          f.vx += (wind * (0.35 + 0.65 * f.depth) - f.vx) * follow;
        }
        f.vy += (f.fall - f.vy) * settle;
        if (wake) {
          const dx = f.x - wake.x, dy = f.y - wake.y;
          const distance = Math.hypot(dx, dy);
          if (distance < 80) {
            const falloff = 1 - distance / 80;
            const k = Math.min(1, falloff * falloff * 4 * dt);
            f.vx += (wake.vx - f.vx) * k;
            f.vy += (wake.vy - f.vy) * k;
          }
        }
        f.phase += f.swayRate * dt;
        if (f.phase > PHASE_WRAP) f.phase -= PHASE_WRAP;
        const sway = (Math.cos(f.phase) + 0.3 * Math.cos(f.phase * 2.3)) * f.swayAmount;
        const flutter = 1 + 0.12 * Math.sin(f.phase * 1.3);
        const from = f.y;
        f.x += (f.vx + sway) * dt;
        f.y += f.vy * flutter * dt;
        // Carried off by a gust, a flake leaves for good instead of coming round again.
        let gone = Boolean(f.blown) && (f.x < -30 || f.x > this.width + 30);
        if (f.x < -30) f.x += this.width + 60;
        else if (f.x > this.width + 30) f.x -= this.width + 60;

        if (!gone && f.settles && !f.blown && resolvedTheme() === 'drops') {
          const hit = this.landing(f.x, from, f.y);
          if (hit) {
            this.splash(f.x, hit.layer.surface(hit.i), f.size);
            gone = true;
          }
        } else if (!gone && f.settles && piles() && !f.blown) {
          const hit = this.landing(f.x, from, f.y);
          if (hit) {
            const missed = hit.layer.deposit(f.size * f.size * DEPOSITION * hit.layer.fill(hit.i), f.x);
            this.dropPast(hit.layer, missed, f.x, 0);
            gone = true;
          }
        } else if (f.y > this.height + f.size * 2) {
          gone = true;
        }
        if (!gone) continue;
        if (flakes.length <= target) {
          flakes[i] = this.makeFlake(-rand(4, 40));
        } else {
          flakes[i] = flakes[flakes.length - 1];
          flakes.pop();
        }
      }
    }

    advanceGrains(dt) {
      const damping = Math.exp(-GRAIN_DRAG * dt);
      const grains = this.grains;
      for (let i = grains.length - 1; i >= 0; i--) {
        const g = grains[i];
        const from = g.y;
        if (g.blown) {
          // Light powder: the wind keeps it moving toward the edge it was blown to, and it barely sinks.
          g.vx += ((g.vx < 0 ? -GUST_WIND : GUST_WIND) * 1.6 - g.vx) * Math.min(1, dt * 2);
          g.vy += GRAVITY * 0.08 * dt;
        } else {
          g.vy += GRAVITY * dt;
          g.vx *= damping;
          g.vy *= damping;
        }
        g.x += g.vx * dt;
        g.y += g.vy * dt;
        let gone = g.x < -10 || g.x > this.width + 10 || g.y > this.height + 20;
        if (!gone && !g.blown && g.vy >= 0) {
          const hit = this.landing(g.x, from, g.y);
          if (hit && resolvedTheme() === 'drops') {
            this.splash(g.x, hit.layer.surface(hit.i), g.size);
            gone = true;
          } else if (hit && g.lump) {
            this.burst(g, hit.layer);
            gone = true;
          } else if (hit) {
            const missed = hit.layer.deposit(g.area, g.x);
            this.dropPast(hit.layer, missed, g.x, g.vx * 0.5);
            gone = true;
          }
        }
        if (gone) {
          grains[i] = grains[grains.length - 1];
          grains.pop();
        }
      }
    }

    advanceMotes(dt) {
      const damping = Math.exp(-3.2 * dt);
      const drift = this.wind() * 0.4;
      const motes = this.motes;
      for (let i = motes.length - 1; i >= 0; i--) {
        const m = motes[i];
        m.age += dt;
        if (m.age >= m.life) {
          motes[i] = motes[motes.length - 1];
          motes.pop();
          continue;
        }
        if (m.rises) {
          // A ripple spreads flat on the surface and leans nowhere.
          const damp = Math.exp(-7 * dt);
          m.vx *= damp;
          m.vy *= damp;
          m.x += m.vx * dt;
          m.y += m.vy * dt;
        } else {
          m.vy += 35 * dt;
          m.vx *= damping;
          m.vy *= damping;
          m.x += (m.vx + drift) * dt;
          m.y += m.vy * dt;
        }
      }
    }

    /// A few columns at a time hold together as a lump; the rest pours off as powder.
    throwLump(area, [x, y]) {
      const pieces = Math.floor(area / 90) + 1;
      const share = area / pieces;
      const { tint, amount: tintAmount, shape } = look(resolvedTheme());
      for (let k = 0; k < pieces && this.grains.length < MAX_GRAINS; k++) {
        this.grains.push({
          x: x + rand(-2, 2),
          y: y + rand(-1, 1),
          // Lumps break away a moment before the powder around them, so a falling sheet comes apart.
          vx: rand(-25, 25),
          vy: rand(0, 80),
          area: share,
          size: Math.min(11, Math.sqrt(share) * 1.05),
          lump: rand(0.05, 1),
          tint,
          tintAmount,
          shape,
        });
      }
    }

    /// A lump hitting the snow: most of it packs in where it lands, a few crumbs hop off and a puff of powder rises.
    burst(lump, layer) {
      const impact = Math.max(0, lump.vy);
      const force = Math.min(Math.max(impact / 900, 0.35), 1.3);
      const crumbs = lump.area * 0.18;
      for (const spot of [-0.3, 0, 0.3]) {
        const x = lump.x + spot * lump.size;
        this.dropPast(layer, layer.deposit((lump.area - crumbs) / 3, x), x, lump.vx * 0.3);
      }
      const surface = layer.surface(layer.clamped(lump.x));
      this.release(crumbs, [lump.x, surface - 1], [lump.vx * 0.3, -impact * rand(0.08, 0.16)], [lump.size * 0.4, 1], 0.6);
      const count = Math.min(MAX_MOTES - this.motes.length, Math.floor(2 + lump.size * 0.6 * force));
      const { tint, amount: tintAmount, shape } = look(resolvedTheme());
      for (let k = 0; k < count; k++) {
        const angle = rand(0.15, Math.PI - 0.15);
        const speed = rand(25, 110) * force;
        this.motes.push({
          x: lump.x + rand(-0.5, 0.5) * lump.size,
          y: surface - rand(0, lump.size * 0.4),
          vx: Math.cos(angle) * speed * 1.4 + lump.vx * 0.2,
          vy: -Math.sin(angle) * speed * 0.6,
          size: rand(0.3, 0.6) * lump.size + 1,
          alpha: rand(0.1, 0.22),
          age: 0,
          life: rand(0.45, 0.9),
          tint,
          tintAmount,
          shape,
        });
      }
    }

    settle(dt) {
      // Melting nearly keeps pace with the snowfall, so heavier snowfall builds drifts only a little higher.
      const falling = DENSITY[settings.snowfall] * variation.strength;
      const meltPace = falling > 0 ? BUILDUP * (falling / THAW_BALANCE) ** THAW_EXPONENT : 1;
      for (const layer of this.layers) {
        if (!piles() || settings.melt) {
          const floor = !piles() ? 0 : layer === this.ground ? THAW_DEPTH : LEDGE_THAW_DEPTH;
          layer.thaw(dt * meltPace, floor);
        }
        layer.relax(REPOSE);
        if (layer.spill[0] > 1.5) {
          this.release(layer.spill[0], this.edgePoint(layer, true), [-6, 15], [0.25, 1]);
          layer.spill[0] = 0;
        }
        if (layer.spill[1] > 1.5) {
          this.release(layer.spill[1], this.edgePoint(layer, false), [6, 15], [0.25, 1]);
          layer.spill[1] = 0;
        }
      }
    }

    release(area, [x, y], [vx, vy], [sx, sy] = [1, 1], scatter = 0) {
      if (area <= 0.02) return;
      const room = MAX_GRAINS - this.grains.length;
      if (room <= 0) {
        if (piles()) this.ground.deposit(area, x);
        return;
      }
      const count = Math.min(room, Math.max(1, Math.min(6, Math.floor(area / 3))));
      const share = area / count;
      const { tint, amount: tintAmount, shape } = look(resolvedTheme());
      for (let k = 0; k < count; k++) {
        const spread = 1 + rand(-scatter, scatter);
        this.grains.push({
          x: x + rand(-sx, sx),
          y: y - rand(0, sy),
          vx: vx * spread + rand(-12, 12) - 40 * scatter * rand(-1, 1),
          vy: vy * spread + rand(-8, 10),
          area: share,
          size: Math.min(5, Math.max(1.1, Math.sqrt(share) * 1.05)),
          tint,
          tintAmount,
          shape,
        });
      }
    }

    /// Snow that lands past the end of a ledge drops off that end instead.
    dropPast(layer, area, x, vx) {
      if (!layer.open || area <= 0.05) return;
      const left = x < layer.x + layer.width / 2;
      this.release(area, this.edgePoint(layer, left), [vx, 10], [0.25, 1]);
    }

    /// Just past the layer's end, at the height the snow reaches on the end column, which on a rounded corner lies down the curve.
    edgePoint(layer, left) {
      const end = left ? 0 : layer.count - 1;
      return [left ? layer.x - 0.75 : layer.x + layer.width + 0.75, layer.surface(end)];
    }

    /// Lets go of every window's snow at once: it falls as lumps and loose powder.
    shed() {
      for (const { layer } of this.ledges) {
        let start = 0;
        while (start < layer.count) {
          const end = Math.min(layer.count, start + Math.floor(rand(2, 7)));
          let area = 0, top = Infinity, bottom = -Infinity;
          for (let i = start; i < end; i++) {
            if (layer.h[i] <= 0.05) continue;
            area += layer.h[i] * COLUMN;
            top = Math.min(top, layer.surface(i));
            bottom = Math.max(bottom, layer.floorY(i));
          }
          const cohesion = area > 14 ? rand(0.4, 0.7) : 0;
          for (let i = start; i < end; i++) {
            const h = layer.h[i];
            if (h > 0.05) this.release(h * COLUMN * (1 - cohesion), [layer.centerX(i), layer.floorY(i)], [0, 0], [COLUMN / 2, h], 0.25);
          }
          if (cohesion > 0) this.throwLump(area * cohesion, [(layer.centerX(start) + layer.centerX(end - 1)) / 2, (top + bottom) / 2]);
          start = end;
        }
        layer.clear();
      }
    }

    // MARK: Gust

    get blowing() {
      return this.gust !== null && !this.gust.calm;
    }

    gustSpeed() {
      if (!this.blowing) return 0;
      const age = this.time - this.gust.start;
      return this.gust.direction * GUST_WIND * smoothstep(0, 0.12, age) * (1 - smoothstep(0.9, GUST_LENGTH, age));
    }

    /// Where the gust's front is `age` seconds in, coming in from `margin` past the windward edge.
    /// It hits at full speed and eases off as it crosses.
    gustFront(age, direction, margin) {
      const t = Math.min(Math.max(age / GUST_SWEEP, 0), 1);
      const travel = (this.width + 2 * margin) * (1 - (1 - t) * (1 - t));
      return direction > 0 ? -margin + travel : this.width + margin - travel;
    }

    /// Clears all the snow: a gust in `direction` lifts it off the ground and the windows and blows it off the page.
    blowAway(direction) {
      this.gust = { start: this.time, direction: direction < 0 ? -1 : 1, calm: false };
    }

    blow(dt) {
      const gust = this.gust;
      if (!gust) return;
      const age = this.time - gust.start;
      if (age >= GUST_LENGTH && !this.flakes.some(f => f.blown)) {
        this.gust = null;
        return;
      }
      const margin = 40;
      const front = this.gustFront(age, gust.direction, margin);
      // Whatever the front has long passed is gone by now, however deep it was.
      const finish = age > GUST_SWEEP + 0.8;
      const trailing = this.gustFront(age - GUST_LAG, gust.direction, margin);
      for (const f of this.flakes) {
        // Each flake is caught a little further behind, so the snowfall doesn't go as one straight line.
        if (!f.blown && (finish || (trailing - f.x) * gust.direction > f.phase / PHASE_WRAP * 240)) f.blown = gust.direction;
      }
      for (const layer of this.layers) {
        let dust = 0;
        for (let i = 0; i < layer.count; i++) {
          const height = layer.h[i];
          if (height <= 0) continue;
          const x = layer.centerX(i);
          if ((front - x) * gust.direction <= 0) continue;
          const surface = layer.surface(i);
          const loss = finish ? height : Math.min(height, (height * 8 + 50) * dt);
          layer.h[i] = height - loss;
          dust += loss * COLUMN;
          while (dust >= 3.5) {
            this.lift(x, surface, gust.direction);
            dust -= 3.5;
          }
        }
      }
    }

    /// A pinch of snow torn off by the gust: a grain of powder, now and then with a faint puff around it.
    lift(x, y, direction) {
      if (this.grains.length >= MAX_GRAINS) return;
      const { tint, amount: tintAmount, shape } = look(resolvedTheme());
      this.grains.push({
        x: x + rand(-1, 1),
        y: y + rand(0, 3),
        vx: direction * rand(800, 1800),
        vy: -rand(60, 420),
        area: 0,
        size: rand(1.1, 2.6),
        blown: true,
        tint,
        tintAmount,
        shape,
      });
      if (Math.random() >= 0.35 || this.motes.length >= MAX_MOTES) return;
      this.motes.push({ x, y, vx: direction * rand(200, 600), vy: -rand(20, 120), size: rand(6, 14), alpha: rand(0.06, 0.14), age: 0, life: rand(0.6, 1.3), tint, tintAmount, shape });
    }

    clear() {
      this.gust = null;
      this.flakes.length = 0;
      this.grains.length = 0;
      this.motes.length = 0;
      this.ground.clear();
      for (const { layer } of this.ledges) layer.clear();
    }

    // MARK: Cursor

    trackCursor(pointer, dt) {
      for (const { layer } of this.ledges) layer.held.fill(0);
      this.ground.held.fill(0);
      if (!pointer) {
        this.cursor = null;
        return;
      }
      const rect = this.canvas.getBoundingClientRect();
      const center = { x: pointer.x - rect.left + CURSOR_OFFSET[0], y: pointer.y - rect.top + CURSOR_OFFSET[1] };
      const previous = this.cursor;
      this.cursor = center;
      if (!previous) return;

      const dx = center.x - previous.x, dy = center.y - previous.y;
      const distance = Math.hypot(dx, dy);
      let vx = dx / Math.max(dt, 1 / 240), vy = dy / Math.max(dt, 1 / 240);
      const speed = Math.hypot(vx, vy);
      if (speed > 3000) {
        vx *= 3000 / speed;
        vy *= 3000 / speed;
      }
      this.cursorVelocity[0] += (vx - this.cursorVelocity[0]) * 0.5;
      this.cursorVelocity[1] += (vy - this.cursorVelocity[1]) * 0.5;

      const r = CURSOR_RADIUS;
      const top = Math.min(center.y, previous.y) - r, bottom = Math.max(center.y, previous.y) + r;
      const left = Math.min(center.x, previous.x) - r, right = Math.max(center.x, previous.x) + r;
      const touched = this.layers.filter(layer =>
        bottom >= layer.y - layer.top - 1 && top <= layer.y + layer.bedDepth
        && right >= layer.x && left <= layer.x + layer.width
        && (layer === this.ground || layer.top > 0.01));
      if (!touched.length) return;
      if (distance > 400) {
        for (const layer of touched) this.carve(layer, center, [0, 0], 0, true);
        return;
      }
      const steps = Math.floor(distance / (r * 0.5)) + 1;
      for (let s = 1; s <= steps; s++) {
        const p = { x: previous.x + dx * s / steps, y: previous.y + dy * s / steps };
        for (const layer of touched) this.carve(layer, p, [vx, vy], distance / steps, s === steps);
      }
    }

    carve(layer, center, velocity, travel, final) {
      const r = CURSOR_RADIUS;
      const lower = Math.max(0, Math.floor((center.x - r - layer.x) / COLUMN));
      const upper = Math.min(layer.count - 1, Math.floor((center.x + r - layer.x) / COLUMN));
      if (lower > upper) return;
      const local = layer.y - center.y;
      const speed = Math.hypot(velocity[0], velocity[1]);
      const sprayShare = smoothstep(60, 420, speed);
      let pushedLeft = 0, pushedRight = 0;

      for (let i = lower; i <= upper; i++) {
        const x = layer.centerX(i);
        const dx = x - center.x;
        const reach = r * r - dx * dx;
        if (reach <= 0) continue;
        const half = Math.sqrt(reach);
        const bedLevel = -layer.bed[i];
        const bottom = Math.max(0, local - half - bedLevel);
        const top = local + half - bedLevel;
        if (top <= bottom) continue;
        const h = layer.h[i];
        const buried = h > top;
        if (final && !buried && h >= bottom - 1) layer.held[i] = 1;
        if (h <= bottom + 0.01) continue;
        // A buried cursor only displaces the snow it sweeps through; the rest rests on top of it.
        const removed = buried ? (top - bottom) * Math.min(1, travel / (2 * r)) : h - bottom;
        if (removed <= 0.001) continue;
        layer.h[i] = Math.max(0, h - removed);
        const area = removed * COLUMN;
        const surface = layer.floorY(i) - (buried ? h - removed : h);
        const sprayed = sprayShare > 0 ? this.emit(area * sprayShare, [x, surface], center, velocity) : 0;
        const side = Math.abs(velocity[0]) > 40 ? velocity[0] : dx;
        if (side < 0) pushedLeft += area - sprayed;
        else pushedRight += area - sprayed;
      }
      for (const [area, x] of [[pushedLeft, center.x - r - COLUMN * 3], [pushedRight, center.x + r + COLUMN * 3]]) {
        if (area <= 0) continue;
        const missed = layer.deposit(area, x);
        this.dropPast(layer, missed, x, velocity[0] * 0.5);
      }
    }

    emit(area, [ox, oy], center, [vx, vy]) {
      if (area <= 0.6 || this.grains.length >= MAX_GRAINS) return 0;
      const count = Math.min(4, Math.floor(area / 1.6) + 1);
      const share = area / count;
      const speed = Math.hypot(vx, vy);
      const offX = ox - center.x, offY = oy - center.y;
      const length = Math.hypot(offX, offY);
      const outX = length > 0.001 ? offX / length : 0;
      const outY = length > 0.001 ? offY / length : -1;
      const { tint, amount: tintAmount, shape } = look(resolvedTheme());
      for (let k = 0; k < count; k++) {
        const scale = rand(0.55, 1.25), push = speed * rand(0.05, 0.25);
        this.grains.push({
          x: ox + rand(-1, 1),
          y: oy - rand(0, 2),
          vx: vx * scale + outX * push + rand(-30, 30),
          vy: vy * scale + outY * push - speed * rand(0.02, 0.2) + rand(-40, 20),
          area: share,
          size: Math.min(4.2, Math.max(1.1, Math.sqrt(share) * 1.05)),
          tint,
          tintAmount,
          shape,
        });
      }
      return area;
    }

    // MARK: Drawing

    draw() {
      const gl = this.gl;
      if (!gl) return;
      gl.viewport(0, 0, this.canvas.width, this.canvas.height);
      gl.clearColor(0, 0, 0, 0);
      gl.clear(gl.COLOR_BUFFER_BIT);
      if (settings.quit) return;
      const lamp = this.light ? [this.light.x, this.light.y, 1] : [0, 0, 0];

      gl.useProgram(this.pile.program);
      gl.bindVertexArray(this.pileVAO);
      const u = this.pile.uniforms;
      gl.uniform2f(u.uView, this.width, this.height);
      gl.uniform1f(u.uScale, this.dpr);
      gl.uniform1f(u.uTime, this.time % 1000);
      gl.uniform1f(u.uWidth, this.width);
      gl.uniform1i(u.uNight, this.night ? 1 : 0);
      gl.uniform1i(u.uTheme, THEME_INDEX[resolvedTheme()]);
      gl.uniform3f(u.uLamp, ...lamp);
      gl.uniform1i(u.uHeights, 0);
      gl.activeTexture(gl.TEXTURE0);
      for (const layer of this.layers) {
        if (layer.top <= 0.05) continue;
        let texture = this.textures.get(layer);
        if (!texture) {
          texture = gl.createTexture();
          this.textures.set(layer, texture);
        }
        gl.bindTexture(gl.TEXTURE_2D, texture);
        gl.texImage2D(gl.TEXTURE_2D, 0, gl.R32F, layer.count, 2, 0, gl.RED, gl.FLOAT, layer.write());
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
        gl.uniform1i(u.uCount, layer.count);
        gl.uniform2f(u.uOrigin, layer.x, layer.y);
        gl.uniform4f(u.uRect, layer.x, layer.y - layer.top - 2, layer.x + layer.width, layer.y + layer.bedDepth);
        gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
      }

      const total = this.flakes.length + this.grains.length + this.motes.length;
      if (!total) return;
      if (this.spriteData.length < total * 11) this.spriteData = new Float32Array(Math.ceil(total * 1.5) * 11);
      const data = this.spriteData;
      let k = 0;
      for (const f of this.flakes) {
        data[k++] = f.x; data[k++] = f.y; data[k++] = f.size; data[k++] = f.alpha; data[k++] = f.soft; data[k++] = 0;
        data[k++] = f.tint[0]; data[k++] = f.tint[1]; data[k++] = f.tint[2]; data[k++] = f.tintAmount; data[k++] = f.shape;
      }
      for (const g of this.grains) {
        data[k++] = g.x; data[k++] = g.y; data[k++] = g.size;
        data[k++] = g.lump ? 0.97 : g.blown ? 0.8 : 0.95; data[k++] = g.lump ? 0.2 : 0.3; data[k++] = g.lump || 0;
        data[k++] = g.tint[0]; data[k++] = g.tint[1]; data[k++] = g.tint[2]; data[k++] = g.tintAmount; data[k++] = g.shape;
      }
      for (const m of this.motes) {
        const t = m.age / m.life;
        data[k++] = m.x; data[k++] = m.y; data[k++] = m.size * (1 + t * (m.grow || 0));
        data[k++] = m.alpha * Math.min(1, m.age / 0.06) * Math.pow(1 - t, 1.5); data[k++] = 1; data[k++] = 0;
        data[k++] = m.tint[0]; data[k++] = m.tint[1]; data[k++] = m.tint[2]; data[k++] = m.tintAmount; data[k++] = m.shape;
      }
      gl.useProgram(this.sprite.program);
      const s = this.sprite.uniforms;
      gl.uniform2f(s.uView, this.width, this.height);
      gl.uniform1f(s.uScale, this.dpr);
      gl.uniform1f(s.uTime, this.time % 1000);
      gl.uniform1i(s.uNight, this.night ? 1 : 0);
      gl.uniform3f(s.uLamp, ...lamp);
      gl.bindVertexArray(this.spriteVAO);
      gl.bindBuffer(gl.ARRAY_BUFFER, this.spriteBuffer);
      gl.bufferData(gl.ARRAY_BUFFER, data.subarray(0, k), gl.DYNAMIC_DRAW);
      gl.drawArrays(gl.POINTS, 0, total);
      gl.bindVertexArray(null);
    }
  }

  function ledgesActive() {
    return settings.windows && !settings.behind;
  }

  // MARK: - Page

  const grain = document.querySelectorAll('.grain');
  if (grain.length) {
    const canvas = document.createElement('canvas');
    canvas.width = canvas.height = 128;
    const ctx = canvas.getContext('2d');
    const image = ctx.createImageData(128, 128);
    for (let i = 0; i < image.data.length; i += 4) {
      const v = Math.random() * 255;
      image.data[i] = image.data[i + 1] = image.data[i + 2] = v;
      image.data[i + 3] = 255;
    }
    ctx.putImageData(image, 0, 0);
    const texture = `url(${canvas.toDataURL()})`;
    grain.forEach(element => { element.style.backgroundImage = texture; });
  }

  const scenes = [...document.querySelectorAll('[data-scene]')].map(section => new Scene(section));
  let pointer = null;
  let running = false;
  let last = 0;

  /// Variable snowfall wanders between lulls and bursts like the app's, only every few seconds instead of minutes.
  function vary(dt) {
    variation.clock += dt;
    if (settings.snowfall !== 'variable') {
      variation.target = 1;
    } else if (variation.clock >= variation.next) {
      const roll = Math.random();
      variation.target = roll < 0.25 ? rand(0.25, 0.5) : roll < 0.75 ? rand(0.7, 1.3) : rand(1.5, 2.1);
      variation.next = variation.clock + rand(8, 16);
    }
    variation.strength += (variation.target - variation.strength) * (1 - Math.exp(-dt / 3));
  }

  function frame(now) {
    const dt = last ? Math.min((now - last) / 1000, 0.05) : 1 / 60;
    last = now;
    vary(dt);
    let active = false;
    for (const scene of scenes) {
      if (!scene.visible || !scene.gl) continue;
      scene.step(dt, pointer);
      scene.draw();
      active = true;
    }
    if (active && !document.hidden && !reduceMotion.matches) {
      requestAnimationFrame(frame);
    } else {
      running = false;
      last = 0;
    }
  }

  function wake() {
    if (running || reduceMotion.matches || document.hidden) return;
    running = true;
    last = 0;
    requestAnimationFrame(frame);
  }

  function measureAll() {
    for (const scene of scenes) scene.measure();
  }

  let pendingMeasure = 0;
  const resizeObserver = new ResizeObserver(() => {
    cancelAnimationFrame(pendingMeasure);
    pendingMeasure = requestAnimationFrame(measureAll);
  });
  const visibility = new IntersectionObserver(entries => {
    for (const entry of entries) {
      const scene = scenes.find(s => s.section === entry.target);
      if (scene) scene.visible = entry.isIntersecting;
    }
    wake();
  });
  for (const scene of scenes) {
    resizeObserver.observe(scene.section);
    visibility.observe(scene.section);
  }

  // MARK: - Menu

  for (const menu of document.querySelectorAll('[data-snow-menu]')) {
    menu.addEventListener('click', event => {
      const item = event.target.closest('.menu-item');
      if (!item || item.getAttribute('aria-disabled') === 'true') return;
      const ledgesBefore = ledgesActive();
      if (item.dataset.snowfall) {
        settings.snowfall = item.dataset.snowfall;
        if (settings.snowfall === 'variable') variation.next = variation.clock;
      } else if (item.dataset.theme) {
        settings.theme = item.dataset.theme;
        settings.themeAuto = false;
      } else if (item.dataset.toggle) {
        // Turning Auto off keeps the season that is on screen as the manual pick.
        if (item.dataset.toggle === 'themeAuto' && settings.themeAuto) settings.theme = resolvedTheme();
        settings[item.dataset.toggle] = !settings[item.dataset.toggle];
      }
      const theme = resolvedTheme();
      if (scenes[0] && scenes[0].theme !== theme) scenes.forEach(scene => scene.setTheme(theme));
      if (item.dataset.action === 'quit') {
        settings.quit = true;
        scenes.forEach(scene => scene.clear());
      } else {
        settings.quit = false;
      }
      if (item.dataset.action === 'clear') clearSnow();
      if (ledgesBefore && !ledgesActive()) scenes.forEach(scene => scene.shed());
      document.body.classList.toggle('snow-behind', settings.behind);
      reflect();
      scenes.forEach(scene => scene.draw());
      wake();
      if (item.dataset.toggle === 'screenshots') capture();
    });
  }

  /// Blows the snow off every scene in the way the breeze blows; with reduced motion the drifts just go.
  function clearSnow() {
    if (reduceMotion.matches) {
      for (const scene of scenes) {
        scene.ground.clear();
        for (const { layer } of scene.ledges) layer.clear();
      }
      return;
    }
    const direction = scenes[0]?.breeze() < 0 ? -1 : 1;
    scenes.forEach(scene => scene.blowAway(direction));
  }

  // MARK: - Screenshot

  let shownCapture = null;

  /// Floats a screenshot of the page in the corner, as macOS does: with the snow in it only when Show in Screenshots is on.
  function capture() {
    shownCapture?.remove();
    const width = Math.min(240, innerWidth * 0.42);
    const scale = width / innerWidth;
    const frame = document.createElement('div');
    frame.className = 'capture';
    frame.setAttribute('aria-hidden', 'true');
    frame.inert = true;
    frame.style.width = `${width}px`;
    frame.style.height = `${innerHeight * scale}px`;
    const stage = document.createElement('div');
    stage.className = 'capture-stage';
    stage.style.width = `${innerWidth}px`;
    stage.style.height = `${innerHeight}px`;
    stage.style.transform = `scale(${scale})`;

    for (const block of document.querySelectorAll('body > header, body > main, body > footer')) {
      const rect = block.getBoundingClientRect();
      if (rect.bottom <= 0 || rect.top >= innerHeight) continue;
      const copy = block.cloneNode(true);
      for (const element of [copy, ...copy.querySelectorAll('[id], .reveal')]) {
        element.removeAttribute('id');
        element.classList.remove('reveal');
      }
      Object.assign(copy.style, { position: 'absolute', left: `${rect.left}px`, top: `${rect.top}px`, width: `${rect.width}px`, margin: '0' });
      const originals = block.querySelectorAll('canvas');
      copy.querySelectorAll('canvas').forEach((target, i) => {
        const source = originals[i];
        const scene = scenes.find(s => s.canvas === source);
        if (scene) {
          if (!settings.screenshots) {
            target.remove();
            return;
          }
          // A WebGL canvas can only be read right after it's drawn.
          scene.draw();
        }
        target.width = source.width;
        target.height = source.height;
        target.getContext('2d').drawImage(source, 0, 0);
      });
      stage.append(copy);
    }

    frame.append(stage);
    document.body.append(frame);
    shownCapture = frame;
    setTimeout(() => {
      if (reduceMotion.matches) {
        frame.remove();
        return;
      }
      frame.classList.add('leaving');
      frame.addEventListener('animationend', () => frame.remove(), { once: true });
    }, 3600);
  }

  function reflect() {
    const theme = resolvedTheme();
    for (const item of document.querySelectorAll('[data-snow-menu] .menu-item')) {
      if (item.dataset.snowfall) item.setAttribute('aria-checked', String(!settings.quit && item.dataset.snowfall === settings.snowfall));
      if (item.dataset.theme) item.setAttribute('aria-checked', String(!settings.quit && !settings.themeAuto && item.dataset.theme === settings.theme));
      if (item.dataset.toggle) item.setAttribute('aria-checked', String(settings[item.dataset.toggle]));
      if (item.dataset.toggle === 'melt') item.setAttribute('aria-disabled', String(!piles()));
      if (item.dataset.toggle === 'accumulate') item.setAttribute('aria-disabled', String(theme === 'drops'));
      if (item.dataset.toggle === 'windows') item.setAttribute('aria-disabled', String(settings.behind));
    }
  }

  addEventListener('pointermove', event => { pointer = { x: event.clientX, y: event.clientY }; }, { passive: true });
  document.documentElement.addEventListener('pointerleave', () => { pointer = null; });
  addEventListener('blur', () => { pointer = null; });
  document.addEventListener('visibilitychange', wake);
  reduceMotion.addEventListener('change', () => (reduceMotion.matches ? scenes.forEach(scene => scene.draw()) : wake()));
  document.fonts?.ready.then(measureAll);
  reflect();
  measureAll();
})();
