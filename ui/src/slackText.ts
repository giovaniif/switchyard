// Slack's message markup (mrkdwn) read into a small tree the page renders as
// React elements, never HTML: the text is other people's, untrusted. Bold
// (*x*), italics (_x_), strikes (~x~), inline code, code blocks, quotes (> at
// a line's start), links (<url|label>, only http, https and mailto), mentions
// of people, channels and groups, @here, and emoji shortcodes (a common few
// drawn, the rest kept as :name:).

export type SlackNode =
  | { t: "text"; text: string }
  | { t: "bold" | "italic" | "strike"; children: SlackNode[] }
  | { t: "code"; text: string }
  | { t: "pre"; text: string }
  | { t: "quote"; children: SlackNode[] }
  | { t: "link"; href: string; children: SlackNode[] }
  /** `user`: id, a name to show when the map has none; `channel`, `group`, `special`: label only. */
  | { t: "mention"; kind: "user" | "channel" | "group" | "special"; id: string; label: string }
  | { t: "emoji"; name: string; char: string | null };

const EMOJI: Record<string, string> = {
  "+1": "👍", thumbsup: "👍", "-1": "👎", thumbsdown: "👎", tada: "🎉", heart: "❤️", smile: "😄", smiley: "😃", grinning: "😀",
  joy: "😂", laughing: "😆", wink: "😉", blush: "😊", slightly_smiling_face: "🙂", upside_down_face: "🙃", thinking_face: "🤔",
  eyes: "👀", white_check_mark: "✅", heavy_check_mark: "✔️", x: "❌", warning: "⚠️", pray: "🙏", fire: "🔥", rocket: "🚀",
  raised_hands: "🙌", clap: "👏", ok_hand: "👌", wave: "👋", "100": "💯", sob: "😭", cry: "😢", sweat_smile: "😅",
  muscle: "💪", star: "⭐", sparkles: "✨", bulb: "💡", memo: "📝", point_up: "☝️", point_right: "👉", sunglasses: "😎",
  partying_face: "🥳", heart_eyes: "😍", see_no_evil: "🙈", facepalm: "🤦", shrug: "🤷", coffee: "☕", bug: "🐛", zap: "⚡",
};

/** An emoji shortcode's character, skin tone aside; null when not one of ours. */
export function emojiChar(name: string): string | null {
  return EMOJI[name.replace(/::skin-tone-\d$/, "")] ?? null;
}

const decode = (s: string) => s.replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&amp;/g, "&");

/** Slack markup as nodes. */
export function parseSlack(text: string): SlackNode[] {
  const out: SlackNode[] = [];
  // Code blocks first: nothing inside them is markup.
  const parts = text.split("```");
  parts.forEach((part, i) => {
    if (i % 2 === 1 && i < parts.length - 1) {
      out.push({ t: "pre", text: decode(part.replace(/^\n/, "")) });
    } else {
      const piece = i % 2 === 1 ? "```" + part : part;
      out.push(...blocks(piece));
    }
  });
  return merge(out);
}

/** Lines starting with `>` (or `&gt;`) become quotes. */
function blocks(text: string): SlackNode[] {
  const out: SlackNode[] = [];
  const lines = text.split("\n");
  let quote: string[] = [];
  let plain: string[] = [];
  const flushPlain = () => {
    if (plain.length) out.push(...inline(plain.join("\n")));
    plain = [];
  };
  const flushQuote = () => {
    if (quote.length) out.push({ t: "quote", children: inline(quote.join("\n")) });
    quote = [];
  };
  lines.forEach((line, i) => {
    const m = /^(?:&gt;|>) ?(.*)$/.exec(line);
    if (m) {
      flushPlain();
      quote.push(m[1]);
    } else {
      flushQuote();
      plain.push(i < lines.length - 1 ? line : line);
    }
  });
  flushPlain();
  flushQuote();
  return out;
}

const MARKS: Record<string, "bold" | "italic" | "strike"> = { "*": "bold", _: "italic", "~": "strike" };
const BOUNDARY = /[\s.,;:!?()[\]{}'"/\-]/;

function inline(s: string): SlackNode[] {
  const out: SlackNode[] = [];
  let text = "";
  const flush = () => {
    if (text) out.push({ t: "text", text: decode(text) });
    text = "";
  };
  let i = 0;
  while (i < s.length) {
    const c = s[i];
    if (c === "<") {
      const end = s.indexOf(">", i + 1);
      if (end > i + 1 && !s.slice(i + 1, end).includes("\n")) {
        flush();
        out.push(angle(s.slice(i + 1, end)));
        i = end + 1;
        continue;
      }
    }
    if (c === "`") {
      const end = s.indexOf("`", i + 1);
      if (end > i + 1 && !s.slice(i + 1, end).includes("\n")) {
        flush();
        out.push({ t: "code", text: decode(s.slice(i + 1, end)) });
        i = end + 1;
        continue;
      }
    }
    const mark = MARKS[c];
    if (mark && (i === 0 || BOUNDARY.test(s[i - 1])) && s[i + 1] && !/\s/.test(s[i + 1])) {
      let end = s.indexOf(c, i + 1);
      while (end > 0 && (/\s/.test(s[end - 1]) || (end + 1 < s.length && !BOUNDARY.test(s[end + 1])))) end = s.indexOf(c, end + 1);
      if (end > i + 1 && !s.slice(i + 1, end).includes("\n")) {
        flush();
        out.push({ t: mark, children: inline(s.slice(i + 1, end)) });
        i = end + 1;
        continue;
      }
    }
    if (c === ":") {
      const m = /^:([a-z0-9_+'-]+(?:::skin-tone-\d)?):/.exec(s.slice(i));
      if (m) {
        flush();
        out.push({ t: "emoji", name: m[1], char: emojiChar(m[1]) });
        i += m[0].length;
        continue;
      }
    }
    text += c;
    i++;
  }
  flush();
  return out;
}

/** What is between `<` and `>`: a mention, a special, or a link. */
function angle(inner: string): SlackNode {
  const bar = inner.indexOf("|");
  const target = bar < 0 ? inner : inner.slice(0, bar);
  const label = bar < 0 ? "" : decode(inner.slice(bar + 1));
  if (target.startsWith("@")) return { t: "mention", kind: "user", id: target.slice(1), label };
  if (target.startsWith("#")) return { t: "mention", kind: "channel", id: target.slice(1), label: `#${label || target.slice(1)}` };
  if (target.startsWith("!subteam^")) return { t: "mention", kind: "group", id: target.slice(9), label: label || "@group" };
  if (target.startsWith("!date^")) return { t: "text", text: label || "a date" };
  if (target.startsWith("!")) return { t: "mention", kind: "special", id: target.slice(1), label: `@${target.slice(1)}` };
  const href = decode(target);
  if (/^(https?:\/\/|mailto:)/i.test(href)) return { t: "link", href, children: [{ t: "text", text: label || href.replace(/^mailto:/i, "") }] };
  return { t: "text", text: label || href };
}

/** Neighbouring text nodes as one. */
function merge(nodes: SlackNode[]): SlackNode[] {
  const out: SlackNode[] = [];
  for (const n of nodes) {
    const last = out[out.length - 1];
    if (n.t === "text" && last?.t === "text") last.text += n.text;
    else out.push(n);
  }
  return out;
}

/** The text a message reads as in one line (previews, titles). */
export function plainSlack(text: string, names: Record<string, string> = {}): string {
  const walk = (nodes: SlackNode[]): string =>
    nodes
      .map((n) => {
        switch (n.t) {
          case "text":
          case "code":
          case "pre":
            return n.text;
          case "emoji":
            return n.char ?? `:${n.name}:`;
          case "mention":
            return n.kind === "user" ? `@${names[n.id] || n.label || n.id}` : n.label;
          default:
            return walk(n.children);
        }
      })
      .join("");
  return walk(parseSlack(text)).replace(/\s+/g, " ").trim();
}
