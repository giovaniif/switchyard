// Attachments in the browser: size checks, base64, and matching upload and
// fetch replies to the request that asked. The bytes skip the reducer, so a
// 10 MB upload never sits in React state.
import { MAX_ATTACHMENT_BYTES, MAX_ATTACHMENTS_PER_MESSAGE } from "./protocol";

export function humanSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** Why a file cannot be attached, before any byte is sent. yardd checks again. */
export function tooLarge(size: number): string | null {
  if (size <= MAX_ATTACHMENT_BYTES) return null;
  return `Too large: ${humanSize(size)}. The limit is ${humanSize(MAX_ATTACHMENT_BYTES)}.`;
}

/** Why the files cannot all go on one message. Checked before Send: the
 *  session composer clears as it sends, so a refusal from yardd would lose
 *  the message and the files with it. */
export function tooMany(count: number): string | null {
  if (count <= MAX_ATTACHMENTS_PER_MESSAGE) return null;
  return `Too many files: ${count}. The limit is ${MAX_ATTACHMENTS_PER_MESSAGE} per message.`;
}

/** Image types both engines take inline, which the thread shows as thumbnails. */
export const isInlineImage = (mime: string) => ["image/png", "image/jpeg", "image/gif", "image/webp"].includes(mime);

/** The files a paste should attach; none means a normal text paste. Rich
 *  text (a copy from a document or a web page) can carry a picture of the
 *  selection too, and then the text is what was meant. */
export function pastedFiles(data: { files: ArrayLike<File>; types: readonly string[] } | null): File[] {
  if (!data || !data.files.length) return [];
  if (data.types.includes("text/html")) return [];
  return Array.from(data.files);
}

/** Whether a drag carries files (and not text or a link from the page). */
export const dragHasFiles = (data: { types: readonly string[] } | null) => !!data && Array.from(data.types).includes("Files");

export function fileToBase64(file: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const url = String(reader.result ?? "");
      resolve(url.slice(url.indexOf(",") + 1));
    };
    reader.onerror = () => reject(new Error("Could not read the file."));
    reader.readAsDataURL(file);
  });
}

export function base64ToBlob(data: string, mime: string): Blob {
  const bin = atob(data);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return new Blob([bytes], { type: mime });
}

/** Requests waiting for a reply that carries their `ref`. */
export class Replies {
  private waiting = new Map<string, { resolve(msg: any): void; timer: ReturnType<typeof setTimeout> }>();
  private count = 0;

  ref(): string {
    return `u${Date.now().toString(36)}${(++this.count).toString(36)}`;
  }

  /** Resolves with the reply, or with `{ error }` on timeout or a dropped socket. */
  wait(ref: string, timeoutMs: number): Promise<any> {
    return new Promise((resolve) => {
      const timer = setTimeout(() => this.settle(ref, { error: "yardd did not answer in time." }), timeoutMs);
      this.waiting.set(ref, { resolve, timer });
    });
  }

  /** Hand a server message to the request it answers. False when it answers none. */
  take(msg: any): boolean {
    if (typeof msg?.ref !== "string" || !this.waiting.has(msg.ref)) return false;
    // A refused op (bad target, host down) comes back as a plain `error`.
    this.settle(msg.ref, msg.op === "error" ? { ...msg, error: msg.message } : msg);
    return true;
  }

  failAll(error: string) {
    for (const ref of [...this.waiting.keys()]) this.settle(ref, { error });
  }

  private settle(ref: string, msg: any) {
    const w = this.waiting.get(ref);
    if (!w) return;
    this.waiting.delete(ref);
    clearTimeout(w.timer);
    w.resolve(msg);
  }
}
