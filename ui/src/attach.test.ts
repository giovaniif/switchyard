import { describe, expect, test } from "bun:test";
import { base64ToBlob, dragHasFiles, humanSize, pastedFiles, Replies, tooLarge, tooMany } from "./attach";
import { MAX_ATTACHMENT_BYTES, MAX_ATTACHMENTS_PER_MESSAGE } from "./protocol";

describe("attachments in the browser", () => {
  test("sizes read like yardd's", () => {
    expect(humanSize(18)).toBe("18 B");
    expect(humanSize(2048)).toBe("2 KB");
    expect(humanSize(10 * 1024 * 1024)).toBe("10.0 MB");
  });

  test("the cap is checked before upload", () => {
    expect(tooLarge(MAX_ATTACHMENT_BYTES)).toBeNull();
    expect(tooLarge(MAX_ATTACHMENT_BYTES + 1)).toBe("Too large: 10.0 MB. The limit is 10.0 MB.");
    expect(tooLarge(25 * 1024 * 1024)).toBe("Too large: 25.0 MB. The limit is 10.0 MB.");
  });

  test("the per-message file count is checked before Send, like yardd's", () => {
    expect(MAX_ATTACHMENTS_PER_MESSAGE).toBe(20);
    expect(tooMany(20)).toBeNull();
    expect(tooMany(21)).toBe("Too many files: 21. The limit is 20 per message.");
  });

  test("a paste attaches files, unless it is rich text with a picture of itself", () => {
    const shot = new File([new Uint8Array([1])], "image.png", { type: "image/png" });
    expect(pastedFiles({ files: [shot], types: ["Files"] })).toEqual([shot]);
    expect(pastedFiles({ files: [shot], types: ["text/plain", "Files"] })).toEqual([shot]);
    expect(pastedFiles({ files: [shot], types: ["text/plain", "text/html", "Files"] })).toEqual([]);
    expect(pastedFiles({ files: [], types: ["text/plain"] })).toEqual([]);
    expect(pastedFiles(null)).toEqual([]);
  });

  test("only file drags count", () => {
    expect(dragHasFiles({ types: ["Files"] })).toBe(true);
    expect(dragHasFiles({ types: ["text/plain", "text/uri-list"] })).toBe(false);
    expect(dragHasFiles(null)).toBe(false);
  });

  test("base64 turns back into the same bytes", async () => {
    const blob = base64ToBlob("iVBORw==", "image/png");
    expect(blob.type).toBe("image/png");
    expect([...new Uint8Array(await blob.arrayBuffer())]).toEqual([0x89, 0x50, 0x4e, 0x47]);
  });

  test("replies reach the request with their ref, and nothing else", async () => {
    const r = new Replies();
    const a = r.ref();
    const b = r.ref();
    expect(a).not.toBe(b);
    const first = r.wait(a, 1000);
    const second = r.wait(b, 1000);
    expect(r.take({ op: "attach.stored", ref: "other" })).toBe(false);
    expect(r.take({ op: "event" })).toBe(false);
    expect(r.take({ op: "attach.stored", ref: a, attachment: { id: "x" } })).toBe(true);
    // A refused op comes back as `error`; its message becomes the error.
    expect(r.take({ op: "error", ref: b, message: "this host is not connected" })).toBe(true);
    expect((await first).attachment.id).toBe("x");
    expect((await second).error).toBe("this host is not connected");
    // Already settled: a late duplicate is not swallowed.
    expect(r.take({ op: "attach.stored", ref: a })).toBe(false);
  });

  test("a dropped socket or a silent yardd fails what is waiting", async () => {
    const r = new Replies();
    const dropped = r.wait(r.ref(), 1000);
    r.failAll("The connection to yardd dropped.");
    expect((await dropped).error).toBe("The connection to yardd dropped.");
    const slow = r.wait(r.ref(), 5);
    expect((await slow).error).toBe("yardd did not answer in time.");
  });
});
