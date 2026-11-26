import XCTest
@testable import Switchyard

@MainActor
final class DemoTests: XCTestCase {
    func testTheDemoLoadsAndAnswers() async throws {
        let yard = Yard(host: URL(string: "https://demo.switchyard.local")!, transport: DemoTransport(), demo: true)
        yard.start()
        try await Task.sleep(for: .milliseconds(400))
        XCTAssertTrue(yard.ready)
        XCTAssertEqual(yard.sessions.count, 8)
        XCTAssertEqual(yard.sessions["d2"]?.status, .needsApproval)
        yard.view("d2")
        try await Task.sleep(for: .milliseconds(300))
        guard case .approval(let a)? = yard.items("d2").last else { return XCTFail("\(yard.items("d2"))") }
        yard.approve("d2", requestId: a.requestId, decision: .allow)
        try await Task.sleep(for: .seconds(2.2))
        XCTAssertEqual(yard.sessions["d2"]?.status, .idle)
        XCTAssertEqual(VoiceRules.cue(yard.items("d2"))?.cue.kind, .answer)
    }
}
