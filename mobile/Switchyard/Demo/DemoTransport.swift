import Foundation

/// A pretend yardd for the demo and screenshots: the bundled snapshot and
/// histories, and a scripted echo for anything sent. Times are moved so the
/// sessions look recent.
@MainActor
final class DemoTransport: YardTransport {
    var onMessage: ((JSON) -> Void)?
    var onClosed: ((String?) -> Void)?
    var onOpen: (() -> Void)?

    private var data: JSON = .null
    private var sessions: [String: JSON] = [:]
    private var histories: [String: [JSON]] = [:]
    private var shift: Double = 0
    private var next = 0

    init() {
        if let url = Bundle.main.url(forResource: "demo", withExtension: "json"),
           let raw = try? Data(contentsOf: url), let json = JSON.parse(raw) {
            data = json
        }
        shift = Date().timeIntervalSince1970 * 1000 - (data["now"].double ?? 0)
        for s in data["snapshot"]["sessions"].list {
            if let id = s["id"].string { sessions[id] = rebase(s, keys: ["createdAt", "updatedAt"]) }
        }
        for (id, events) in data["histories"].object ?? [:] {
            histories[id] = events.list.map { rebase($0, keys: ["at"]) }
        }
    }

    private func rebase(_ j: JSON, keys: [String]) -> JSON {
        guard var o = j.object else { return j }
        for k in keys { if let v = o[k]?.double { o[k] = .number(v + shift) } }
        return .object(o)
    }

    private var now: JSON { .number(Date().timeIntervalSince1970 * 1000) }

    func start() {
        onOpen?()
        deliver(after: 0.15) { [self] in
            var snap = data["snapshot"].object ?? [:]
            snap["sessions"] = .array(Array(sessions.values))
            return .object(snap)
        }
    }

    func stop() {}

    func send(_ msg: JSON) {
        let op = msg["op"].text
        let ref = msg["ref"]
        let sid = msg["sessionId"].text
        switch op {
        case "session.open":
            deliver(after: 0.1) { [self] in ["op": "history", "sessionId": .string(sid), "events": .array(histories[sid] ?? [])] }
        case "session.send":
            let text = msg["text"].text
            emit(sid, ["type": "user_message", "text": .string(text)])
            setStatus(sid, "running")
            emit(sid, ["type": "turn_started", "at": now], after: 0.3)
            let answer = "Got it: \(text)\n\nThis is the demo, so nothing ran. Connect your host to talk to real agents."
            emit(sid, ["type": "assistant_delta", "itemId": .string("demo-\(next)"), "text": .string(String(answer.prefix(24)))], after: 1.0)
            emit(sid, ["type": "assistant_message", "itemId": .string("demo-\(next)"), "text": .string(answer)], after: 1.8)
            emit(sid, ["type": "turn_completed", "status": "completed", "at": now], after: 1.9)
            setStatus(sid, "idle", after: 1.9)
            next += 1
        case "session.approve":
            let decision = msg["decision"].text
            emit(sid, ["type": "approval_resolved", "requestId": msg["requestId"], "decision": .string(decision), "at": now], after: 0.3)
            let ok = decision != "deny"
            emit(sid, ["type": "tool_completed", "itemId": "c2", "ok": .bool(ok), "output": ok ? .string("Applied 2 migrations") : .null], after: 0.8)
            emit(sid, ["type": "assistant_message", "itemId": "m9", "text": .string(ok ? "Both migrations ran. Row counts match: **18,204** invoices before and after." : "Okay, I didn't run it.")], after: 1.6)
            emit(sid, ["type": "turn_completed", "status": "completed", "at": now], after: 1.7)
            setStatus(sid, "idle", after: 1.7)
        case "session.interrupt":
            emit(sid, ["type": "turn_completed", "status": "interrupted", "at": now], after: 0.2)
            setStatus(sid, "idle", after: 0.2)
        case "session.create":
            next += 1
            let id = "demo-new-\(next)"
            let text = msg["text"].text
            sessions[id] = [
                "id": .string(id), "title": .string(String(text.prefix(60))), "engine": msg["engine"], "model": msg["model"],
                "effort": msg["effort"], "status": "running", "cwd": msg["cwd"], "host": "local", "permission": msg["permission"],
                "pending": [], "createdAt": now, "updatedAt": now,
            ]
            histories[id] = []
            deliver(after: 0.4) { [self] in ["op": "session", "session": sessions[id] ?? .null] }
            deliver(after: 0.5) { ["op": "created", "sessionId": .string(id), "ref": ref] }
            send(["op": "session.send", "sessionId": .string(id), "text": .string(text)])
        case "session.update":
            guard var s = sessions[sid]?.object else { return }
            for k in ["pinned", "archived"] where !msg[k].isNull { s[k] = msg[k] }
            sessions[sid] = .object(s)
            deliver(after: 0.05) { ["op": "session", "session": .object(s)] }
        case "session.setModel", "session.setPermission":
            guard var s = sessions[sid]?.object else { return }
            for k in ["model", "effort", "permission"] where !msg[k].isNull { s[k] = msg[k] }
            sessions[sid] = .object(s)
            deliver(after: 0.05) { ["op": "session", "session": .object(s)] }
        case "fs.dirs":
            let path = msg["path"].text
            deliver(after: 0.1) { [self] in ["op": "fs.dirs", "path": .string(path), "dirs": data["dirs"][path].isNull ? [] : data["dirs"][path]] }
        case "attach.put":
            next += 1
            deliver(after: 0.6) { [self] in
                ["op": "attach.stored", "ref": ref, "attachment": [
                    "id": .string("a\(next)"), "name": msg["name"], "mime": msg["mime"], "size": 1, "path": "/tmp/demo",
                ]]
            }
        case "session.rename":
            guard var s = sessions[sid]?.object else { return }
            s["title"] = msg["title"]
            sessions[sid] = .object(s)
            deliver(after: 0.05) { ["op": "session.renamed", "ref": ref] }
            deliver(after: 0.06) { ["op": "session", "session": .object(s)] }
        default:
            break
        }
    }

    private func emit(_ sid: String, _ event: JSON, after: Double = 0.05) {
        var list = histories[sid] ?? []
        let seq = list.count
        list.append(event)
        histories[sid] = list
        deliver(after: after) { ["op": "event", "sessionId": .string(sid), "seq": .number(Double(seq)), "event": event] }
    }

    private func setStatus(_ sid: String, _ status: String, after: Double = 0.05) {
        guard var s = sessions[sid]?.object else { return }
        s["status"] = .string(status)
        s["updatedAt"] = now
        if status == "idle" { s["lastTurn"] = "completed" }
        sessions[sid] = .object(s)
        deliver(after: after) { ["op": "session", "session": .object(s)] }
    }

    private func deliver(after: Double, _ make: @escaping () -> JSON) {
        Task { @MainActor [weak self] in
            try? await Task.sleep(for: .seconds(after))
            self?.onMessage?(make())
        }
    }
}
