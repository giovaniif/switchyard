// An agent's own Slack app wears the agent's face. Slack takes an app icon
// as a PNG of 512 to 2000 pixels (`apps.icon.set`, which yardd calls), and
// only a page can draw the face: so each agent with an app has its avatar
// rendered out of sight here, as the page draws it, turned into a standalone
// SVG, then a 1024-pixel PNG, and sent to yardd (`slack.app.icon`) whenever
// its picture differs from the one Slack has (`iconKey`) and the agent's
// host is online. Taking the picture from the rendered avatar keeps the icon
// right when faces are redrawn.
import { useEffect, useRef } from "react";
import type { Agent } from "./protocol";
import { hostOnline, type Actions, type State } from "./store";
import { AgentAvatar } from "./AgentsPage";
import { agentHost } from "./agents";
import { avatarIconSvg, iconKey, sendsIcon } from "./slack";

const PX = 1024;
/** The styles that decide how a face looks, inlined so the SVG stands alone. */
const PAINTED = ["fill", "fill-opacity", "fill-rule", "stroke", "stroke-width", "stroke-opacity", "stroke-linecap", "stroke-linejoin", "stroke-dasharray", "opacity", "stop-color", "stop-opacity"];

/** The rendered avatar as a standalone SVG: its face's own markup, every
 *  element's paint taken from computed style (colours resolved), no classes
 *  or animation, on its colour. */
export function iconSvgOf(avatar: HTMLElement): string | null {
  const svg = avatar.querySelector("svg");
  if (!svg) return null;
  const clone = svg.cloneNode(true) as SVGSVGElement;
  const from = [svg, ...svg.querySelectorAll("*")];
  const to = [clone, ...clone.querySelectorAll("*")];
  from.forEach((el, i) => {
    const css = getComputedStyle(el);
    const out = to[i] as Element;
    for (const prop of PAINTED) {
      const v = css.getPropertyValue(prop);
      if (v) out.setAttribute(prop, v);
    }
    out.removeAttribute("class");
    out.removeAttribute("style");
  });
  // The face fills the square as it fills the circle on the page.
  for (const [k, v] of Object.entries({ x: "0", y: "0", width: "24", height: "24" })) clone.setAttribute(k, v);
  if (!clone.getAttribute("viewBox")) clone.setAttribute("viewBox", "0 0 24 24");
  const css = getComputedStyle(avatar);
  return avatarIconSvg(new XMLSerializer().serializeToString(clone), {
    background: css.backgroundColor,
    light: css.getPropertyValue("--avatar-light"),
    clear: css.getPropertyValue("--avatar-light-clear"),
  }, PX);
}

/** The SVG drawn at `PX` pixels, as base64 PNG. */
async function rasterize(svg: string): Promise<string> {
  const img = new Image(PX, PX);
  img.src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
  await img.decode();
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = PX;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("no canvas");
  ctx.drawImage(img, 0, 0, PX, PX);
  const blob = await new Promise<Blob | null>((done) => canvas.toBlob(done, "image/png"));
  if (!blob) throw new Error("no PNG");
  const bytes = new Uint8Array(await blob.arrayBuffer());
  let bin = "";
  for (let i = 0; i < bytes.length; i += 0x8000) bin += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(bin);
}

function IconSource({ agent, has, online, actions }: { agent: Agent; has: string; online: boolean; actions: Actions }) {
  const ref = useRef<HTMLSpanElement>(null);
  // One try per picture in this window: a refusal is shown in its dialog.
  const tried = useRef("");
  useEffect(() => {
    const avatar = ref.current?.querySelector<HTMLElement>(".agent-avatar");
    const svg = avatar && iconSvgOf(avatar);
    if (!svg) return;
    const key = iconKey(svg);
    if (!sendsIcon(key, has, tried.current, online)) return;
    tried.current = key;
    rasterize(svg)
      .then((png) => actions.slackApp("slack.app.icon", { agentId: agent.id, key, png }))
      .catch(() => {});
  }, [agent.id, agent.face, agent.color, has, online, actions]);
  return (
    <span ref={ref}>
      <AgentAvatar agent={agent} size="xl" still />
    </span>
  );
}

/** Mounted once: keeps every agent app's icon in step with its agent's face. */
export function SlackIcons({ state, actions }: { state: State; actions: Actions }) {
  const apps = state.slack?.agentApps;
  if (!apps) return null;
  const agents = state.agents.filter((a) => apps[a.id]);
  if (!agents.length) return null;
  return (
    <div className="slack-icon-source" aria-hidden="true">
      {agents.map((a) => (
        <IconSource key={a.id} agent={a} has={apps[a.id].iconKey ?? ""} online={hostOnline(state, agentHost(a))} actions={actions} />
      ))}
    </div>
  );
}
