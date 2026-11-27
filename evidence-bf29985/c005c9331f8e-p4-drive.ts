// A raw page on the hub. stall: wait for its pipe to the box to drop, then
// send a 24 MB upload (its write is reset). removed: send an op while the
// pipe is still opening, then remove the host. Prints what came back.
const [port, scenario, hostId] = process.argv.slice(2);
const ws = new WebSocket(`ws://127.0.0.1:${port}/ws`);
const got: any[] = [];
const seen: string[] = [];
const started = Date.now();
let resolveDown: () => void;
const down = new Promise<void>((r) => (resolveDown = r));
ws.onmessage = (e) => {
  const m = JSON.parse(String(e.data));
  seen.push(`${Date.now() - started}:${m.op}${m.hostId ? ":" + m.hostId : ""}`);
  if (m.op === "host.down" && m.hostId === hostId) resolveDown();
  if (m.ref || m.op === "error") got.push({ atMs: Date.now() - started, op: m.op, ref: m.ref, message: m.message, for: m.for });
};
await new Promise((r) => (ws.onopen = r));
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
let sent: Record<string, number> = {};
if (scenario === "stall") {
  await Promise.race([down, sleep(5000)]);
  sent.big = Date.now() - started;
  ws.send(JSON.stringify({ op: "attach.put", host: hostId, ref: "big", name: "TEST-big.bin", data: "x".repeat(24 << 20) }));
  await sleep(15000);
} else {
  await sleep(500);
  sent.open = Date.now() - started;
  ws.send(JSON.stringify({ op: "session.open", sessionId: `${hostId}~TEST-none`, ref: "open" }));
  await sleep(500);
  sent.rm = Date.now() - started;
  ws.send(JSON.stringify({ op: "host.remove", hostId, ref: "rm" }));
  await sleep(4000);
}
console.log(JSON.stringify({ scenario, hostId, sent, replies: got, seen: seen.filter((s) => !/:(stats|event|sessions|hosts)/.test(s)).slice(0, 40) }));
process.exit(0);
