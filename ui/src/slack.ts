// Slack on the Connectors page: the app manifest the owner pastes at
// api.slack.com, token checks before anything is sent, the connection's
// status line, and how a Slack message is labelled in a thread; for an
// agent's own app, the steps it still takes and the picture of its face that
// becomes its icon. The connection itself lives in yardd (slack.rs).
import type { MessageOrigin, SlackAgentApp, SlackOrigin, SlackView } from "./protocol";

/** Socket Mode on (no public address needed), a bot that reads DMs and
 *  mentions, follow-ups in threads it answered, and reacts and posts. */
export const SLACK_MANIFEST = {
  display_information: {
    name: "Switchyard",
    description: "Talk to your Switchyard agents.",
  },
  features: {
    app_home: { home_tab_enabled: false, messages_tab_enabled: true, messages_tab_read_only_enabled: false },
    bot_user: { display_name: "switchyard", always_online: true },
  },
  oauth_config: {
    scopes: {
      bot: [
        "app_mentions:read",
        "chat:write",
        "im:history",
        "im:read",
        "im:write",
        "channels:history",
        "channels:read",
        "groups:history",
        "groups:read",
        "users:read",
        "reactions:write",
      ],
    },
  },
  settings: {
    event_subscriptions: { bot_events: ["app_mention", "message.im", "message.channels", "message.groups"] },
    interactivity: { is_enabled: false },
    org_deploy_enabled: false,
    socket_mode_enabled: true,
    token_rotation_enabled: false,
  },
};

export const manifestText = () => JSON.stringify(SLACK_MANIFEST, null, 2);

/** Slack's "Create an app" page with the manifest filled in. */
export const createAppUrl = () => `https://api.slack.com/apps?new_app=1&manifest_json=${encodeURIComponent(JSON.stringify(SLACK_MANIFEST))}`;

/** What is wrong with a pasted token, before it is sent; null when it looks right. */
export function tokenProblem(kind: "app" | "bot", value: string): string | null {
  const token = value.trim();
  if (!token) return null;
  if (/\s/.test(token)) return "A token is one word with no spaces.";
  if (kind === "app") {
    if (token.startsWith("xoxb-")) return "That is the bot token. The app-level token starts with xapp-.";
    if (!token.startsWith("xapp-")) return "The app-level token starts with xapp-.";
  } else {
    if (token.startsWith("xapp-")) return "That is the app-level token. The bot token starts with xoxb-.";
    if (token.startsWith("xoxp-")) return "That is a user token. Use the Bot User OAuth Token, which starts with xoxb-.";
    if (!token.startsWith("xoxb-")) return "The bot token starts with xoxb-.";
  }
  return token.length < 12 ? "That token is too short." : null;
}

export type Tone = "ok" | "warn" | "bad" | "muted";

/** The card's state word and the line under it. */
export function slackStatus(view: SlackView): { label: string; tone: Tone; line: string } {
  switch (view.state) {
    case "connected":
      return { label: "Connected", tone: "ok", line: `As @${view.bot || "bot"}${view.team ? ` in ${view.team}` : ""}` };
    case "connecting":
      return { label: "Connecting…", tone: "muted", line: view.team ? `To ${view.team}` : "Opening Socket Mode" };
    case "error":
      return { label: "Not working", tone: "bad", line: view.error || "The connection failed." };
    default:
      return { label: "Not set up", tone: "muted", line: "Talk to your agents from Slack" };
  }
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
  if (n === 0) return "Nobody yet. Send the bot a direct message, then allow yourself here.";
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
