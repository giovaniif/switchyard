import XCTest
@testable import Switchyard

/// The same cases as ui/src/voice.test.ts, so the phone and the web speak alike.
final class VoiceRulesTests: XCTestCase {
    func testSpeakable() {
        XCTAssertEqual(VoiceRules.speakable("All tests pass. The build is green."), "All tests pass. The build is green.")
        XCTAssertEqual(VoiceRules.speakable("Here is the fix:\n\n```ts\nconst a = 1;\nconst b = 2;\n```\n\nThat's it."), "Here is the fix: Code block omitted. That's it.")
        XCTAssertEqual(VoiceRules.speakable("Start:\n```sh\nrm -rf build"), "Start: Code block omitted.")
        XCTAssertEqual(VoiceRules.speakable("Run `bun test` in `ui`."), "Run bun test in ui.")
        XCTAssertEqual(VoiceRules.speakable("See [the PR](https://github.com/x/y/pull/1) and https://example.com/a?b=c ![shot](a.png) now."), "See the PR and a link now.")
        XCTAssertEqual(VoiceRules.speakable("## Summary\n\n- **Fixed** the _flaky_ test\n- Removed ~~dead~~ code\n1. First\n> quoted line"),
                       "Summary. Fixed the flaky test. Removed dead code. First. quoted line")
        XCTAssertEqual(VoiceRules.speakable("Results:\n\n| Suite | Pass |\n|---|---|\n| ui | 120 |\n| yardd | 80 |\n\nAll good."), "Results: Table omitted. All good.")
        XCTAssertEqual(VoiceRules.speakable("Press <kbd>Esc</kbd> to stop."), "Press Esc to stop.")
        XCTAssertEqual(VoiceRules.speakable("```\ncode\n```").trimmingCharacters(in: .whitespaces), "Code block omitted.")
        XCTAssertEqual(VoiceRules.speakable("   \n\n "), "")
    }

    func testLongAnswerIsCutAtASentence() {
        let md = (0..<200).map { "Sentence number \($0) is here." }.joined(separator: " ")
        let out = VoiceRules.speakable(md, max: 300)
        XCTAssertLessThanOrEqual(out.count, 340)
        XCTAssertTrue(out.hasSuffix("here. The rest is on screen."), out)
    }

    func testSpeechChunks() {
        XCTAssertEqual(VoiceRules.speechChunks("Hello there."), ["Hello there."])
        let text = "One two three. Four five six! Seven eight nine? Ten eleven twelve."
        let chunks = VoiceRules.speechChunks(text, max: 30)
        XCTAssertTrue(chunks.allSatisfy { $0.count <= 30 })
        XCTAssertEqual(chunks.joined(separator: " "), text)
        let words = Array(repeating: "word", count: 40).joined(separator: " ")
        XCTAssertEqual(VoiceRules.speechChunks(words, max: 50).joined(separator: " "), words)
        XCTAssertEqual(VoiceRules.speechChunks("  "), [])
    }

    func testEchoAndStopWords() {
        XCTAssertTrue(VoiceRules.isEcho("all tests pass the build", speaking: "All tests pass. The build is green."))
        XCTAssertFalse(VoiceRules.isEcho("wait run the linter first", speaking: "All tests pass. The build is green."))
        XCTAssertTrue(VoiceRules.isEcho("  ", speaking: "anything"))
        for word in ["Stop", " stop. ", "okay stop", "be quiet"] { XCTAssertTrue(VoiceRules.isStopWord(word), word) }
        XCTAssertFalse(VoiceRules.isStopWord("stop the server and restart it"))
    }

    func testDictationJoins() {
        XCTAssertEqual(VoiceRules.joinDictation("", "fix the test"), "fix the test")
        XCTAssertEqual(VoiceRules.joinDictation("Please", "fix the test"), "Please fix the test")
        XCTAssertEqual(VoiceRules.joinDictation("Line one\n", "two"), "Line one\ntwo")
    }

    // MARK: Cues

    private func user(_ text: String, _ key: String = "u") -> Item { .user(key: key, text: text, attachments: [], steered: false) }
    private func answer(_ text: String, _ key: String = "a") -> Item { .assistant(key: key, itemId: key, text: text, streaming: false) }
    private func turn(_ status: String, _ key: String = "t") -> Item { .turn(key: key, start: 1, end: 2, status: status) }

    func testCues() {
        let items = [user("old", "u0"), answer("Old answer", "a0"), turn("completed", "t0"), user("hi"), turn("completed"), answer("First."), answer("**Final** answer.", "a2")]
        XCTAssertEqual(VoiceRules.cue(items)?.cue, VoiceRules.Cue(kind: .answer, text: "Final answer."))
        XCTAssertNotEqual(VoiceRules.cue([user("hi"), turn("completed", "t1")])?.key, VoiceRules.cue([user("hi"), turn("completed", "t2")])?.key)
        XCTAssertEqual(VoiceRules.cue([user("hi"), turn("completed")])?.cue, VoiceRules.Cue(kind: .answer, text: "Done."))
        XCTAssertNil(VoiceRules.cue([user("hi"), turn("running"), answer("partial")]))
        XCTAssertNil(VoiceRules.cue([user("a", "u0"), turn("completed", "t0"), answer("Earlier"), user("b")]))
        XCTAssertNil(VoiceRules.cue([]))
        let ask = ApprovalItem(key: "p", requestId: "r", approvalKind: "command", title: "echo approved", detail: "needs to run a command")
        XCTAssertEqual(VoiceRules.cue([user("hi"), turn("running"), .approval(ask)])?.cue, VoiceRules.Cue(kind: .approval, text: "Needs your approval: echo approved: needs to run a command"))
        var allowed = ask
        allowed.decision = .allow
        XCTAssertNil(VoiceRules.cue([user("hi"), turn("running"), .approval(allowed)]))
        XCTAssertEqual(VoiceRules.cue([user("hi"), turn("failed"), .error(key: "e", message: "fake failure: the model is unavailable")])?.cue,
                       VoiceRules.Cue(kind: .failed, text: "The turn failed: fake failure: the model is unavailable"))
        XCTAssertEqual(VoiceRules.cue([user("hi"), turn("interrupted")])?.cue, VoiceRules.Cue(kind: .stopped, text: ""))
    }

    // MARK: The state machine

    private func run(_ inputs: [VoiceRules.Input], from: VoiceRules.Mode = .off) -> (mode: VoiceRules.Mode, effects: [String]) {
        var mode = from
        var effects: [String] = []
        for input in inputs {
            let step = VoiceRules.step(mode, input)
            mode = step.mode
            effects += step.effects.map {
                switch $0 {
                case .listen: "listen"
                case .unlisten: "unlisten"
                case .speak(let t): "speak:\(t)"
                case .hush: "hush"
                case .arm: "arm"
                case .disarm: "disarm"
                case .send: "send"
                }
            }
        }
        return (mode, effects)
    }

    func testVoiceMode() {
        XCTAssertEqual(run([.on]).mode.phase, .speaking)
        XCTAssertEqual(run([.on]).effects, ["listen", "speak:Voice mode on."])
        XCTAssertEqual(run([.on, .spoken]).mode.phase, .listening)
        XCTAssertEqual(run([.on, .spoken, .heard, .heard]).effects.suffix(2), ["arm", "arm"])

        let listening = run([.on, .spoken]).mode
        XCTAssertEqual(run([.silence(hasText: false)], from: listening).effects, [])
        XCTAssertEqual(run([.silence(hasText: true)], from: listening).effects, ["disarm", "send"])
        XCTAssertEqual(run([.sent], from: listening).mode.phase, .waiting)
        XCTAssertEqual(run([.sent], from: listening).effects, ["disarm", "unlisten"])

        let waiting = run([.on, .spoken, .sent]).mode
        let spoken = run([.cue(.init(kind: .answer, text: "All done."))], from: waiting)
        XCTAssertEqual(spoken.mode.phase, .speaking)
        XCTAssertEqual(spoken.effects, ["listen", "speak:All done."])
        XCTAssertEqual(run([.spoken], from: spoken.mode).mode.phase, .listening)

        let approval = run([.cue(.init(kind: .approval, text: "Needs your approval: x")), .spoken], from: waiting)
        XCTAssertEqual(approval.mode.phase, .waiting)
        XCTAssertEqual(approval.effects, ["listen", "speak:Needs your approval: x", "unlisten"])

        let stopped = run([.cue(.init(kind: .stopped, text: ""))], from: waiting)
        XCTAssertEqual(stopped.mode.phase, .listening)
        XCTAssertEqual(stopped.effects, ["listen"])

        let barge = run([.heard], from: spoken.mode)
        XCTAssertEqual(barge.mode.phase, .listening)
        XCTAssertEqual(barge.effects, ["hush", "arm"])

        let speaking = run([.on]).mode
        XCTAssertEqual(run([.stop], from: speaking).effects, ["hush"])
        XCTAssertEqual(run([.sent], from: speaking).effects, ["hush", "disarm", "unlisten"])
        for from in [speaking, listening, waiting] {
            XCTAssertEqual(run([.off], from: from).mode, .off)
        }
    }
}
