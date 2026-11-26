import XCTest
@testable import Switchyard

final class MarkdownTests: XCTestCase {
    func testBlocks() {
        let md = """
        ## Release plan

        Ship the **native** app:

        1. Sessions
        2. Voice
        - [x] done
        - [ ] todo

        ```swift
        let glass = Glass.regular
        ```

        > Quoted

        | A | B |
        |---|---|
        | 1 | 2 |

        ---
        Tail
        """
        let blocks = Markdown.parse(md)
        XCTAssertEqual(blocks.first, .heading(level: 2, text: "Release plan"))
        XCTAssertEqual(blocks[1], .paragraph("Ship the **native** app:"))
        guard case .list(true, let items) = blocks[2] else { return XCTFail("\(blocks[2])") }
        XCTAssertEqual(items.map(\.text), ["Sessions", "Voice"])
        guard case .list(false, let checks) = blocks[3] else { return XCTFail("\(blocks[3])") }
        XCTAssertEqual(checks.map(\.checked), [true, false])
        XCTAssertEqual(blocks[4], .code(language: "swift", text: "let glass = Glass.regular"))
        XCTAssertEqual(blocks[5], .quote("Quoted"))
        XCTAssertEqual(blocks[6], .table(header: ["A", "B"], rows: [["1", "2"]]))
        XCTAssertEqual(blocks[7], .rule)
        XCTAssertEqual(blocks[8], .paragraph("Tail"))
    }

    func testAnUnclosedFenceWhileStreamingRunsToTheEnd() {
        XCTAssertEqual(Markdown.parse("Start:\n```sh\nrm -rf build"), [.paragraph("Start:"), .code(language: "sh", text: "rm -rf build")])
    }

    func testInlineMarksBecomeAttributes() {
        let s = Markdown.inline("See [docs](https://example.com) and `code`")
        XCTAssertEqual(String(s.characters), "See docs and code")
        XCTAssertNotNil(s.runs.first { $0.link != nil })
    }
}
