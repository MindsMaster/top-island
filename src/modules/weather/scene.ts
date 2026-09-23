import type { WxKind } from './codes';

const TAU = Math.PI * 2;
const LAYERS = [0.35, 0.62, 1];
const MAX_DPR = 2;

const rand = (a: number, b: number) => a + Math.random() * (b - a);

type CloudTone = 'light' | 'grey' | 'dark' | 'night';

interface SceneConfig {
  sun?: number;
  moon?: number;
  stars?: number;
  shoot?: boolean;
  clouds?: [count: number, tone: CloudTone, alpha: number];
  rain?: [count: number, wind: number, heavy: boolean];
  snow?: number;
  fog?: number;
  flash?: boolean;
  mist?: boolean;
}

function configFor(kind: WxKind, night: boolean): SceneConfig {
  switch (kind) {
    case 'clear':
      return night ? { stars: 80, moon: 1, shoot: true } : { sun: 1 };
    case 'partly':
      return night
        ? { clouds: [4, 'night', 0.6], stars: 40, moon: 0.7 }
        : { clouds: [4, 'light', 0.45], sun: 0.8 };
    case 'cloudy':
      return night ? { clouds: [6, 'night', 0.75], stars: 15 } : { clouds: [6, 'light', 0.55], sun: 0.25 };
    case 'rain':
      return { clouds: [5, night ? 'night' : 'grey', 0.7], rain: [130, 0.12, false] };
    case 'rain-heavy':
      return { clouds: [6, night ? 'night' : 'dark', 0.9], rain: [300, 0.26, true], mist: true };
    case 'thunder':
      return { clouds: [6, night ? 'night' : 'dark', 0.95], rain: [200, 0.18, false], flash: true };
    case 'snow':
      return { clouds: [4, night ? 'night' : 'light', 0.35], snow: 120 };
    case 'fog':
      return { fog: 6, clouds: [3, night ? 'night' : 'light', 0.25] };
  }
}

function sprite(w: number, h: number, color: string, blur: number, puffs: number[][]) {
  const c = document.createElement('canvas');
  c.width = w;
  c.height = h;
  const x = c.getContext('2d')!;
  x.filter = `blur(${blur}px)`;
  x.fillStyle = color;
  for (const [px, py, r] of puffs) {
    x.beginPath();
    x.arc(px * w, py * h, r * h, 0, TAU);
    x.fill();
  }
  return c;
}

const PUFFS = [
  [0.3, 0.62, 0.24],
  [0.45, 0.46, 0.32],
  [0.62, 0.5, 0.28],
  [0.76, 0.63, 0.2],
  [0.52, 0.68, 0.22],
  [0.2, 0.68, 0.16],
];

let sprites: Record<CloudTone | 'fog', HTMLCanvasElement> | null = null;

/** 模糊只在预渲染时做一次 */
function getSprites() {
  sprites ??= {
    light: sprite(320, 160, '#eef3f9', 12, PUFFS),
    grey: sprite(320, 160, '#8391a4', 12, PUFFS),
    dark: sprite(320, 160, '#4a5566', 12, PUFFS),
    night: sprite(320, 160, '#3b4557', 12, PUFFS),
    fog: sprite(640, 160, '#d9dee4', 26, [
      [0.18, 0.5, 0.22],
      [0.38, 0.5, 0.26],
      [0.6, 0.5, 0.25],
      [0.82, 0.5, 0.22],
    ]),
  };
  return sprites;
}

interface Cloud {
  x: number;
  y: number;
  s: number;
  a: number;
  v: number;
  spr: HTMLCanvasElement;
}
interface Drop {
  layer: number;
  v: number;
  len: number;
  x: number;
  y: number;
}
interface Flake {
  layer: number;
  x: number;
  y: number;
  v: number;
  amp: number;
  f: number;
  ph: number;
}
interface Star {
  x: number;
  y: number;
  r: number;
  sp: number;
  ph: number;
}
interface FogBand {
  x: number;
  y: number;
  s: number;
  v: number;
  a: number;
}
type Bolt = Array<[points: Array<[number, number]>, width: number]>;

export class WeatherScene {
  private ctx: CanvasRenderingContext2D;
  private w = 0;
  private h = 0;
  private cfg: SceneConfig | null = null;
  private kind: WxKind | null = null;
  private night = false;
  private light = false;
  private time = 0;
  private clouds: Cloud[] = [];
  private drops: Drop[] = [];
  private flakes: Flake[] = [];
  private stars: Star[] = [];
  private fogs: FogBand[] = [];
  private shoot: { x: number; y: number; life: number } | null = null;
  private nextShoot = 0;
  private flashT = 99;
  private nextFlash = 0;
  private bolt: Bolt | null = null;

  constructor(private canvas: HTMLCanvasElement) {
    this.ctx = canvas.getContext('2d')!;
  }

  resize() {
    const dpr = Math.min(window.devicePixelRatio || 1, MAX_DPR);
    this.w = this.canvas.clientWidth;
    this.h = this.canvas.clientHeight;
    this.canvas.width = Math.round(this.w * dpr);
    this.canvas.height = Math.round(this.h * dpr);
    this.ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    this.build();
  }

  set(kind: WxKind | null, night: boolean, light: boolean) {
    if (kind === this.kind && night === this.night && light === this.light) return;
    this.kind = kind;
    this.night = night;
    this.light = light;
    this.build();
  }

  private build() {
    const { w, h } = this;
    const cfg = this.kind ? configFor(this.kind, this.night) : null;
    this.cfg = cfg;
    this.time = 0;
    this.clouds = [];
    this.drops = [];
    this.flakes = [];
    this.stars = [];
    this.fogs = [];
    this.shoot = null;
    this.nextShoot = rand(2, 6);
    this.flashT = 99;
    this.nextFlash = rand(1.5, 4);
    this.bolt = null;
    if (!cfg || !w || !h) return;
    const spr = getSprites();
    if (cfg.clouds) {
      const [n, tone, a] = cfg.clouds;
      for (let i = 0; i < n; i++) {
        const s = rand(0.45, 1.05);
        this.clouds.push({
          x: rand(-160, w),
          y: rand(-0.1, 0.3) * h - 20,
          s,
          a: a * rand(0.6, 1),
          v: rand(3, 9) * s,
          spr: spr[tone],
        });
      }
      this.clouds.sort((p, q) => p.s - q.s);
    }
    if (cfg.rain) for (let i = 0; i < cfg.rain[0]; i++) this.drops.push(this.drop(true));
    for (let i = 0; i < (cfg.snow ?? 0); i++) {
      const layer = i % 3;
      this.flakes.push({
        layer,
        x: rand(0, w),
        y: rand(-h, h),
        v: 18 + 34 * LAYERS[layer] * rand(0.8, 1.2),
        amp: rand(8, 24),
        f: rand(0.5, 1.4),
        ph: rand(0, TAU),
      });
    }
    if (!this.light) {
      for (let i = 0; i < (cfg.stars ?? 0); i++) {
        this.stars.push({
          x: rand(0, w),
          y: rand(0, h * 0.72),
          r: rand(0.4, 1.3),
          sp: rand(0.6, 2.2),
          ph: rand(0, TAU),
        });
      }
    }
    for (let i = 0; i < (cfg.fog ?? 0); i++) {
      this.fogs.push({
        x: rand(-640, w),
        y: h * rand(0.25, 0.95) - 80,
        s: rand(0.7, 1.2),
        v: rand(4, 12) * (i % 2 ? 1 : -1),
        a: rand(0.12, 0.26),
      });
    }
  }

  private drop(init: boolean): Drop {
    const [, wind, heavy] = this.cfg!.rain!;
    const layer = Math.random() < 0.45 ? 0 : Math.random() < 0.6 ? 1 : 2;
    const z = LAYERS[layer];
    const len = (9 + 16 * z) * (heavy ? 1.4 : 1);
    return {
      layer,
      v: (420 + 720 * z) * (heavy ? 1.3 : 1),
      len,
      x: rand(-this.h * wind, this.w),
      y: init ? rand(-this.h, this.h) : rand(-this.h * 0.25, -len),
    };
  }

  frame(dt: number) {
    const { ctx, w, h, cfg } = this;
    ctx.clearRect(0, 0, w, h);
    if (!cfg) return;
    const t = (this.time += dt);

    if (cfg.sun) this.drawSun(t, cfg.sun);
    if (cfg.moon) {
      const mx = w * 0.82;
      const my = h * 0.1;
      const g = ctx.createRadialGradient(mx, my, 0, mx, my, 150);
      g.addColorStop(0, `rgba(205,218,255,${0.28 * cfg.moon})`);
      g.addColorStop(1, 'rgba(205,218,255,0)');
      ctx.fillStyle = g;
      ctx.fillRect(0, 0, w, h);
    }
    for (const s of this.stars) {
      ctx.fillStyle = `rgba(235,240,255,${0.2 + 0.6 * (0.5 + 0.5 * Math.sin(t * s.sp + s.ph))})`;
      ctx.beginPath();
      ctx.arc(s.x, s.y, s.r, 0, TAU);
      ctx.fill();
    }
    if (cfg.shoot && !this.light) this.drawShootingStar(t, dt);

    for (const c of this.clouds) {
      c.x += c.v * dt;
      const cw = c.spr.width * c.s;
      if (c.x > w + 20) c.x = -cw;
      ctx.globalAlpha = c.a;
      ctx.drawImage(c.spr, c.x, c.y, cw, c.spr.height * c.s);
    }
    const fogSpr = this.fogs.length ? getSprites().fog : null;
    for (const f of this.fogs) {
      f.x += f.v * dt;
      const fw = fogSpr!.width * f.s;
      if (f.v > 0 && f.x > w) f.x = -fw;
      if (f.v < 0 && f.x < -fw) f.x = w;
      ctx.globalAlpha = f.a * (0.8 + 0.2 * Math.sin(t * 0.3 + f.y));
      ctx.drawImage(fogSpr!, f.x, f.y, fw, fogSpr!.height * f.s);
    }
    ctx.globalAlpha = 1;

    if (this.drops.length) this.drawRain(dt);
    if (this.flakes.length) this.drawSnow(t, dt);
    if (cfg.flash) this.drawFlash(t, dt);
    if (cfg.mist) {
      const g = ctx.createLinearGradient(0, h * 0.5, 0, h);
      g.addColorStop(0, 'rgba(190,210,235,0)');
      g.addColorStop(1, 'rgba(190,210,235,.12)');
      ctx.fillStyle = g;
      ctx.fillRect(0, 0, w, h);
    }
  }

  private drawSun(t: number, strength: number) {
    const { ctx, w, h } = this;
    const sx = w * 0.84;
    const sy = h * 0.06;
    const r = Math.max(w, h) * 0.75;
    ctx.save();
    ctx.globalAlpha = strength;
    let g = ctx.createRadialGradient(sx, sy, 0, sx, sy, r * 0.55);
    g.addColorStop(0, 'rgba(255,226,150,.55)');
    g.addColorStop(0.25, 'rgba(255,200,110,.18)');
    g.addColorStop(1, 'rgba(255,200,110,0)');
    ctx.fillStyle = g;
    ctx.fillRect(0, 0, w, h);
    g = ctx.createRadialGradient(sx, sy, 0, sx, sy, r);
    g.addColorStop(0, 'rgba(255,236,190,.10)');
    g.addColorStop(1, 'rgba(255,236,190,0)');
    ctx.fillStyle = g;
    ctx.translate(sx, sy);
    ctx.rotate(t * 0.02);
    for (let i = 0; i < 9; i++) {
      const a = (i * TAU) / 9;
      const sw = 0.05 + 0.03 * Math.sin(t * 0.4 + i);
      ctx.beginPath();
      ctx.moveTo(0, 0);
      ctx.arc(0, 0, r, a - sw, a + sw);
      ctx.closePath();
      ctx.fill();
    }
    ctx.restore();
  }

  private drawShootingStar(t: number, dt: number) {
    const { ctx, w, h } = this;
    if (!this.shoot && t > this.nextShoot)
      this.shoot = { x: rand(w * 0.2, w * 0.8), y: rand(10, h * 0.25), life: 0 };
    const s = this.shoot;
    if (!s) return;
    s.life += dt;
    const p = s.life / 0.8;
    const hx = s.x + p * 180;
    const hy = s.y + p * 70;
    const g = ctx.createLinearGradient(hx - 70, hy - 27, hx, hy);
    g.addColorStop(0, 'rgba(255,255,255,0)');
    g.addColorStop(1, `rgba(255,255,255,${0.8 * (1 - Math.min(p, 1))})`);
    ctx.strokeStyle = g;
    ctx.lineWidth = 1.4;
    ctx.beginPath();
    ctx.moveTo(hx - 70, hy - 27);
    ctx.lineTo(hx, hy);
    ctx.stroke();
    if (p >= 1) {
      this.shoot = null;
      this.nextShoot = t + rand(5, 11);
    }
  }

  /** 每层合并成一条路径 一次描边 */
  private drawRain(dt: number) {
    const { ctx, h } = this;
    const wind = this.cfg!.rain![1];
    const rgb = this.light ? '50,90,150' : '200,222,255';
    ctx.lineCap = 'round';
    for (let layer = 0; layer < 3; layer++) {
      const z = LAYERS[layer];
      ctx.strokeStyle = `rgba(${rgb},${0.1 + 0.32 * z})`;
      ctx.lineWidth = 0.6 + 0.9 * z;
      ctx.beginPath();
      for (let i = 0; i < this.drops.length; i++) {
        let d = this.drops[i];
        if (d.layer !== layer) continue;
        d.y += d.v * dt;
        d.x += d.v * wind * dt;
        if (d.y - d.len > h) {
          d = this.drops[i] = { ...this.drop(false), layer };
        }
        ctx.moveTo(d.x, d.y);
        ctx.lineTo(d.x - d.len * wind, d.y - d.len);
      }
      ctx.stroke();
    }
  }

  private drawSnow(t: number, dt: number) {
    const { ctx, w, h } = this;
    for (let layer = 0; layer < 3; layer++) {
      const z = LAYERS[layer];
      ctx.fillStyle = `rgba(255,255,255,${0.3 + 0.6 * z})`;
      ctx.beginPath();
      for (const f of this.flakes) {
        if (f.layer !== layer) continue;
        f.y += f.v * dt;
        f.x += (Math.sin(t * f.f + f.ph) * f.amp * 0.9 + 6) * dt;
        if (f.y > h + 4) {
          f.y = -4;
          f.x = rand(0, w);
        }
        if (f.x > w + 4) f.x = -4;
        const r = 0.8 + 2.1 * z;
        ctx.moveTo(f.x + r, f.y);
        ctx.arc(f.x, f.y, r, 0, TAU);
      }
      ctx.fill();
    }
  }

  private drawFlash(t: number, dt: number) {
    const { ctx, w, h } = this;
    this.flashT += dt;
    if (this.flashT > 3 && t > this.nextFlash) {
      this.flashT = 0;
      this.nextFlash = t + rand(3.5, 8);
      this.bolt = this.makeBolt();
    }
    const ft = this.flashT;
    const f =
      ft < 0.07 ? 1 : ft < 0.14 ? 0.15 : ft < 0.24 ? 0.75 : ft < 1 ? 0.75 * Math.exp(-(ft - 0.24) * 6) : 0;
    if (f <= 0.01) return;
    ctx.fillStyle = `rgba(215,220,255,${f * 0.3})`;
    ctx.fillRect(0, 0, w, h);
    if (!this.bolt || f <= 0.08) return;
    ctx.save();
    ctx.strokeStyle = `rgba(245,245,255,${f})`;
    // 仅闪电亮起的几帧开阴影
    ctx.shadowColor = 'rgba(190,200,255,.9)';
    ctx.shadowBlur = 14;
    ctx.lineJoin = 'round';
    for (const [pts, lw] of this.bolt) {
      ctx.lineWidth = lw;
      ctx.beginPath();
      pts.forEach(([x, y], i) => (i ? ctx.lineTo(x, y) : ctx.moveTo(x, y)));
      ctx.stroke();
    }
    ctx.restore();
  }

  private makeBolt(): Bolt {
    const { w, h } = this;
    const walk = (x: number, y: number, endY: number, spread: number, step: number) => {
      const pts: Array<[number, number]> = [[x, y]];
      while (y < endY) {
        y += rand(step * 0.6, step);
        x += rand(-spread, spread);
        pts.push([x, y]);
      }
      return pts;
    };
    const main = walk(rand(w * 0.25, w * 0.75), -5, h * rand(0.45, 0.7), 14, 22);
    const bolt: Bolt = [[main, 2]];
    for (let b = 0; b < 2; b++) {
      const [bx, by] = main[Math.floor(rand(2, main.length - 2))];
      bolt.push([walk(bx, by, by + rand(40, 90), 16, 14), 0.9]);
    }
    return bolt;
  }
}
