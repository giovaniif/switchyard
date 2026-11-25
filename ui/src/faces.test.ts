import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import type { AgentColor } from "./protocol";
import { AGENT_COLORS } from "./agents";
import { AGENT_FACES, AGENT_RGB, avatarSvg, detailFor, faceTree, type SvgNode } from "./faces";

const ids = (svg: string) => [...svg.matchAll(/\bid="([^"]+)"/g)].map((m) => m[1]);
const refs = (svg: string) => [...svg.matchAll(/url\(#([^)]+)\)/g)].map((m) => m[1]);
const hex = (c: readonly number[]) => "#" + c.map((v) => v.toString(16).padStart(2, "0")).join("");
function find(n: SvgNode, cls: string): SvgNode | null {
  if (String(n.attrs.class ?? "").split(" ").includes(cls)) return n;
  for (const c of n.children) {
    const hit = find(c, cls);
    if (hit) return hit;
  }
  return null;
}

test("avatarSvg is a standalone picture: its own colours, gradients and ids, nothing from the page", () => {
  for (const face of AGENT_FACES) {
    for (const color of AGENT_COLORS) {
      for (const px of [14, 32, 88, 512]) {
        const svg = avatarSvg({ id: "g1", color, face }, px);
        expect(svg.startsWith('<svg xmlns="http://www.w3.org/2000/svg"')).toBe(true);
        expect(svg).toContain(`width="${px}" height="${px}"`);
        // Rasterized alone (a Slack icon), there is no stylesheet: no CSS
        // variables, no currentColor, and the agent's colour is in the file.
        expect(svg).not.toMatch(/var\(|currentColor|style=/);
        expect(svg).toContain(hex(AGENT_RGB[color]));
        // Every gradient or clip it uses is defined inside it, once.
        const own = ids(svg);
        expect(new Set(own).size).toBe(own.length);
        for (const r of refs(svg)) expect(own).toContain(r);
        expect(refs(svg).length).toBeGreaterThan(0);
        // Well formed: every opened element is closed.
        const open = [...svg.matchAll(/<([a-zA-Z]+)[^>]*?(\/?)>/g)].filter((m) => !m[2]).length;
        expect([...svg.matchAll(/<\/[a-zA-Z]+>/g)].length).toBe(open);
      }
    }
  }
});

test("avatarSvg: steady for one agent, ids that do not clash between pictures", () => {
  const a = avatarSvg({ id: "g1", color: "teal", face: "bot" }, 88);
  expect(avatarSvg({ id: "g1", color: "teal", face: "bot" }, 88)).toBe(a);
  const b = avatarSvg({ id: "g2", color: "teal", face: "bot" }, 88);
  expect(ids(a).some((i) => ids(b).includes(i))).toBe(false);
  const c = avatarSvg({ id: "g1", color: "teal", face: "bot" }, 88, "slack");
  expect(ids(c).every((i) => i.startsWith("slack-"))).toBe(true);
  // An agent without a face gets the one picked from its id, like on its card.
  expect(avatarSvg({ id: "g1", color: "teal" }, 32)).toBe(avatarSvg({ id: "g1", color: "teal", face: "" }, 32));
});

test("faces: the moving parts are there for every face, detail drops at small sizes", () => {
  expect(detailFor(14)).toBe("low");
  expect(detailFor(20)).toBe("low");
  expect(detailFor(32)).toBe("mid");
  expect(detailFor(48)).toBe("mid");
  expect(detailFor(88)).toBe("high");
  for (const face of AGENT_FACES) {
    for (const detail of ["low", "mid", "high"] as const) {
      const t = faceTree({ color: "blue", face }, detail, "t");
      for (const part of ["face-body", "face-bob", "face-tilt", "face-feature", "face-look", "face-lids", "face-mouth", "face-follow", "face-follow-eyes"]) expect(find(t, part)).not.toBeNull();
      // No room for a contact shadow when the head fills a 14 px box.
      expect(find(t, "face-shadow") === null).toBe(detail === "low");
    }
  }
  // Fewer, bigger parts at 14 px than at 88 px.
  expect(avatarSvg({ id: "g", color: "rose", face: "glasses" }, 14).length).toBeLessThan(avatarSvg({ id: "g", color: "rose", face: "glasses" }, 88).length);
});

test("the face palette matches the --agent-* colour tokens in styles.css", () => {
  const css = readFileSync(join(import.meta.dir, "styles.css"), "utf8");
  for (const c of Object.keys(AGENT_RGB) as AgentColor[]) {
    const token = css.match(new RegExp(`--agent-${c}:\\s*(#[0-9a-fA-F]{6})`))?.[1];
    expect(token?.toLowerCase()).toBe(hex(AGENT_RGB[c]));
  }
  expect(Object.keys(AGENT_RGB).sort()).toEqual([...AGENT_COLORS].sort());
});
