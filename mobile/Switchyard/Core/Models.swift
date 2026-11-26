import Foundation

// Shapes from ui/src/protocol.ts, read leniently from JSON.

enum Engine: String, CaseIterable, Identifiable, Sendable {
    case claude, codex, opencode, grok, gemini
    var id: String { rawValue }

    var name: String {
        switch self {
        case .claude: "Claude"
        case .codex: "Codex"
        case .opencode: "OpenCode"
        case .grok: "Grok"
        case .gemini: "Gemini"
        }
    }

    /// Claude and Codex are always offered; others once their host reports models.
    static let always: [Engine] = [.claude, .codex]

    /// Whether a message sent mid-turn joins the running turn (yardd's `engines::can_steer`).
    var canSteer: Bool { self == .claude || self == .codex }
}

enum SessionStatus: String, Sendable {
    case idle, queued, running, needsApproval = "needs_approval", error

    var label: String {
        switch self {
        case .idle: "Idle"
        case .queued: "Queued"
        case .running: "Working"
        case .needsApproval: "Needs approval"
        case .error: "Error"
        }
    }
}

enum Permission: String, CaseIterable, Identifiable, Sendable {
    case ask, acceptEdits, plan, full
    var id: String { rawValue }
    var label: String {
        switch self {
        case .ask: "Ask first"
        case .acceptEdits: "Accept edits"
        case .plan: "Plan only"
        case .full: "Full access"
        }
    }
    var symbol: String {
        switch self {
        case .ask: "hand.raised"
        case .acceptEdits: "pencil.and.outline"
        case .plan: "list.bullet.clipboard"
        case .full: "bolt.shield"
        }
    }
}

enum Decision: String, Sendable { case allow, allowAlways = "allow_always", deny }

struct Attachment: Identifiable, Equatable, Sendable {
    var id: String
    var name: String
    var mime: String
    var size: Int
    var path: String

    init?(_ j: JSON) {
        guard let id = j["id"].string else { return nil }
        self.id = id
        name = j["name"].text
        mime = j["mime"].text
        size = j["size"].int ?? 0
        path = j["path"].text
    }

    var isImage: Bool { mime.hasPrefix("image/") }
}

struct PendingMessage: Identifiable, Equatable, Sendable {
    var id: String
    var text: String
    var attachments: [Attachment]
    init?(_ j: JSON) {
        guard let id = j["id"].string else { return nil }
        self.id = id
        text = j["text"].text
        attachments = j["attachments"].list.compactMap(Attachment.init)
    }
}

struct SessionMeta: Identifiable, Equatable, Sendable {
    var id: String
    var title: String
    var engine: Engine
    var engineRaw: String
    var model: String
    var effort: String
    var cwd: String
    var status: SessionStatus
    var host: String
    var branch: String?
    var repo: String?
    var permission: Permission
    var account: String
    var pinned: Bool
    var archived: Bool
    var group: String?
    var automation: String?
    var agent: String?
    var lastTurn: String?
    var pending: [PendingMessage]
    var createdAt: Double
    var updatedAt: Double

    init?(_ j: JSON) {
        guard let id = j["id"].string else { return nil }
        self.id = id
        title = j["title"].text
        engineRaw = j["engine"].text
        engine = Engine(rawValue: engineRaw) ?? .claude
        model = j["model"].text
        effort = j["effort"].text
        cwd = j["cwd"].text
        status = SessionStatus(rawValue: j["status"].text) ?? .idle
        host = j["host"].string ?? "local"
        branch = j["branch"].string
        repo = j["repo"].string
        permission = Permission(rawValue: j["permission"].text) ?? .ask
        account = j["account"].string ?? "default"
        pinned = j["pinned"].bool ?? false
        archived = j["archived"].bool ?? false
        group = j["group"].string
        automation = j["automation"].string
        agent = j["agent"].string
        lastTurn = j["lastTurn"].string
        pending = j["pending"].list.compactMap(PendingMessage.init)
        createdAt = j["createdAt"].double ?? 0
        updatedAt = j["updatedAt"].double ?? 0
    }

    var isRemote: Bool { host != "local" && !host.isEmpty }
    var folderName: String { (cwd as NSString).lastPathComponent }
    var updated: Date { Date(timeIntervalSince1970: updatedAt / 1000) }
    var isBusy: Bool { status == .running || status == .queued || status == .needsApproval }
    var displayTitle: String { title.isEmpty ? "New session" : title }
}

struct ModelInfo: Identifiable, Equatable, Sendable {
    var id: String
    var name: String
    var efforts: [String]
    var defaultEffort: String?
    var isDefault: Bool

    init?(_ j: JSON) {
        guard let id = j["id"].string else { return nil }
        self.id = id
        name = j["name"].string ?? id
        efforts = j["efforts"].list.compactMap(\.string)
        defaultEffort = j["defaultEffort"].string
        isDefault = j["isDefault"].bool ?? false
    }

    var effortOrDefault: String { defaultEffort ?? efforts.first(where: { $0 == "medium" }) ?? efforts.first ?? "" }
}

typealias Models = [Engine: [ModelInfo]]

func parseModels(_ j: JSON) -> Models {
    var out: Models = [:]
    for engine in Engine.allCases {
        let list = j[engine.rawValue].list.compactMap(ModelInfo.init)
        if !list.isEmpty { out[engine] = list }
    }
    return out
}

struct HostEntry: Identifiable, Equatable, Sendable {
    var id: String
    var name: String
    var status: String
    init?(_ j: JSON) {
        guard let id = j["id"].string else { return nil }
        self.id = id
        name = j["name"].string ?? id
        status = j["status"].text
    }
    var connected: Bool { status == "connected" }
}

struct HostStats: Equatable, Sendable {
    var name: String
    var cpu: Double
    var memUsed: Double
    var memTotal: Double
    var diskUsed: Double
    var diskTotal: Double
    var pressure: String?
    init?(_ j: JSON) {
        guard j.object != nil else { return nil }
        name = j["name"].text
        cpu = j["cpu"].double ?? 0
        memUsed = j["memUsed"].double ?? 0
        memTotal = j["memTotal"].double ?? 0
        diskUsed = j["diskUsed"].double ?? 0
        diskTotal = j["diskTotal"].double ?? 0
        pressure = j["pressure"].string
    }
}

struct AgentInfo: Identifiable, Equatable, Sendable {
    var id: String
    var name: String
    var color: String
    var about: String
    var engine: Engine
    var builtin: Bool
    var chat: String?
    init?(_ j: JSON) {
        guard let id = j["id"].string else { return nil }
        self.id = id
        name = j["name"].text
        color = j["color"].string ?? "blue"
        about = j["about"].text
        engine = Engine(rawValue: j["engine"].text) ?? .claude
        builtin = j["builtin"].bool ?? false
        chat = j["chat"].string
    }
}

struct DirEntry: Identifiable, Equatable, Sendable {
    var name: String
    var git: Bool
    var id: String { name }
}

struct ChangedFile: Identifiable, Equatable, Sendable {
    var path: String
    var added: Int?
    var removed: Int?
    var kind: String?
    var id: String { path }
    init?(_ j: JSON) {
        guard let path = j["path"].string else { return nil }
        self.path = path
        added = j["added"].int
        removed = j["removed"].int
        kind = j["kind"].string
    }
}
