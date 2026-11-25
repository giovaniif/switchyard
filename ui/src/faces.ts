// Agent faces: small 3D characters drawn in SVG. One description builds both
// the live avatar (AgentAvatar renders it as React elements, CSS moves its
// parts) and a standalone SVG string (`avatarSvg`) whose colours and
// gradients all live inside it, so serializing it and rasterizing it gives
// the same picture (an agent's Slack app icon). Pure, so bun test covers it.
import type { Agent, AgentColor, AgentFace } from "./protocol";

type RGB = readonly [number, number, number];

/** The agent colours as channels: the same values as the `--agent-*` tokens in
 *  styles.css (a test keeps them equal). Channels, because the head's shading
 *  is computed from them and the SVG has to carry its own colours. */
export const AGENT_RGB: Record<AgentColor, RGB> = {
  blue: [47, 111, 237],
  green: [31, 138, 76],
  amber: [183, 121, 31],
  rose: [194, 65, 107],
  violet: [122, 63, 209],
  teal: [15, 139, 141],
  orange: [194, 86, 12],
  slate: [85, 96, 112],
};
const WHITE: RGB = [255, 255, 255];
const BLACK: RGB = [0, 0, 0];
const BLUSH: RGB = [255, 122, 150];
const TONGUE: RGB = [255, 116, 138];
const EYE_SHADE: RGB = [214, 220, 234];

const mix = (a: RGB, b: RGB, t: number): RGB => [0, 1, 2].map((i) => Math.round(a[i] + (b[i] - a[i]) * t)) as unknown as RGB;
const hex = (c: RGB) => "#" + c.map((v) => v.toString(16).padStart(2, "0")).join("");

/** An expression: the parts that differ between the eight faces. Geometry is
 *  in a 64-unit box with the head at (32, 34), radius 24. */
export interface Expression {
  /** Open round eyes, square screen eyes (bot), or the right one shut (wink). */
  eyes: "round" | "screen" | "wink";
  /** Upper lids resting part-way down, 0 open to 1 shut. */
  lid?: number;
  /** Lower lids pushed up by a big smile. */
  smiling?: boolean;
  /** Where the pupils rest, in units from the eye's centre. */
  gaze?: [number, number];
  /** An open mouth is filled; `tongue` is its centre height. */
  mouth: { d: string; open: boolean; tongue?: number; teeth?: boolean };
  glasses?: boolean;
  brows?: string;
  antenna?: boolean;
  /** Resting head tilt in degrees. */
  tilt?: number;
}

export const FACES: Record<AgentFace, Expression> = {
  smile: { eyes: "round", mouth: { d: "M25.5 41.5q6.5 6 13 0", open: false } },
  grin: { eyes: "round", gaze: [0, 0.4], mouth: { d: "M24.5 40.2h15a7.5 7.5 0 0 1-15 0Z", open: true, tongue: 44.6, teeth: true } },
  wink: { eyes: "wink", gaze: [0.6, 0.4], mouth: { d: "M25.5 41.8q7 6 13-1.2", open: false }, tilt: -5 },
  calm: { eyes: "round", lid: 0.42, gaze: [0, 1], mouth: { d: "M27 42.2q5 3.6 10 0", open: false } },
  glasses: { eyes: "round", glasses: true, mouth: { d: "M27 42.6q5 3.4 10 0", open: false } },
  bot: { eyes: "screen", antenna: true, mouth: { d: "M26 40.5h12a2.75 2.75 0 0 1 0 5.5h-12a2.75 2.75 0 0 1 0-5.5Z", open: true, teeth: true } },
  happy: { eyes: "round", smiling: true, mouth: { d: "M23.5 39.6h17a8.5 8.5 0 0 1-17 0Z", open: true, tongue: 44.8 } },
  curious: { eyes: "round", gaze: [0.8, -1], brows: "M17.5 23.6q4.5-1.2 9 0M36.5 21.8q5-4 10-0.6", mouth: { d: "M29.3 44.4a2.7 3.1 0 1 0 5.4 0a2.7 3.1 0 1 0-5.4 0Z", open: true }, tilt: 7 },
};
export const AGENT_FACES = Object.keys(FACES) as AgentFace[];

/** The face an agent shows: its own, or a steady one picked from its id. */
export function faceOf(agent: Pick<Agent, "face" | "id">): AgentFace {
  if (agent.face && agent.face in FACES) return agent.face;
  let h = 0;
  for (const ch of agent.id) h = (h * 31 + ch.charCodeAt(0)) >>> 0;
  return AGENT_FACES[h % AGENT_FACES.length];
}

/** A steady offset in seconds (negative: already part-way through), so agents
 *  side by side do not blink together. */
export function blinkDelay(id: string): number {
  let h = 7;
  for (const ch of id) h = (h * 33 + ch.charCodeAt(0)) >>> 0;
  return -((h % 50) / 10);
}

/** How much to draw: at 14-20 px the head fills the box and loses its shadow,
 *  rim and cheeks so the eyes stay readable; at 88 px it gets every sparkle. */
export type Detail = "low" | "mid" | "high";
export const detailFor = (px: number): Detail => (px <= 24 ? "low" : px >= 64 ? "high" : "mid");

/** An SVG element, before it becomes React elements or text. */
export interface SvgNode {
  tag: string;
  attrs: Record<string, string | number>;
  children: SvgNode[];
}
type Child = SvgNode | false | null | undefined;
// Geometry is computed, so numbers are cut to two decimals to keep the markup short.
const short = (v: string | number) => (typeof v === "number" ? +v.toFixed(2) : v.replace(/-?\d+\.\d{3,}/g, (m) => String(+(+m).toFixed(2))));
const h = (tag: string, attrs: Record<string, string | number | undefined> = {}, ...children: Child[]): SvgNode => ({
  tag,
  attrs: Object.fromEntries(Object.entries(attrs).flatMap(([k, v]) => (v === undefined ? [] : [[k, short(v)]]))),
  children: children.filter((c): c is SvgNode => !!c),
});
const stop = (offset: number, color: RGB, opacity = 1) => h("stop", { offset, "stop-color": hex(color), "stop-opacity": opacity === 1 ? undefined : opacity });

const EYE_Y = 31;
const EYE_DX = 9.5;

/** The face as a tree of SVG elements. `uid` makes its gradient and clip ids
 *  unique on a page full of faces. Class names are what the CSS animates:
 *  face-body hops, face-bob breathes, face-tilt tilts, face-feature and
 *  face-look glance, face-lids blink, face-mouth talks, face-follow and
 *  face-follow-eyes follow the pointer. Without that CSS it is a still pose. */
export function faceTree(agent: { color: AgentColor; face: AgentFace }, detail: Detail, uid: string): SvgNode {
  const x = FACES[agent.face];
  const base = AGENT_RGB[agent.color] ?? AGENT_RGB.slate;
  const light = mix(base, WHITE, 0.5);
  const dark = mix(base, BLACK, 0.36);
  const ink = mix(base, BLACK, 0.8);
  const id = (n: string) => `${uid}-${n}`;
  const url = (n: string) => `url(#${id(n)})`;
  const low = detail === "low";
  const high = detail === "high";
  // Small faces get bigger eyes and thicker lines.
  const eyeK = low ? 1.2 : 1;
  const rx = 6 * eyeK;
  const ry = 7 * eyeK;
  const line = low ? 3.4 : 2.4;
  const sides = x.eyes === "wink" ? [-1] : [-1, 1];
  const eyeX = (s: number) => 32 + s * EYE_DX;
  const eyeShape = (s: number) =>
    x.eyes === "screen" ? h("rect", { x: eyeX(s) - rx, y: EYE_Y - ry * 0.85, width: rx * 2, height: ry * 1.7, rx: 2.6 * eyeK }) : h("ellipse", { cx: eyeX(s), cy: EYE_Y, rx, ry });
  const top = EYE_Y - (x.eyes === "screen" ? ry * 0.85 : ry);
  const eyeH = x.eyes === "screen" ? ry * 1.7 : ry * 2;
  // The blinking lid, open: it and its edge line sit wholly above the eye.
  const lid = (s: number) => {
    const y = top - 1.8;
    const x0 = eyeX(s) - rx - 1;
    const x1 = eyeX(s) + rx + 1;
    return `M${x0} ${y - eyeH - 2}H${x1}V${y}Q${eyeX(s)} ${y + 2.2} ${x0} ${y}Z`;
  };
  // The skin gradient's colour at a point, for parts that move across it.
  const skinAt = (px: number, py: number) => {
    const t = Math.min(1, Math.hypot(px - 25, py - 23) / 36);
    return t < 0.42 ? mix(light, base, t / 0.42) : mix(base, dark, (t - 0.42) / 0.58);
  };
  const [gx, gy] = x.gaze ?? [0, 0.6];
  const pupilR = (x.eyes === "screen" ? 2.9 : 3.3) * eyeK;

  const defs = h(
    "defs",
    {},
    h("radialGradient", { id: id("skin"), gradientUnits: "userSpaceOnUse", cx: 25, cy: 23, r: 36 }, stop(0, light), stop(0.42, base), stop(1, dark)),
    h("radialGradient", { id: id("spec"), cx: 0.5, cy: 0.5, r: 0.5 }, stop(0, WHITE, 0.85), stop(1, WHITE, 0)),
    !low && h("linearGradient", { id: id("rim"), x1: 0.2, y1: 0.1, x2: 0.85, y2: 1 }, stop(0.55, WHITE, 0), stop(1, WHITE, 0.6)),
    !low && h("radialGradient", { id: id("shadow"), cx: 0.5, cy: 0.5, r: 0.5 }, stop(0, BLACK, 0.3), stop(1, BLACK, 0)),
    h("radialGradient", { id: id("eye"), cx: 0.45, cy: 0.35, r: 0.75 }, stop(0.6, WHITE), stop(1, EYE_SHADE)),
    h("clipPath", { id: id("eyes") }, ...sides.map((s) => eyeShape(s))),
  );

  const antenna =
    x.antenna &&
    !low &&
    h(
      "g",
      {},
      h("path", { d: "M32 12V5.5", stroke: hex(dark), "stroke-width": 2.4, "stroke-linecap": "round" }),
      h("circle", { cx: 32, cy: 4.2, r: 3.2, fill: url("skin") }),
      h("circle", { cx: 31, cy: 3.2, r: 1, fill: hex(WHITE), "fill-opacity": 0.8 }),
      h("circle", { cx: 8.4, cy: 34, r: 3.4, fill: hex(dark) }),
      h("circle", { cx: 55.6, cy: 34, r: 3.4, fill: hex(dark) }),
    );
  const head = h(
    "g",
    {},
    h("circle", { cx: 32, cy: 34, r: 24, fill: url("skin") }),
    !low && h("circle", { cx: 32, cy: 34, r: 23, fill: "none", stroke: url("rim"), "stroke-width": 2 }),
    h("ellipse", { cx: 23.5, cy: 20.5, rx: 9, ry: 5.5, fill: url("spec"), transform: "rotate(-35 23.5 20.5)" }),
    high && h("circle", { cx: 20, cy: 19, r: 1.6, fill: hex(WHITE), "fill-opacity": 0.85 }),
  );

  const cheeks = !low && h("g", { fill: hex(BLUSH), "fill-opacity": x.smiling ? 0.6 : 0.42 }, h("ellipse", { cx: 17, cy: 41, rx: 4, ry: 2.5 }), h("ellipse", { cx: 47, cy: 41, rx: 4, ry: 2.5 }));
  const whites = h("g", { fill: url("eye") }, ...sides.map((s) => eyeShape(s)));
  const pupils = h(
    "g",
    { class: "face-follow-eyes" },
    h(
      "g",
      { class: "face-look" },
      ...sides.map((s) =>
        h(
          "g",
          {},
          h("circle", { cx: eyeX(s) + gx, cy: EYE_Y + gy, r: pupilR, fill: hex(ink) }),
          h("circle", { cx: eyeX(s) + gx + 1.2 * eyeK, cy: EYE_Y + gy - 1.4 * eyeK, r: (low ? 1.2 : 1.15) * eyeK, fill: hex(WHITE) }),
          high && h("circle", { cx: eyeX(s) + gx - 1.1, cy: EYE_Y + gy + 1.3, r: 0.55, fill: hex(WHITE), "fill-opacity": 0.8 }),
        ),
      ),
    ),
  );
  // Resting lids (calm's heavy lids, happy's cheeks pushing up) are drawn
  // unclipped in the head's own gradient, so they melt into it with no seam;
  // only the upper lid's edge line is clipped to the eye.
  const rest = (s: number) => {
    const x0 = eyeX(s) - rx - 0.6;
    const x1 = eyeX(s) + rx + 0.6;
    const y = top + eyeH * (x.lid ?? 0);
    const yb = EYE_Y + ry * 0.5;
    return {
      upper: `M${x0} ${top - 1.2}H${x1}V${y}Q${eyeX(s)} ${y - 3} ${x0} ${y}Z`,
      edge: `M${x0} ${y}Q${eyeX(s)} ${y - 3} ${x1} ${y}`,
      lower: `M${x0} ${EYE_Y + ry + 1.2}V${yb}Q${eyeX(s)} ${EYE_Y - ry * 0.15} ${x1} ${yb}V${EYE_Y + ry + 1.2}Z`,
    };
  };
  const restLids =
    (x.lid || x.smiling) &&
    h(
      "g",
      {},
      h("g", { fill: url("skin") }, ...sides.flatMap((s) => [x.lid ? h("path", { d: rest(s).upper }) : null, x.smiling ? h("path", { d: rest(s).lower }) : null])),
      x.lid ? h("g", { "clip-path": url("eyes"), stroke: hex(ink), "stroke-opacity": 0.45, "stroke-width": 1 }, ...sides.map((s) => h("path", { d: rest(s).edge }))) : null,
    );
  // Open lids rest just above the eye; the blink slides them down by their
  // own height. Flat skin colour: a gradient would slide with them.
  const lids = h(
    "g",
    { "clip-path": url("eyes") },
    h("g", { class: "face-lids", stroke: hex(ink), "stroke-opacity": 0.45, "stroke-width": 1 }, ...sides.map((s) => h("path", { d: lid(s), fill: hex(skinAt(eyeX(s), EYE_Y)) }))),
  );
  const shut = x.eyes === "wink" && h("path", { d: `M${eyeX(1) - rx} ${EYE_Y + 1}q${rx} ${-ry * 0.9} ${rx * 2} 0`, fill: "none", stroke: hex(ink), "stroke-width": line, "stroke-linecap": "round" });
  const lens = rx + (low ? 1 : 1.8);
  const gap = EYE_DX * 2 - lens * 2;
  const glasses =
    x.glasses &&
    h(
      "g",
      { fill: "none", stroke: hex(ink), "stroke-width": low ? 2.6 : 2, "stroke-linecap": "round" },
      ...sides.map((s) => h("circle", { cx: eyeX(s), cy: EYE_Y, r: lens, fill: hex(WHITE), "fill-opacity": 0.12 })),
      h("path", { d: `M${eyeX(-1) + lens} ${EYE_Y - 1}q${gap / 2} -2.5 ${gap} 0` }),
      !low && h("path", { d: `M${eyeX(-1) - lens} ${EYE_Y - 1}L9.5 ${EYE_Y - 2.5}M${eyeX(1) + lens} ${EYE_Y - 1}L54.5 ${EYE_Y - 2.5}` }),
      !low && h("path", { d: `M${eyeX(-1) - 4} ${EYE_Y - 3}l2.5-2.5M${eyeX(1) - 4} ${EYE_Y - 3}l2.5-2.5`, stroke: hex(WHITE), "stroke-opacity": 0.7, "stroke-width": 1.4 }),
    );
  const brows = x.brows && !low ? h("path", { d: x.brows, fill: "none", stroke: hex(ink), "stroke-width": 2.2, "stroke-linecap": "round" }) : null;

  const m = x.mouth;
  const mouth = h(
    "g",
    { class: "face-mouth" },
    m.open
      ? h(
          "g",
          {},
          h("path", { d: m.d, fill: hex(ink) }),
          m.tongue && !low ? h("ellipse", { cx: 32, cy: m.tongue, rx: 3.6, ry: 2, fill: hex(TONGUE) }) : null,
          m.teeth && !low && x.eyes !== "screen" && h("rect", { x: 26.5, y: 40.2, width: 11, height: 1.8, rx: 0.9, fill: hex(WHITE) }),
          m.teeth && !low && x.eyes === "screen" && h("path", { d: "M29 41.6v2.8M32 41.6v2.8M35 41.6v2.8", stroke: hex(light), "stroke-width": 1.2, "stroke-linecap": "round" }),
        )
      : h("path", { d: m.d, fill: "none", stroke: hex(ink), "stroke-width": line, "stroke-linecap": "round" }),
  );

  const face = h("g", { class: "face-follow" }, h("g", { class: "face-feature" }, whites, pupils, restLids, lids, cheeks, shut, glasses, brows, mouth));
  // Small faces: the head fills the box (no room for a shadow) and grows 25 %.
  // Card-sized ones grow 12 % and sit on a flatter shadow, so they read as
  // large as the plain circles they replaced.
  const fit = low ? "matrix(1.25 0 0 1.25 -8 -10.5)" : high ? undefined : "matrix(1.12 0 0 1.12 -3.84 -3.08)";
  return h(
    "svg",
    { viewBox: "0 0 64 64", fill: "none" },
    defs,
    !low && h("ellipse", { class: "face-shadow", cx: 32, cy: high ? 60.5 : 62.2, rx: high ? 17 : 18, ry: high ? 3.2 : 2.4, fill: url("shadow") }),
    h(
      "g",
      { class: "face-body" },
      h("g", { class: "face-bob" }, h("g", { class: "face-tilt" }, h("g", { transform: fit }, h("g", { transform: x.tilt ? `rotate(${x.tilt} 32 34)` : undefined }, antenna, head, face)))),
    ),
  );
}

const esc = (v: string | number) => String(v).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/"/g, "&quot;");
function serialize(n: SvgNode): string {
  const attrs = Object.entries(n.attrs)
    .map(([k, v]) => ` ${k}="${esc(v)}"`)
    .join("");
  return n.children.length ? `<${n.tag}${attrs}>${n.children.map(serialize).join("")}</${n.tag}>` : `<${n.tag}${attrs}/>`;
}

/** A steady id prefix for one standalone picture. */
function prefixFor(s: string): string {
  let h = 5381;
  for (const ch of s) h = (h * 33 + ch.charCodeAt(0)) >>> 0;
  return `yf${h.toString(36)}`;
}

/** The agent's face as a standalone SVG document at `sizePx`, in its still
 *  pose: every colour and gradient inside it, ids unique to it (`idPrefix`
 *  for several in one page), so it rasterizes the same anywhere. */
export function avatarSvg(agent: Pick<Agent, "color"> & Partial<Pick<Agent, "face" | "id">>, sizePx: number, idPrefix?: string): string {
  const face = faceOf({ face: agent.face ?? "", id: agent.id ?? "" });
  const tree = faceTree({ color: agent.color, face }, detailFor(sizePx), idPrefix ?? prefixFor(`${agent.id ?? ""}:${agent.color}:${face}:${sizePx}`));
  tree.attrs = { xmlns: "http://www.w3.org/2000/svg", width: sizePx, height: sizePx, ...tree.attrs };
  return serialize(tree);
}
