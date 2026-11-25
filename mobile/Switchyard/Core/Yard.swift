import Foundation
import Observation

/// One host's live state, fed by its WebSocket: what ui/src/store.ts keeps
/// for the web page, for the parts the phone shows.
@MainActor
@Observable
final class Yard {
    enum Link: Equatable {
        case connecting
        case online
        case offline(String)
    }

    let host: URL
    let demo: Bool
    private(set) var link: Link = .connecting
    /// The first snapshot arrived; until then the list is not known.
    private(set) var ready = false

    private(set) var sessions: [String: SessionMeta] = [:]
    private(set) var threads: [String: ThreadState] = [:]
    private(set) var models: [String: Models] = [:]
    private(set) var hosts: [HostEntry] = []
    private(set) var agents: [AgentInfo] = []
    private(set) var hostName = ""
    private(set) var stats: HostStats?
    private(set) var workspace: [String: String] = [:]
    private(set) var home: [String: String] = [:]
    private(set) var dirs: [String: DirListing] = [:]
    /// Text Stop or Edit took back from the queue, for that session's reply box.
    var returned: [String: String] = [:]
    /// The last refusal or failure from yardd, shown as a banner.
    var problem: String?
    /// Reply-box drafts by session id ("" is the new-session page).
    var drafts: [String: String] = [:]

    @ObservationIgnored private let transport: YardTransport
    @ObservationIgnored private var opened = Set<String>()
    @ObservationIgnored private var viewing: [String: Int] = [:]
    @ObservationIgnored private var waiting: [String: CheckedContinuation<JSON, Never>] = [:]
    @ObservationIgnored private var nextRef = 0
    @ObservationIgnored private var retry: Task<Void, Never>?
    @ObservationIgnored private var failures = 0
    @ObservationIgnored private var started = false
    /// Sessions' last status, to notice a turn ending in another session.
    @ObservationIgnored private var lastStatus: [String: SessionStatus] = [:]
    @ObservationIgnored var onConnected: (() -> Void)?
    /// A session finished or needs approval while the user looked elsewhere.
    @ObservationIgnored var onAttention: ((SessionMeta) -> Void)?

    init(host: URL, transport: YardTransport, demo: Bool = false) {
        self.host = host
        self.transport = transport
        self.demo = demo
        transport.onMessage = { [weak self] in self?.receive($0) }
        transport.onOpen = { [weak self] in self?.failures = 0 }
        transport.onClosed = { [weak self] in self?.closed($0) }
    }

    func start() {
        guard !started else { return }
        started = true
        link = .connecting
        transport.start()
    }

    func stop() {
        started = false
        retry?.cancel()
        transport.stop()
        failAll()
    }

    /// iOS may suspend a socket without telling it; coming back reconnects.
    func resume() {
        guard started else { return }
        retry?.cancel()
        failures = 0
        link = .connecting
        transport.start()
    }

    private func closed(_ reason: String?) {
        failAll()
        link = .offline(reason ?? "Disconnected.")
        opened.removeAll()
        guard started else { return }
        failures += 1
        let delay = min(10.0, 1.5 * Double(failures))
        retry?.cancel()
        retry = Task { [weak self] in
            try? await Task.sleep(for: .seconds(delay))
            guard let self, !Task.isCancelled, self.started else { return }
            self.link = .connecting
            self.transport.start()
        }
    }

    // MARK: Sending

    func send(_ msg: JSON) { transport.send(msg) }

    /// Sends an op with a fresh `ref` and waits for the reply carrying it.
    func ask(_ msg: JSON, timeout: Double = 30) async -> JSON {
        guard case .online = link else { return ["op": "error", "message": "Not connected to the host."] }
        nextRef += 1
        let ref = "m\(nextRef)"
        var body = msg.object ?? [:]
        body["ref"] = .string(ref)
        transport.send(.object(body))
        let timer = Task { [weak self] in
            try? await Task.sleep(for: .seconds(timeout))
            self?.reply(ref, ["op": "error", "message": "The host did not answer in time."])
        }
        let answer = await withCheckedContinuation { waiting[ref] = $0 }
        timer.cancel()
        return answer
    }

    private func reply(_ ref: String, _ msg: JSON) {
        waiting.removeValue(forKey: ref)?.resume(returning: msg)
    }

    private func failAll() {
        let all = waiting
        waiting.removeAll()
        for (_, c) in all { c.resume(returning: ["op": "error", "message": "The connection to the host dropped. Try again."]) }
    }

    // MARK: Receiving

    private func receive(_ msg: JSON) {
        if let ref = msg["ref"].string, waiting[ref] != nil {
            reply(ref, msg)
            // A created session is also routed; everything else stops here.
            if msg["op"].string != "created" { return }
        }
        switch msg["op"].text {
        case "snapshot":
            link = .online
            ready = true
            let remote = Set(msg["hosts"].list.compactMap { $0["id"].string })
            var next = sessions.filter { $0.value.host != "local" && remote.contains($0.value.host) }
            for s in msg["sessions"].list.compactMap(SessionMeta.init) { next[s.id] = s }
            sessions = next
            hosts = msg["hosts"].list.compactMap(HostEntry.init)
            agents = msg["agents"].list.compactMap(AgentInfo.init)
            models["local"] = parseModels(msg["models"])
            hostName = msg["hostName"].text
            stats = HostStats(msg["host"]) ?? stats
            home["local"] = msg["home"].text
            workspace["local"] = msg["workspace"].string.flatMap { $0.isEmpty ? nil : $0 } ?? msg["home"].text
            lastStatus = sessions.mapValues(\.status)
            // Events may have been missed while away: reload what is on screen.
            for id in threads.keys { threads[id]?.next = nil }
            opened.removeAll()
            for id in viewing.keys { open(id) }
            // An older host has no setup state to record.
            if msg["mobile"].object != nil, msg["mobile"]["configured"].bool != true { send(["op": "mobile.connected"]) }
            onConnected?()
        case "host.snapshot":
            guard let hostId = msg["hostId"].string else { return }
            var next = sessions.filter { $0.value.host != hostId }
            for var s in msg["sessions"].list.compactMap(SessionMeta.init) {
                if s.host == "local" { s.host = hostId }
                next[s.id] = s
            }
            sessions = next
            models[hostId] = parseModels(msg["models"])
            home[hostId] = msg["home"].text
            workspace[hostId] = msg["workspace"].string.flatMap { $0.isEmpty ? nil : $0 } ?? msg["home"].text
            for (id, s) in sessions where s.host == hostId {
                if threads[id] != nil { threads[id]?.next = nil; opened.remove(id) }
                if viewing[id] != nil { open(id) }
            }
        case "hosts":
            hosts = msg["hosts"].list.compactMap(HostEntry.init)
            let known = Set(["local"] + hosts.map(\.id))
            sessions = sessions.filter { known.contains($0.value.host) }
        case "agents":
            if msg["hostId"].isNull { agents = msg["agents"].list.compactMap(AgentInfo.init) }
        case "session":
            guard var s = SessionMeta(msg["session"]) else { return }
            if let h = msg["hostId"].string, s.host == "local" { s.host = h }
            let before = lastStatus[s.id]
            sessions[s.id] = s
            lastStatus[s.id] = s.status
            if let before, before != s.status, before == .running || before == .queued,
               s.status == .idle || s.status == .needsApproval || s.status == .error {
                onAttention?(s)
            }
        case "session.deleted":
            guard let id = msg["sessionId"].string else { return }
            sessions.removeValue(forKey: id)
            threads.removeValue(forKey: id)
        case "models":
            models[msg["hostId"].string ?? "local"] = parseModels(msg["models"])
        case "host":
            if msg["hostId"].isNull { stats = HostStats(msg["host"]) ?? stats }
        case "history":
            guard let id = msg["sessionId"].string else { return }
            var t = threads[id] ?? ThreadState()
            t.history(msg["events"].list)
            threads[id] = t
        case "event":
            guard let id = msg["sessionId"].string, threads[id] != nil, let seq = msg["seq"].int else { return }
            threads[id]?.event(msg["event"], seq: seq)
        case "queue.returned":
            guard let id = msg["sessionId"].string else { return }
            let text = msg["text"].text
            if !text.isEmpty { returned[id] = [returned[id], text].compactMap { $0 }.joined(separator: "\n") }
        case "fs.dirs":
            let listing = DirListing(msg)
            dirs[DirListing.key(listing.host, listing.path)] = listing
        case "error":
            let what = msg["for"].text
            if what == "session.approve" {
                for (id, t) in threads { threads[id]?.items = ThreadReducer.releaseApprovals(t.items) }
            }
            // Status queries an older host does not know are not worth a banner.
            if ["engines.status", "session.procs", "mobile.connected"].contains(what) { return }
            problem = msg["message"].string ?? "The host refused that."
        default:
            break
        }
    }

    // MARK: Sessions

    /// A screen shows this session: load its history once per connection.
    func view(_ id: String) {
        viewing[id, default: 0] += 1
        open(id)
    }

    func unview(_ id: String) {
        viewing[id, default: 1] -= 1
        if viewing[id] == 0 { viewing.removeValue(forKey: id) }
    }

    private func open(_ id: String) {
        guard case .online = link, !opened.contains(id), sessions[id] != nil else { return }
        opened.insert(id)
        if threads[id] == nil { threads[id] = ThreadState() }
        send(["op": "session.open", "sessionId": .string(id)])
    }

    func items(_ id: String) -> [Item] { threads[id]?.items ?? [] }
    func loaded(_ id: String) -> Bool { threads[id]?.next != nil || !(threads[id]?.items.isEmpty ?? true) }

    func sendMessage(_ id: String, text: String, attachments: [Attachment] = []) {
        var msg: [String: JSON] = ["op": "session.send", "sessionId": .string(id), "text": .string(text)]
        if !attachments.isEmpty { msg["attachments"] = JSON(attachments.map(\.id)) }
        send(.object(msg))
    }

    func interrupt(_ id: String) { send(["op": "session.interrupt", "sessionId": .string(id)]) }

    func approve(_ id: String, requestId: String, decision: Decision) {
        if let items = threads[id]?.items { threads[id]?.items = ThreadReducer.deciding(items, requestId: requestId) }
        send(["op": "session.approve", "sessionId": .string(id), "requestId": .string(requestId), "decision": .string(decision.rawValue)])
    }

    func setModel(_ id: String, model: String, effort: String) {
        send(["op": "session.setModel", "sessionId": .string(id), "model": .string(model), "effort": .string(effort)])
    }

    func setPermission(_ id: String, _ permission: Permission) {
        send(["op": "session.setPermission", "sessionId": .string(id), "permission": .string(permission.rawValue)])
    }

    func setPinned(_ id: String, _ pinned: Bool) { send(["op": "session.update", "sessionId": .string(id), "pinned": .bool(pinned)]) }
    func setArchived(_ id: String, _ archived: Bool) { send(["op": "session.update", "sessionId": .string(id), "archived": .bool(archived)]) }

    func unqueue(_ id: String, messageId: String, drop: Bool) {
        var msg: [String: JSON] = ["op": "session.unqueue", "sessionId": .string(id), "id": .string(messageId)]
        if drop { msg["drop"] = true }
        send(.object(msg))
    }

    func rename(_ id: String, title: String) async -> String? {
        let reply = await ask(["op": "session.rename", "sessionId": .string(id), "title": .string(title)], timeout: 10)
        return reply["op"].string == "session.renamed" ? nil : (reply["message"].string ?? reply["error"].string ?? "Couldn't rename the session.")
    }

    struct NewSession {
        var engine: Engine
        var model: String
        var effort: String
        var cwd: String
        var text: String
        var permission: Permission
        var worktree: Bool
        var host: String
        var attachments: [Attachment]
    }

    /// Starts a session; resolves with its id, or the refusal to show.
    func create(_ s: NewSession) async -> Result<String, YardError> {
        var msg: [String: JSON] = [
            "op": "session.create", "engine": .string(s.engine.rawValue), "model": .string(s.model), "effort": .string(s.effort),
            "cwd": .string(s.cwd), "text": .string(s.text), "permission": .string(s.permission.rawValue), "worktree": .bool(s.worktree),
        ]
        if s.host != "local" { msg["host"] = .string(s.host) }
        if !s.attachments.isEmpty { msg["attachments"] = JSON(s.attachments.map(\.id)) }
        // A worktree can run its setup script first.
        let reply = await ask(.object(msg), timeout: 300)
        if reply["op"].string == "created", let id = reply["sessionId"].string { return .success(id) }
        return .failure(YardError(reply["message"].string ?? reply["error"].string ?? "The session did not start."))
    }

    /// Stores a file on the host that will run the prompt.
    func upload(name: String, mime: String, data: Data, sessionId: String?, host: String) async -> Result<Attachment, YardError> {
        if data.count > 10 * 1024 * 1024 { return .failure(YardError("\(name) is over the 10 MB limit.")) }
        var msg: [String: JSON] = ["op": "attach.put", "name": .string(name), "mime": .string(mime), "data": .string(data.base64EncodedString())]
        if let sessionId { msg["sessionId"] = .string(sessionId) } else if host != "local" { msg["host"] = .string(host) }
        let reply = await ask(.object(msg), timeout: 300)
        if let a = Attachment(reply["attachment"]) { return .success(a) }
        return .failure(YardError(reply["error"].string ?? reply["message"].string ?? "The upload failed."))
    }

    func listDirs(_ path: String, host: String) {
        var msg: [String: JSON] = ["op": "fs.dirs", "path": .string(path)]
        if host != "local" { msg["host"] = .string(host) }
        send(.object(msg))
    }

    // MARK: Reading

    func modelsOn(_ host: String) -> Models { models[host] ?? [:] }

    /// Engines a picker offers on a host: Claude and Codex always, others once installed.
    func engines(on host: String) -> [Engine] {
        let m = modelsOn(host)
        return Engine.allCases.filter { Engine.always.contains($0) || !(m[$0]?.isEmpty ?? true) }
    }

    func hostLabel(_ id: String) -> String {
        if id == "local" { return hostName.isEmpty ? (host.host ?? "Host") : hostName }
        return hosts.first(where: { $0.id == id })?.name ?? id
    }

    /// Hosts a new session can start on.
    var launchHosts: [String] { ["local"] + hosts.filter(\.connected).map(\.id) }

    /// Folders sessions ran in, most recent first: the new-session page's shortcuts.
    func recentFolders(host: String) -> [String] {
        var seen = Set<String>()
        return sessions.values.filter { $0.host == host }.sorted { $0.updatedAt > $1.updatedAt }
            .map { $0.repo ?? $0.cwd }.filter { !$0.isEmpty && seen.insert($0).inserted }
    }
}

struct YardError: Error, Equatable {
    let message: String
    init(_ message: String) { self.message = message }
}

struct DirListing: Equatable {
    var host: String
    var path: String
    var dirs: [DirEntry]
    var error: String?

    init(_ j: JSON) {
        host = j["hostId"].string ?? "local"
        path = j["path"].text
        dirs = j["dirs"].list.compactMap { d in d["name"].string.map { DirEntry(name: $0, git: d["git"].bool ?? false) } }
        error = j["error"].string
    }

    static func key(_ host: String, _ path: String) -> String { "\(host)\n\(path)" }
}
