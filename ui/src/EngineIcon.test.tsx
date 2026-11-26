import { describe, expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";
import { ENGINE_LOGOS, EngineIcon } from "./EngineIcon";
import { ENGINES } from "./engines";

describe("EngineIcon", () => {
  test("every engine has its own mark", () => {
    for (const engine of ENGINES) expect(ENGINE_LOGOS[engine]?.paths.length).toBeGreaterThan(0);
    const marks = new Set(ENGINES.map((e) => ENGINE_LOGOS[e].paths[0].d));
    expect(marks.size).toBe(ENGINES.length);
  });

  test("two Gemini marks on a page keep their own gradients", () => {
    const html = renderToStaticMarkup(
      <>
        <EngineIcon engine="gemini" />
        <EngineIcon engine="gemini" />
      </>,
    );
    const svgs = html.split("</svg>").filter(Boolean);
    expect(svgs).toHaveLength(2);
    const ids = svgs.map((svg) => [...svg.matchAll(/ id="([^"]+)"/g)].map((m) => m[1]));
    expect(ids[0]).toHaveLength(3);
    expect(ids[0].some((id) => ids[1].includes(id))).toBe(false);
    for (const [k, svg] of svgs.entries()) {
      const refs = [...svg.matchAll(/url\(#([^)]+)\)/g)].map((m) => m[1]);
      expect(refs).toHaveLength(3);
      for (const ref of refs) expect(ids[k]).toContain(ref);
    }
  });

  test("decorative by default, an image when labelled; ink marks follow the text colour", () => {
    const plain = renderToStaticMarkup(<EngineIcon engine="codex" />);
    expect(plain).toContain('aria-hidden="true"');
    expect(plain).toContain('fill="currentColor"');
    expect(plain).not.toMatch(/<path[^>]* style=/);
    const named = renderToStaticMarkup(<EngineIcon engine="claude" label="Claude" />);
    expect(named).toContain('role="img"');
    expect(named).toContain('aria-label="Claude"');
    expect(named).toContain("fill:var(--brand-claude)");
  });
});
