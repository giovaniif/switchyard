import Foundation

// What a session's thread shows, built from its events. A port of
// `applyEvent` in ui/src/protocol.ts; keep the two in step.

enum ToolState: String, Sendable { case running, ok, failed }
enum Asked: String, Sendable { case waiting, denied, unanswered }

struct ToolItem: Equatable, Sendable {
    var key: String
    var itemId: String
    var toolKind: String
    var title: String
    var detail: String
    var state: ToolState
    var asked: Asked?
    var output: String?
    var agent: String?
    var parent: String?
    var ms: Double?
}

struct ApprovalItem: Equatable, Sendable {
    var key: String
    var requestId: String
    var approvalKind: String
    var title: String
    var detail: String
    var decision: Decision?
    var itemId: String?
    var unanswered = false
    var sending = false
    var askedAt: Double?
    var answeredAt: Double?

    var asked: Asked? {
        guard let decision else { return unanswered ? .unanswered : .waiting }
        return decision == .deny ? .denied : nil
    }
}

enum Item: Equatable, Identifiable, Sendable {
    case user(key: String, text: String, attachments: [Attachment], steered: Bool)
    case assistant(key: String, itemId: String, text: String, streaming: Bool)
    case reasoning(key: String, itemId: String, text: String, streaming: Bool)
    case turn(key: String, start: Double?, end: Double?, status: String)
    case tool(ToolItem)
    case diff(key: String, files: [ChangedFile])
    case approval(ApprovalItem)
    case error(key: String, message: String)
    case note(key: String, text: String)
    case queued(key: String, reason: String)

    var id: String {
        switch self {
        case .user(let key, _, _, _), .assistant(let key, _, _, _), .reasoning(let key, _, _, _),
             .turn(let key, _, _, _), .diff(let key, _), .error(let key, _), .note(let key, _), .queued(let key, _):
            return key
        case .tool(let t): return t.key
        case .approval(let a): return a.key
        }
    }

    var startsTurn: Bool { if case .user(_, _, _, let steered) = self { return !steered }; return false }
}

enum ThreadReducer {
    static func turnStart(_ items: [Item]) -> Int {
        for i in stride(from: items.count - 1, through: 0, by: -1) where items[i].startsTurn { return i + 1 }
        return 0
    }

    /// Where an item with this engine id sits in the current turn, or nil.
    /// Engines may reuse ids across turns, so only the current turn counts.
    static func inTurn(_ items: [Item], _ match: (Item) -> Bool) -> Int? {
        for i in stride(from: items.count - 1, through: 0, by: -1) {
            if items[i].startsTurn { return nil }
            if match(items[i]) { return i }
        }
        return nil
    }

    static func markAsked(_ items: inout [Item], from: Int, _ approval: ApprovalItem) {
        guard let itemId = approval.itemId else { return }
        for i in items.indices where i >= from {
            if case .tool(var t) = items[i], t.itemId == itemId {
                t.asked = approval.asked
                items[i] = .tool(t)
            }
        }
    }

    static func apply(_ items: [Item], _ ev: JSON, seq: Int) -> [Item] {
        let key = "e\(seq)"
        var items = items
        switch ev["type"].text {
        case "user_message":
            items.append(.user(key: key, text: ev["text"].text, attachments: ev["attachments"].list.compactMap(Attachment.init), steered: ev["steered"].bool ?? false))
        case "reasoning_delta", "reasoning":
            let streaming = ev["type"].text == "reasoning_delta"
            let itemId = ev["itemId"].text, text = ev["text"].text
            if let i = inTurn(items, { if case .reasoning(_, let id, _, _) = $0 { return id == itemId }; return false }),
               case .reasoning(let k, let id, let cur, _) = items[i] {
                items[i] = .reasoning(key: k, itemId: id, text: streaming ? cur + text : (text.isEmpty ? cur : text), streaming: streaming)
            } else if !text.isEmpty {
                items.append(.reasoning(key: key, itemId: itemId, text: text, streaming: streaming))
            }
        case "assistant_delta", "assistant_message":
            let delta = ev["type"].text == "assistant_delta"
            let itemId = ev["itemId"].text, text = ev["text"].text
            if let i = inTurn(items, { if case .assistant(_, let id, _, _) = $0 { return id == itemId }; return false }),
               case .assistant(let k, let id, let cur, let streaming) = items[i] {
                items[i] = delta ? .assistant(key: k, itemId: id, text: cur + text, streaming: streaming) : .assistant(key: k, itemId: id, text: text, streaming: false)
            } else {
                items.append(.assistant(key: key, itemId: itemId, text: text, streaming: delta))
            }
        case "tool_started":
            var tool = ToolItem(key: key, itemId: ev["itemId"].text, toolKind: ev["kind"].text, title: ev["title"].text, detail: ev["detail"].text, state: .running)
            tool.agent = ev["agent"].string
            tool.parent = ev["parent"].string
            // A provider can resume the same subagent in a later turn.
            if tool.agent != nil, let i = items.firstIndex(where: { if case .tool(let t) = $0 { return t.agent != nil && t.itemId == tool.itemId }; return false }),
               case .tool(let old) = items[i] {
                tool.key = old.key
                if tool.detail.isEmpty { tool.detail = old.detail }
                items[i] = .tool(tool)
                return items
            }
            // Some engines ask before they announce the call.
            let from = turnStart(items)
            for i in stride(from: items.count - 1, through: from, by: -1) {
                if case .approval(let a) = items[i], a.itemId == tool.itemId {
                    tool.asked = a.asked
                    break
                }
            }
            items.append(.tool(tool))
        case "tool_completed":
            let from = turnStart(items)
            let itemId = ev["itemId"].text
            for i in items.indices {
                guard case .tool(var t) = items[i], t.itemId == itemId, i >= from || t.agent != nil else { continue }
                t.state = ev["ok"].bool == true ? .ok : .failed
                t.output = ev["output"].string
                if let ms = ev["ms"].double { t.ms = ms }
                items[i] = .tool(t)
            }
        case "file_change":
            let files = ev["files"].list.compactMap(ChangedFile.init)
            if !files.isEmpty { items.append(.diff(key: key, files: files)) }
        case "approval_request":
            var a = ApprovalItem(key: key, requestId: ev["requestId"].text, approvalKind: ev["kind"].text, title: ev["title"].text, detail: ev["detail"].text)
            a.itemId = ev["itemId"].string
            a.askedAt = ev["at"].double
            let from = turnStart(items)
            items.append(.approval(a))
            markAsked(&items, from: from, a)
        case "approval_resolved":
            let from = turnStart(items)
            var resolved: ApprovalItem?
            for i in items.indices where i >= from {
                if case .approval(var a) = items[i], a.requestId == ev["requestId"].text {
                    a.decision = Decision(rawValue: ev["decision"].text) ?? .deny
                    a.sending = false
                    a.answeredAt = ev["at"].double
                    items[i] = .approval(a)
                    resolved = a
                }
            }
            if let resolved { markAsked(&items, from: from, resolved) }
        case "turn_started":
            items.append(.turn(key: key, start: ev["at"].double, end: nil, status: "running"))
        case "turn_completed":
            let status = ev["status"].text
            let open = items.lastIndex(where: { if case .turn = $0 { return true }; return false })
            // A background subagent goes on after a completed turn.
            var goesOn = Set<String>()
            if status == "completed" {
                for it in items { if case .tool(let t) = it, t.agent != nil, t.state == .running { goesOn.insert(t.itemId) } }
            }
            for i in items.indices {
                switch items[i] {
                case .turn(let k, let start, _, _) where i == open:
                    items[i] = .turn(key: k, start: start, end: ev["at"].double, status: status)
                case .assistant(let k, let id, let text, true):
                    items[i] = .assistant(key: k, itemId: id, text: text, streaming: false)
                case .reasoning(let k, let id, let text, true):
                    items[i] = .reasoning(key: k, itemId: id, text: text, streaming: false)
                case .tool(var t) where t.state == .running && !goesOn.contains(t.itemId) && !(t.parent.map { goesOn.contains($0) } ?? false):
                    t.state = .failed
                    if t.asked == .waiting { t.asked = .unanswered }
                    items[i] = .tool(t)
                case .approval(var a) where a.decision == nil:
                    a.unanswered = true
                    items[i] = .approval(a)
                default: break
                }
            }
        case "error":
            // Codex reports a failed turn twice: as an error and as the turn's end.
            let message = ev["message"].text
            if case .error(_, let last)? = items.last, last == message { return items }
            items.append(.error(key: key, message: message))
        case "note":
            items.append(.note(key: key, text: ev["text"].text))
        case "queued":
            items.append(.queued(key: key, reason: ev["reason"].text))
        default:
            break
        }
        return items
    }

    /// Marks approvals as sent until yardd resolves them, so a second tap sends nothing.
    static func deciding(_ items: [Item], requestId: String) -> [Item] {
        items.map {
            if case .approval(var a) = $0, a.requestId == requestId { a.sending = true; return .approval(a) }
            return $0
        }
    }

    /// yardd refused a decision: every waiting card gets its buttons back.
    static func releaseApprovals(_ items: [Item]) -> [Item] {
        items.map {
            if case .approval(var a) = $0, a.sending { a.sending = false; return .approval(a) }
            return $0
        }
    }
}

/// One session's thread: `next` is the seq of the next event expected; until
/// the history arrives (`next == nil`), live events wait in `early`.
struct ThreadState: Equatable, Sendable {
    var next: Int?
    var items: [Item] = []
    var early: [(seq: Int, event: JSON)] = []

    static func == (a: ThreadState, b: ThreadState) -> Bool { a.next == b.next && a.items == b.items && a.early.count == b.early.count }

    mutating func history(_ events: [JSON]) {
        var built: [Item] = []
        for (seq, ev) in events.enumerated() { built = ThreadReducer.apply(built, ev, seq: seq) }
        var n = events.count
        for e in early.sorted(by: { $0.seq < $1.seq }) where e.seq == n {
            built = ThreadReducer.apply(built, e.event, seq: e.seq)
            n += 1
        }
        items = built
        next = n
        early = []
    }

    mutating func event(_ ev: JSON, seq: Int) {
        guard let n = next else { early.append((seq, ev)); return }
        guard seq == n else { return } // already applied through history
        items = ThreadReducer.apply(items, ev, seq: seq)
        next = n + 1
    }
}
