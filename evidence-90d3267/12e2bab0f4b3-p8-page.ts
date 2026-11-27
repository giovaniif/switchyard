// A raw page on the hub: waits for its snapshot, sends one op to the box
// through its pipe, and reports what comes back in `watchMs`.
// args: hubPort hostName op(json, HOST replaced by the box's host id) watchMs
const [hubPort, hostName, opJson, watchMs] = process.argv.slice(2);
const ws = new WebSocket(`ws://127.0.0.1:${hubPort}/ws`);
const t0 = Date.now();
const seen: any[] = [];
let sent: any = null;
ws.onmessage = (e) => {
  const m = JSON.parse(String(e.data));
  if (m.op === "snapshot" && !sent) {
    const host = (m.hosts ?? []).find((h: any) => h.name === hostName);
    if (!host) throw new Error("no host " + hostName);
    sent = JSON.parse(opJson.replaceAll("HOST", host.id));
    ws.send(JSON.stringify(sent));
    seen.push({ at: Date.now() - t0, sent: sent.op, ref: sent.ref ?? null });
    return;
  }
  const mine = (sent?.ref && m.ref === sent.ref) || m.op === "error" || m.op === "host.down" || m.op === "host.snapshot" || (m.op === "event" && m.event?.type === "approval_resolved") || m.op === "attach.stored" || m.op === "created";
  if (mine) {
    const { attachment, sessions, models, usage, accounts, host, home, workspace, automations, mobile, ...rest } = m;
    seen.push({ at: Date.now() - t0, ...rest });
  }
};
setTimeout(() => {
  const answered = seen.filter((m) => !m.sent && ((sent?.ref && m.ref === sent.ref) || (m.op === "error" && m.for === sent?.op) || (m.op === "event" && m.event?.requestId === sent?.requestId)));
  console.log(JSON.stringify({ op: sent, answers: answered.length, seen }, null, 1));
  process.exit(0);
}, Number(watchMs));
