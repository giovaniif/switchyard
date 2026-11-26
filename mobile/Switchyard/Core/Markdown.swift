import Foundation

/// Agent Markdown as blocks. Inline marks (emphasis, code, links) are left to
/// `AttributedString(markdown:)`; this splits what it does not handle: code
/// fences, headings, lists, quotes, tables and rules. Agent output is
/// untrusted: it only ever becomes text views.
enum MarkdownBlock: Equatable, Identifiable {
    case heading(level: Int, text: String)
    case paragraph(String)
    case list(ordered: Bool, items: [ListItem])
    case code(language: String, text: String)
    case quote(String)
    case table(header: [String], rows: [[String]])
    case rule

    struct ListItem: Equatable {
        var marker: String
        var text: String
        var depth: Int
        var checked: Bool?
    }

    var id: String {
        switch self {
        case .heading(let l, let t): "h\(l):\(t)"
        case .paragraph(let t): "p:\(t.prefix(80))\(t.count)"
        case .list(_, let items): "l:\(items.first?.text.prefix(40) ?? "")\(items.count)"
        case .code(_, let t): "c:\(t.prefix(40))\(t.count)"
        case .quote(let t): "q:\(t.prefix(40))"
        case .table(let h, let r): "t:\(h.joined())\(r.count)"
        case .rule: "rule"
        }
    }
}

enum Markdown {
    static func parse(_ source: String) -> [MarkdownBlock] {
        let lines = source.replacingOccurrences(of: "\r\n", with: "\n").components(separatedBy: "\n")
        var blocks: [MarkdownBlock] = []
        var para: [String] = []
        var i = 0
        func flush() {
            let text = para.joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines)
            if !text.isEmpty { blocks.append(.paragraph(text)) }
            para = []
        }
        while i < lines.count {
            let line = lines[i]
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            // A fence; an unclosed one (still streaming) runs to the end.
            if trimmed.hasPrefix("```") || trimmed.hasPrefix("~~~") {
                flush()
                let fence = String(trimmed.prefix(3))
                let language = String(trimmed.dropFirst(3)).trimmingCharacters(in: .whitespaces)
                var body: [String] = []
                i += 1
                while i < lines.count, !lines[i].trimmingCharacters(in: .whitespaces).hasPrefix(fence) {
                    body.append(lines[i])
                    i += 1
                }
                blocks.append(.code(language: language, text: body.joined(separator: "\n")))
                i += 1
                continue
            }
            if trimmed.isEmpty { flush(); i += 1; continue }
            if let h = heading(trimmed) { flush(); blocks.append(h); i += 1; continue }
            if isRule(trimmed) { flush(); blocks.append(.rule); i += 1; continue }
            if trimmed.contains("|"), i + 1 < lines.count, isTableDelimiter(lines[i + 1]) {
                flush()
                let header = cells(trimmed)
                var rows: [[String]] = []
                i += 2
                while i < lines.count, lines[i].contains("|"), !lines[i].trimmingCharacters(in: .whitespaces).isEmpty {
                    rows.append(cells(lines[i]))
                    i += 1
                }
                blocks.append(.table(header: header, rows: rows))
                continue
            }
            if trimmed.hasPrefix(">") {
                flush()
                var quoted: [String] = []
                while i < lines.count, lines[i].trimmingCharacters(in: .whitespaces).hasPrefix(">") {
                    var q = lines[i].trimmingCharacters(in: .whitespaces).dropFirst()
                    if q.first == " " { q = q.dropFirst() }
                    quoted.append(String(q))
                    i += 1
                }
                blocks.append(.quote(quoted.joined(separator: "\n")))
                continue
            }
            if listItem(line) != nil {
                flush()
                var items: [MarkdownBlock.ListItem] = []
                let ordered = listItem(line)!.ordered
                while i < lines.count {
                    if let item = listItem(lines[i]) {
                        // A top-level marker of the other kind starts another list.
                        if item.ordered != ordered, item.item.depth == 0 { break }
                        items.append(item.item)
                    } else if !lines[i].trimmingCharacters(in: .whitespaces).isEmpty, lines[i].hasPrefix("  "), !items.isEmpty {
                        // A wrapped line of the item above.
                        items[items.count - 1].text += " " + lines[i].trimmingCharacters(in: .whitespaces)
                    } else { break }
                    i += 1
                }
                blocks.append(.list(ordered: ordered, items: items))
                continue
            }
            para.append(line)
            i += 1
        }
        flush()
        return blocks
    }

    private static func heading(_ line: String) -> MarkdownBlock? {
        let hashes = line.prefix(while: { $0 == "#" }).count
        guard (1...6).contains(hashes), line.dropFirst(hashes).first == " " else { return nil }
        var text = line.dropFirst(hashes).trimmingCharacters(in: .whitespaces)
        while text.hasSuffix("#") { text = String(text.dropLast()).trimmingCharacters(in: .whitespaces) }
        return .heading(level: hashes, text: text)
    }

    private static func isRule(_ line: String) -> Bool {
        let chars = line.filter { !$0.isWhitespace }
        guard chars.count >= 3, let first = chars.first, "-*_".contains(first) else { return false }
        return chars.allSatisfy { $0 == first }
    }

    private static func isTableDelimiter(_ line: String) -> Bool {
        let t = line.trimmingCharacters(in: .whitespaces)
        guard t.contains("-"), t.contains("|") || t.hasPrefix("-") || t.hasPrefix(":") else { return false }
        return t.allSatisfy { "|-: ".contains($0) }
    }

    private static func cells(_ line: String) -> [String] {
        var t = line.trimmingCharacters(in: .whitespaces)
        if t.hasPrefix("|") { t.removeFirst() }
        if t.hasSuffix("|") { t.removeLast() }
        return t.components(separatedBy: "|").map { $0.trimmingCharacters(in: .whitespaces) }
    }

    private static func listItem(_ line: String) -> (ordered: Bool, item: MarkdownBlock.ListItem)? {
        let indent = line.prefix(while: { $0 == " " || $0 == "\t" }).count
        let rest = line.dropFirst(indent)
        let depth = min(3, indent / 2)
        if let first = rest.first, "-*+".contains(first), rest.dropFirst().first == " " {
            var text = String(rest.dropFirst(2))
            var checked: Bool?
            if text.hasPrefix("[ ] ") { checked = false; text.removeFirst(4) } else if text.lowercased().hasPrefix("[x] ") { checked = true; text.removeFirst(4) }
            return (false, .init(marker: "•", text: text, depth: depth, checked: checked))
        }
        let digits = rest.prefix(while: \.isNumber)
        if !digits.isEmpty, digits.count <= 9 {
            let after = rest.dropFirst(digits.count)
            if let p = after.first, p == "." || p == ")", after.dropFirst().first == " " {
                return (true, .init(marker: "\(digits).", text: String(after.dropFirst(2)), depth: depth, checked: nil))
            }
        }
        return nil
    }

    /// Inline Markdown as attributed text; plain text when it does not parse.
    static func inline(_ text: String) -> AttributedString {
        let options = AttributedString.MarkdownParsingOptions(interpretedSyntax: .inlineOnlyPreservingWhitespace, failurePolicy: .returnPartiallyParsedIfPossible)
        return (try? AttributedString(markdown: text, options: options)) ?? AttributedString(text)
    }
}
