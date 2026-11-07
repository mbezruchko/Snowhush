'use strict';

// The app's own simulation and the Linux app's shaders for Autumn Leaves, Rain and Cherry Blossom, built from
// site-engine/ to engine.wasm and loaded the first time one of them is picked. The simulation works in points, y up;
// the page is y down, so frames and the pointer are flipped on the way in.
window.SnowhushEngine = (() => {
  const SPRITE_FLOATS = 33;
  const PILE_FLOATS = 72;
  /// The height texture's width; the pile shader wraps its index at the same width.
  const ROW = 4096;
  const COLUMN_WIDTH = 2;
  const MODES = { snow: 0, autumn: 1, rain: 2, blossom: 3 };

  let loading = null;
  const load = () => {
    loading ??= fetch('engine.wasm')
      .then(response => {
        if (!response.ok) throw new Error(`engine.wasm: ${response.status}`);
        return response.arrayBuffer();
      })
      .then(bytes => WebAssembly.compile(bytes));
    return loading;
  };

  class Engine {
    constructor(module, width, height, intensity, mode) {
      this.w = new WebAssembly.Instance(module, {}).exports;
      this.width = width;
      this.height = height;
      this.e = this.w.engine_new(width, height, intensity, MODES[mode]);
    }

    free() { this.w.engine_free(this.e); }
    resize(width, height) {
      this.width = width;
      this.height = height;
      this.w.engine_resize(this.e, width, height);
    }
    switchTo(mode, calm) { this.w.engine_switch(this.e, MODES[mode], calm); }
    blow(calm) { this.w.engine_blow(this.e, calm); }
    intensity(value) { this.w.engine_intensity(this.e, value); }
    opacity(value) { this.w.engine_opacity(this.e, value); }
    accumulates(on) { this.w.engine_accumulates(this.e, on); }
    thaws(on) { this.w.engine_thaws(this.e, on); }

    /// The windows, in page points from the section's top left: x, y, w, h and their corner radius.
    windows(ledges, interval) {
      const at = this.w.engine_input(this.e, Math.max(1, ledges.length * 7));
      const data = new Float32Array(this.w.memory.buffer, at, ledges.length * 7);
      ledges.forEach((l, k) => data.set([k + 1, l.x, this.height - l.y - l.h, l.w, l.h, l.radius, 2.5], k * 7));
      this.w.engine_windows(this.e, ledges.length, interval);
    }

    step(dt, cursor) {
      if (cursor) this.w.engine_step(this.e, dt, cursor.x, this.height - cursor.y);
      else this.w.engine_step(this.e, dt, -10000, -10000);
    }

    sprites() {
      const count = this.w.engine_sprites(this.e);
      return { count, data: new Float32Array(this.w.memory.buffer, this.w.engine_sprites_at(this.e), count * SPRITE_FLOATS) };
    }

    layers() {
      const count = this.w.engine_layers(this.e);
      const columns = this.w.engine_columns(this.e);
      const heights = new Float32Array(this.w.memory.buffer, this.w.engine_heights_at(this.e), columns);
      const table = new Float32Array(this.w.memory.buffer, this.w.engine_piles_at(this.e), count * PILE_FLOATS);
      const piles = [];
      for (let i = 0; i < count; i++) {
        const row = table.subarray(i * PILE_FLOATS, (i + 1) * PILE_FLOATS);
        piles.push({ offset: row[0], count: row[1], origin: [row[2], row[3]], top: row[4], bottom: row[5], drift: row[6], occluders: row.slice(8, 8 + 4 * row[7]) });
      }
      return { heights, piles };
    }

    glass() { return Boolean(this.w.engine_glass(this.e)); }

    look() {
      const bits = this.w.engine_look(this.e);
      return { autumn: bits & 1 ? 1 : 0, blossom: bits & 2 ? 1 : 0, wilt: bits & 4 ? 1 : 0 };
    }

    shader(index) {
      const length = this.w.engine_shader(this.e, index);
      return new TextDecoder().decode(new Uint8Array(this.w.memory.buffer, this.w.engine_text_at(this.e), length));
    }
  }

  function compile(gl, vertex, fragment) {
    const shader = (type, source) => {
      const s = gl.createShader(type);
      gl.shaderSource(s, source);
      gl.compileShader(s);
      if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s) || 'shader');
      return s;
    };
    const program = gl.createProgram();
    gl.attachShader(program, shader(gl.VERTEX_SHADER, vertex));
    gl.attachShader(program, shader(gl.FRAGMENT_SHADER, fragment));
    gl.linkProgram(program);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(program) || 'link');
    const cache = new Map();
    return {
      program,
      at: name => {
        if (!cache.has(name)) cache.set(name, gl.getUniformLocation(program, name));
        return cache.get(name);
      },
    };
  }

  /// Draws as the Linux app does: the piles from a height texture, every leaf, petal or drop as an instanced quad, and
  /// rain on the glass added up into a field first and drawn from it.
  class Renderer {
    constructor(gl, engine) {
      this.gl = gl;
      const [spriteVertex, spriteFragment, pileVertex, pileFragment, fieldFragment, glassVertex, glassFragment] =
        [0, 1, 2, 3, 4, 5, 6].map(i => engine.shader(i));
      this.sprite = compile(gl, spriteVertex, spriteFragment);
      this.pile = compile(gl, pileVertex, pileFragment);
      this.fields = gl.getExtension('EXT_color_buffer_float') !== null;
      if (this.fields) {
        this.field = compile(gl, spriteVertex, fieldFragment);
        this.glass = compile(gl, glassVertex, glassFragment);
      }
      this.array = gl.createVertexArray();
      this.buffer = gl.createBuffer();
      gl.bindVertexArray(this.array);
      gl.bindBuffer(gl.ARRAY_BUFFER, this.buffer);
      // Position, velocity, size, alpha, soft, tint amount, tint, lump, ghost, pose, mood, leaf, spin, rain, drop: see
      // `Sprite`.
      for (const [location, components, offset] of [
        [0, 2, 0], [1, 2, 8], [2, 1, 16], [3, 1, 20], [4, 1, 24], [5, 1, 28], [6, 3, 32], [7, 1, 44],
        [8, 4, 48], [9, 4, 64], [10, 2, 80], [11, 3, 88], [12, 2, 100], [13, 2, 108], [14, 4, 116],
      ]) {
        gl.enableVertexAttribArray(location);
        gl.vertexAttribPointer(location, components, gl.FLOAT, false, SPRITE_FLOATS * 4, offset);
        gl.vertexAttribDivisor(location, 1);
      }
      this.empty = gl.createVertexArray();
      gl.bindVertexArray(null);
      this.heights = gl.createTexture();
      gl.bindTexture(gl.TEXTURE_2D, this.heights);
      for (const [parameter, value] of [[gl.TEXTURE_MIN_FILTER, gl.NEAREST], [gl.TEXTURE_MAG_FILTER, gl.NEAREST], [gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE], [gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE]]) {
        gl.texParameteri(gl.TEXTURE_2D, parameter, value);
      }
      this.rows = 0;
      this.columns = new Float32Array(0);
      this.target = null;
    }

    fieldFor(width, height) {
      const gl = this.gl;
      if (this.target && this.target.width === width && this.target.height === height) return this.target;
      if (this.target) {
        gl.deleteFramebuffer(this.target.framebuffer);
        gl.deleteTexture(this.target.texture);
      }
      const texture = gl.createTexture();
      gl.bindTexture(gl.TEXTURE_2D, texture);
      for (const [parameter, value] of [[gl.TEXTURE_MIN_FILTER, gl.NEAREST], [gl.TEXTURE_MAG_FILTER, gl.NEAREST], [gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE], [gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE]]) {
        gl.texParameteri(gl.TEXTURE_2D, parameter, value);
      }
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RG16F, width, height, 0, gl.RG, gl.HALF_FLOAT, null);
      const framebuffer = gl.createFramebuffer();
      gl.bindFramebuffer(gl.FRAMEBUFFER, framebuffer);
      gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, texture, 0);
      gl.bindFramebuffer(gl.FRAMEBUFFER, null);
      this.target = { texture, framebuffer, width, height };
      return this.target;
    }

    /// `scale` is pixels per point; `light` the colour everything is lit in.
    draw(engine, scale, light, time) {
      const gl = this.gl;
      const view = [engine.width, engine.height];
      const lit = [...light, 1];
      const shadowTint = [0, 0, 0, 0];
      const rim = [1, 0.56, 0.2, 0];
      const lantern = [0, 0, 130, 0];
      const candles = new Float32Array(12);
      const shake = [0, 0];
      const panel = [0, 0, 0, 0];
      const { autumn, blossom, wilt } = engine.look();

      gl.viewport(0, 0, gl.drawingBufferWidth, gl.drawingBufferHeight);
      gl.clearColor(0, 0, 0, 0);
      gl.clear(gl.COLOR_BUFFER_BIT);
      gl.enable(gl.BLEND);
      gl.blendFuncSeparate(gl.ONE, gl.ONE_MINUS_SRC_ALPHA, gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
      gl.pixelStorei(gl.UNPACK_ALIGNMENT, 4);

      const { heights, piles } = engine.layers();
      if (piles.length) {
        const rows = Math.max(1, Math.ceil(heights.length / ROW));
        if (this.columns.length !== rows * ROW) this.columns = new Float32Array(rows * ROW);
        this.columns.fill(0);
        this.columns.set(heights);
        gl.activeTexture(gl.TEXTURE0);
        gl.bindTexture(gl.TEXTURE_2D, this.heights);
        if (rows === this.rows) {
          gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, ROW, rows, gl.RED, gl.FLOAT, this.columns);
        } else {
          gl.texImage2D(gl.TEXTURE_2D, 0, gl.R32F, ROW, rows, 0, gl.RED, gl.FLOAT, this.columns);
          this.rows = rows;
        }
        const p = this.pile;
        gl.useProgram(p.program);
        gl.bindVertexArray(this.empty);
        gl.uniform2fv(p.at('viewSize'), view);
        gl.uniform1f(p.at('scale'), scale);
        gl.uniform1f(p.at('time'), time);
        gl.uniform1f(p.at('columnWidth'), COLUMN_WIDTH);
        gl.uniform4fv(p.at('light'), lit);
        gl.uniform4fv(p.at('shadowTint'), shadowTint);
        gl.uniform4fv(p.at('rim'), rim);
        gl.uniform4fv(p.at('lantern'), lantern);
        gl.uniform4fv(p.at('pumpkins'), candles);
        gl.uniform2fv(p.at('shake'), shake);
        gl.uniform1i(p.at('heights'), 0);
        gl.uniform4fv(p.at('panel'), panel);
        gl.uniform1f(p.at('autumn'), autumn);
        gl.uniform1f(p.at('blossom'), blossom);
        gl.uniform1f(p.at('wilt'), wilt);
        for (const pile of piles) {
          gl.uniform1i(p.at('columnCount'), pile.count);
          gl.uniform1f(p.at('pileTop'), pile.top + 2);
          gl.uniform1f(p.at('pileBottom'), pile.bottom);
          gl.uniform2f(p.at('origin'), pile.origin[0], pile.origin[1]);
          gl.uniform1f(p.at('drift'), pile.drift);
          gl.uniform1i(p.at('heightOffset'), pile.offset);
          gl.uniform1i(p.at('occluderCount'), pile.occluders.length / 4);
          if (pile.occluders.length) gl.uniform4fv(p.at('occluders'), pile.occluders);
          gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
        }
      }

      const glass = this.fields && engine.glass();
      const sprites = engine.sprites();
      if (sprites.count > 0) {
        gl.bindVertexArray(this.array);
        gl.bindBuffer(gl.ARRAY_BUFFER, this.buffer);
        gl.bufferData(gl.ARRAY_BUFFER, sprites.data, gl.STREAM_DRAW);
        const common = program => {
          gl.useProgram(program.program);
          gl.uniform2fv(program.at('viewSize'), view);
          gl.uniform1f(program.at('scale'), scale);
          gl.uniform1f(program.at('time'), time);
          gl.uniform4fv(program.at('lantern'), lantern);
          gl.uniform4fv(program.at('pumpkins'), candles);
          gl.uniform2fv(program.at('shake'), shake);
          gl.uniform4fv(program.at('panel'), panel);
        };
        const field = glass ? this.fieldFor(gl.drawingBufferWidth, gl.drawingBufferHeight) : null;
        if (field) {
          gl.bindFramebuffer(gl.FRAMEBUFFER, field.framebuffer);
          gl.clear(gl.COLOR_BUFFER_BIT);
          gl.blendFunc(gl.ONE, gl.ONE);
          common(this.field);
          gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, sprites.count);
          gl.bindFramebuffer(gl.FRAMEBUFFER, null);
          gl.blendFuncSeparate(gl.ONE, gl.ONE_MINUS_SRC_ALPHA, gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
        }
        const s = this.sprite;
        common(s);
        gl.uniform1f(s.at('glass'), field ? 1 : 0);
        gl.uniform4fv(s.at('light'), lit);
        gl.uniform4fv(s.at('shadowTint'), shadowTint);
        gl.uniform4fv(s.at('rim'), rim);
        gl.uniform1f(s.at('autumn'), autumn);
        gl.uniform1f(s.at('blossom'), blossom);
        gl.uniform1f(s.at('wilt'), wilt);
        gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, sprites.count);
        if (field) {
          const g = this.glass;
          gl.bindVertexArray(this.empty);
          gl.useProgram(g.program);
          gl.activeTexture(gl.TEXTURE0);
          gl.bindTexture(gl.TEXTURE_2D, field.texture);
          gl.uniform1i(g.at('field'), 0);
          gl.uniform4fv(g.at('light'), lit);
          gl.uniform1f(g.at('scale'), scale);
          gl.drawArrays(gl.TRIANGLES, 0, 3);
        }
      }
      gl.bindVertexArray(null);
      gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
    }
  }

  return { load, Engine, Renderer };
})();
