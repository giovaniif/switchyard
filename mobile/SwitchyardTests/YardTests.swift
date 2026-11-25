import XCTest
@testable import Switchyard

/// Stands in for the socket: records what the store sends, plays what yardd says.
@MainActor
final class FakeTransport: YardTransport {
    var onMessage: ((JSON) -> Void)?
    var onClosed: ((String?) -> Void)?
    var onOpen: (() -> Void)?
    var sent: [JSON] = []
    var starts = 0
    func start() { starts += 1; onOpen?() }
    func stop() {}
    func send(_ msg: JSON) { sent.append(msg) }
    func say(_ msg: JSON) { onMessage?(msg) }
}

@MainActor
final class YardTests: XCTestCase {
    /// Messages recorded from a real yardd with scripted engines (verify/yv iso seed).
    private func capture() throws -> JSON {
        let url = try XCTUnwrap(Bundle(for: YardTests.self).url(forResource: "yardd-capture", withExtension: "json"))
        return try XCTUnwrap(JSON.parse(Data(contentsOf: url)))
    }

    private func connected() throws -> (Yard, FakeTransport, JSON) {
        let t = FakeTransport()
        let yard = Yard(host: URL(string: "https://yard.test")!, transport: t)
        yard.start()
        let cap = try capture()
        t.say(cap["snapshot"])
        return (yard, t, cap)
    }

    private func history(_ cap: JSON, titled prefix: String) throws -> (String, JSON) {
        let session = try XCTUnwrap(cap["snapshot"]["sessions"].list.first { $0["title"].text.hasPrefix(prefix) })
        let id = session["id"].text
        let history = try XCTUnwrap(cap["histories"].list.first { $0["sessionId"].text == id })
        return (id, history)
    }

    func testSnapshotListsSessionsAndModels() throws {
        let (yard, t, cap) = try connected()
        XCTAssertEqual(yard.link, .online)
        XCTAssertEqual(yard.sessions.count, cap["snapshot"]["sessions"].list.count)
        XCTAssertFalse(yard.modelsOn("local")[.codex]?.isEmpty ?? true)
        XCTAssertTrue(yard.engines(on: "local").contains(.claude))
        // A hub with setup state learns that a phone connected; once.
        XCTAssertEqual(t.sent.filter { $0["op"].string == "mobile.connected" }.count, 1)
    }

    func testHistoryBuildsTheThread() throws {
        let (yard, t, cap) = try connected()
        let (id, history) = try self.history(cap, titled: "[cmd]")
        yard.view(id)
        XCTAssertEqual(t.sent.last?["op"].string, "session.open")
        t.say(history)
        let items = yard.items(id)
        guard case .approval(let a)? = items.last else { return XCTFail("no approval: \(items)") }
        XCTAssertNil(a.decision)
        XCTAssertEqual(VoiceRules.cue(items)?.cue.kind, .approval)
        let tool = items.compactMap { if case .tool(let t) = $0 { return t }; return nil }.first
        XCTAssertEqual(tool?.asked, .waiting)

        // Deciding marks it sent; a refusal gives the buttons back.
        yard.approve(id, requestId: a.requestId, decision: .allow)
        guard case .approval(let sending)? = yard.items(id).last else { return XCTFail() }
        XCTAssertTrue(sending.sending)
        t.say(["op": "error", "for": "session.approve", "message": "no such request"])
        guard case .approval(let back)? = yard.items(id).last else { return XCTFail() }
        XCTAssertFalse(back.sending)
        XCTAssertEqual(yard.problem, "no such request")
    }

    func testEveryCapturedHistoryEndsAsItsTurnDid() throws {
        let (yard, t, cap) = try connected()
        for history in cap["histories"].list {
            let id = history["sessionId"].text
            yard.view(id)
            t.say(history)
            let items = yard.items(id)
            XCTAssertFalse(items.isEmpty, id)
            // Nothing still streams once a turn has completed.
            if case .turn(_, _, _, let status)? = items.last(where: { if case .turn = $0 { return true }; return false }), status != "running" {
                for it in items {
                    if case .assistant(_, _, _, let streaming) = it { XCTAssertFalse(streaming, id) }
                    if case .tool(let tool) = it { XCTAssertNotEqual(tool.state, .running, id) }
                }
            }
        }
    }

    func testStreamingDeltasJoinAndSubagentStepsNest() throws {
        let (yard, t, cap) = try connected()
        let (think, thinkHistory) = try history(cap, titled: "[think] TEST")
        yard.view(think)
        t.say(thinkHistory)
        let items = yard.items(think)
        XCTAssertTrue(items.contains { if case .reasoning(_, _, let text, false) = $0 { return !text.isEmpty }; return false })
        XCTAssertEqual(items.filter { if case .assistant = $0 { return true }; return false }.count, 1)

        let (sub, subHistory) = try history(cap, titled: "[subagent]")
        yard.view(sub)
        t.say(subHistory)
        let rows = ThreadRow.build(yard.items(sub))
        let groups = rows.compactMap { if case .tools(_, let nodes) = $0 { return nodes }; return nil }
        XCTAssertEqual(groups.count, 1)
        XCTAssertEqual(groups.first?.count, 1, "the subagent's steps sit under its call")
        XCTAssertEqual(groups.first?.first?.children.count, 2)
        // "Worked for" closes the turn, under its answer.
        guard case .item(.turn(_, _, _, "completed"))? = rows.last else { return XCTFail("\(rows.map(\.id))") }
        guard case .item(.user)? = rows.first else { return XCTFail("\(rows.map(\.id))") }
    }

    func testLiveEventsWaitForHistory() throws {
        let (yard, t, cap) = try connected()
        let (id, history) = try self.history(cap, titled: "TEST-say hello")
        yard.view(id)
        let count = history["events"].list.count
        // An event that arrives before the history is kept, then applied after it.
        t.say(["op": "event", "sessionId": .string(id), "seq": .number(Double(count)), "event": ["type": "user_message", "text": "again"]])
        XCTAssertTrue(yard.items(id).isEmpty)
        t.say(history)
        guard case .user(_, let text, _, _)? = yard.items(id).last else { return XCTFail() }
        XCTAssertEqual(text, "again")
        // A repeat of an event already applied changes nothing.
        let before = yard.items(id).count
        t.say(["op": "event", "sessionId": .string(id), "seq": .number(Double(count)), "event": ["type": "user_message", "text": "again"]])
        XCTAssertEqual(yard.items(id).count, before)
    }

    func testRepliesFindTheirAsker() async throws {
        let (yard, t, _) = try connected()
        async let created = yard.create(.init(engine: .codex, model: "fake-fast", effort: "low", cwd: "/tmp", text: "hi", permission: .ask, worktree: false, host: "local", attachments: []))
        try await Task.sleep(for: .milliseconds(50))
        let ref = try XCTUnwrap(t.sent.last?["ref"].string)
        XCTAssertEqual(t.sent.last?["op"].string, "session.create")
        t.say(["op": "created", "sessionId": "s-new", "ref": .string(ref)])
        let result = await created
        XCTAssertEqual(try result.get(), "s-new")
    }

    func testADroppedSocketReconnectsAndReloadsWhatIsOnScreen() async throws {
        let (yard, t, cap) = try connected()
        let (id, history) = try self.history(cap, titled: "TEST-say hello")
        yard.view(id)
        t.say(history)
        t.onClosed?("gone")
        XCTAssertEqual(yard.link, .offline("gone"))
        XCTAssertFalse(yard.items(id).isEmpty, "the thread stays on screen while away")
        try await Task.sleep(for: .seconds(2))
        XCTAssertEqual(t.starts, 2)
        t.sent.removeAll()
        t.say(cap["snapshot"])
        XCTAssertEqual(t.sent.first { $0["op"].string == "session.open" }?["sessionId"].string, id)
    }

    func testQueueReturnedGoesBackToTheReplyBox() throws {
        let (yard, t, _) = try connected()
        t.say(["op": "queue.returned", "sessionId": "s1", "text": "take this back"])
        XCTAssertEqual(yard.returned["s1"], "take this back")
    }
}
