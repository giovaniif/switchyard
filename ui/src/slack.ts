// Slack on the Connectors page: the owner's own Slack (its steps and its
// connection's status line), who may talk to agents, how a Slack message is
// labelled in a thread; for an agent's own app, the steps it still takes and
// the picture of its face that becomes its icon. The connections themselves
// live in yardd (slack.rs).
import type { MessageOrigin, SlackAgentApp, SlackOrigin, SlackView } from "./protocol";

export type Tone = "ok" | "warn" | "bad" | "muted";

/** The card's state word and the line under it: the owner's own Slack. */
export function slackStatus(view: SlackView): { label: string; tone: Tone; line: string } {
  const me = view.me;
  const intro = "Your direct messages and the channels you choose";
  if (!me) return { label: "Not connected", tone: "muted", line: intro };
  const team = me.teamName ? ` in ${me.teamName}` : "";
  if (me.step === "install") return { label: "Approve in Slack", tone: "warn", line: `Your app is made${team}. Approve it in Slack.` };
  if (me.step === "appToken") return { label: "One step left", tone: "warn", line: `Approved${me.userName ? ` by ${me.userName}` : ""}${team}. Paste its connection token.` };
  switch (me.state) {
    case "connected":
      return { label: "Connected", tone: "ok", line: `As @${me.handle || me.userName || "you"}${team}` };
    case "error":
      return { label: "Not working", tone: "bad", line: me.error || "The connection failed." };
    default:
      return { label: "Connecting…", tone: "muted", line: me.teamName ? `To ${me.teamName}` : "Opening Socket Mode" };
  }
}

/** The owner's own Slack's steps, as the card numbers them, and which one he is at. */
export function mySteps(view: SlackView): { steps: string[]; at: number } {
  const steps = ["Token", "Approve", "Connect"];
  const me = view.me;
  return { steps, at: !me ? 0 : me.step === "install" ? 1 : me.step === "appToken" ? 2 : steps.length };
}

export function isSlack(origin: MessageOrigin | undefined): origin is SlackOrigin {
  return origin?.kind === "slack";
}

/** The badge's words over a message from Slack: `From Gio on Slack · #general`,
 *  or for a watched channel's notice `Watched on Slack · #alerts · 3 messages`. */
export function slackFrom(origin: SlackOrigin): string {
  const who = origin.userName || origin.user || "someone";
  const place = origin.channelName || origin.channel;
  if (origin.watched) {
    const n = origin.count ?? 1;
    return [`Watched on Slack`, place, n === 1 ? `from ${who}` : `${n} messages`].filter(Boolean).join(" · ");
  }
  return place ? `From ${who} on Slack · ${place}` : `From ${who} on Slack`;
}

/** Who may talk to agents, as a sentence for the card. */
export function allowedSummary(view: SlackView): string {
  const n = view.allowed.length;
  if (n === 0) return "Nobody yet. Whoever approves an agent's app in Slack can, and others who write to it ask here.";
  if (n === 1) return `Only ${view.allowed[0].name}.`;
  return `${n} people.`;
}

/** The steps of putting an agent in Slack, as its dialog numbers them. Over
 *  HTTP (a public address) Slack calls Switchyard, so there is no connection
 *  token to paste and no Connect step. */
export function setupSteps(view: SlackView | null | undefined, app: SlackAgentApp | undefined): string[] {
  const http = app ? !!app.eventsUrl : !!view?.publicUrl;
  return http ? ["Token", "Approve"] : ["Token", "Approve", "Connect"];
}

/** How many tokens a new agent app takes here: the configuration token,
 *  the bot token when Slack cannot send the browser back to Switchyard, and
 *  the app-level token when its events come over Socket Mode. */
export function tokensNeeded(view: SlackView | null | undefined): number {
  return 1 + (view?.canRedirect ? 0 : 1) + (view?.publicUrl ? 0 : 1);
}

/** The colours of an avatar as the page paints it. */
export interface AvatarPaint {
  /** Its colour. */
  background: string;
  /** The light from above, and where it has faded out (at 70%). */
  light: string;
  clear: string;
}

/** A colour from computed style, safe inside an attribute; else null. */
const paint = (c: string) => (/^[#a-zA-Z0-9(),.%\s-]{1,64}$/.test(c.trim()) ? c.trim() : null);

/** The agent's avatar as a standalone square SVG, for Slack's app icon: its
 *  face (`face`, the rendered avatar's own `<svg>` with its styles inlined)
 *  on its colour, lit from above as on the page. Slack rounds the corners.
 *  Null when its colour is not one. */
export function avatarIconSvg(face: string, colors: AvatarPaint, px = 1024): string | null {
  const bg = paint(colors.background);
  if (!bg) return null;
  const light = paint(colors.light) ?? "transparent";
  const clear = paint(colors.clear) ?? "transparent";
  return (
    `<svg xmlns="http://www.w3.org/2000/svg" width="${px}" height="${px}" viewBox="0 0 24 24">` +
    `<defs><linearGradient id="yard-light" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="${light}"/><stop offset="0.7" stop-color="${clear}"/></linearGradient></defs>` +
    `<rect width="24" height="24" fill="${bg}"/><rect width="24" height="24" fill="url(#yard-light)"/>` +
    face +
    `</svg>`
  );
}

/** A short key for an icon's SVG. yardd keeps the key of the icon Slack
 *  has, so a window sends a picture only when the face, its colour or its
 *  drawing changed, and every window agrees on the key. */
export function iconKey(svg: string): string {
  let a = 0x811c9dc5;
  let b = (0x01000193 ^ svg.length) >>> 0;
  for (let i = 0; i < svg.length; i++) {
    const c = svg.charCodeAt(i);
    a = Math.imul(a ^ c, 0x01000193) >>> 0;
    b = Math.imul(b ^ c, 0x5bd1e995) >>> 0;
  }
  return `v1-${a.toString(16).padStart(8, "0")}${b.toString(16).padStart(8, "0")}`;
}
