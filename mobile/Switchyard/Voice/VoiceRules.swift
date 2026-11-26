import Foundation

// The voice rules, ported from ui/src/voice.ts: what is read aloud, echo and
// stop words, the cue a session gives, and voice mode's state machine. Pure,
// so the unit tests cover them; VoiceController drives the audio.

enum VoiceRules {
    /// How long a pause in speech has to be before voice mode sends.
    static let silence: Duration = .milliseconds(1500)
    /// Longest answer read aloud; the rest stays on screen.
    static let speakMax = 2500

    private static let code = "Code block omitted."
    private static let table = "Table omitted."

    private static func sentence(_ s: String) -> String {
        guard !s.isEmpty else { return s }
        var t = Substring(s)
        while let c = t.last, "\"')]".contains(c) { t = t.dropLast() }
        if let end = t.last, ".!?:;…".contains(end) { return s }
        return s + "."
    }

    private static func replace(_ s: String, _ pattern: String, _ template: String) -> String {
        guard let re = try? NSRegularExpression(pattern: pattern) else { return s }
        return re.stringByReplacingMatches(in: s, range: NSRange(s.startIndex..., in: s), withTemplate: template)
    }

    private static func matches(_ s: String, _ pattern: String) -> [String]? {
        guard let re = try? NSRegularExpression(pattern: pattern),
              let m = re.firstMatch(in: s, range: NSRange(s.startIndex..., in: s)) else { return nil }
        return (0..<m.numberOfRanges).map { i in
            Range(m.range(at: i), in: s).map { String(s[$0]) } ?? ""
        }
    }

    /// One line of Markdown as words: links read as their text, a bare URL as
    /// "a link", code and emphasis marks dropped.
    static func inlineSpeech(_ line: String) -> String {
        var s = line
        s = replace(s, #"!\[[^\]]*\]\([^)]*\)"#, "")
        s = replace(s, #"\[([^\]]+)\]\([^)]*\)"#, "$1")
        s = replace(s, #"<?https?://[^\s>)]+>?"#, "a link")
        s = replace(s, #"`([^`]*)`"#, "$1")
        s = replace(s, #"</?[a-zA-Z][^>]*>"#, "")
        s = replace(s, #"\*\*|__|~~"#, "")
        s = replace(s, #"(^|[\s(])[*_](?=\S)"#, "$1")
        s = replace(s, #"(\S)[*_](?=[\s).,!?:;]|$)"#, "$1")
        return s
    }

    private static let fence = #"^\s{0,3}(```|~~~)"#
    private static let tableDelimiter = #"^\s*\|?\s*:?-+:?\s*(\|\s*:?-+:?\s*)+\|?\s*$"#
    private static let heading = #"^\s{0,3}#{1,6}\s+(.*?)(\s+#+)?\s*$"#
    private static let listItem = #"^\s*(?:[-*+]|\d{1,9}[.)])\s+(.*)$"#
    private static let quote = #"^\s{0,3}>\s?(.*)$"#

    /// An agent's Markdown answer as text a speech engine can read: prose only.
    static func speakable(_ markdown: String, max: Int = speakMax) -> String {
        let lines = markdown.replacingOccurrences(of: "\r\n", with: "\n").replacingOccurrences(of: "\r", with: "\n").components(separatedBy: "\n")
        var parts: [String] = []
        var para: [String] = []
        func endPara() {
            let text = para.joined(separator: " ").trimmingCharacters(in: .whitespaces)
            if !text.isEmpty { parts.append(sentence(text)) }
            para = []
        }
        var i = 0
        while i < lines.count {
            let line = lines[i]
            if matches(line, fence) != nil {
                endPara()
                i += 1
                while i < lines.count, matches(lines[i], fence) == nil { i += 1 }
                parts.append(code)
                i += 1
                continue
            }
            if line.contains("|"), i + 1 < lines.count, matches(lines[i + 1], tableDelimiter) != nil {
                endPara()
                i += 2
                while i < lines.count, lines[i].contains("|"), !lines[i].trimmingCharacters(in: .whitespaces).isEmpty { i += 1 }
                parts.append(table)
                continue
            }
            if line.trimmingCharacters(in: .whitespaces).isEmpty {
                endPara()
                i += 1
                continue
            }
            if let m = matches(line, heading) ?? matches(line, listItem) {
                endPara()
                let text = inlineSpeech(m[1]).trimmingCharacters(in: .whitespaces)
                if !text.isEmpty { parts.append(sentence(text)) }
                i += 1
                continue
            }
            let q = matches(line, quote)
            para.append(inlineSpeech(q?[1] ?? line).trimmingCharacters(in: .whitespaces))
            i += 1
        }
        // The last paragraph keeps its own ending: it may be a fragment on purpose.
        let tail = para.joined(separator: " ").trimmingCharacters(in: .whitespaces)
        if !tail.isEmpty { parts.append(tail) }
        let text = replace(parts.joined(separator: " "), #"\s+"#, " ").trimmingCharacters(in: .whitespaces)
        return cap(text, max)
    }

    static func cap(_ text: String, _ max: Int) -> String {
        guard text.count > max else { return text }
        let head = String(text.prefix(max))
        let ends = [". ", "! ", "? "].compactMap { head.range(of: $0, options: .backwards)?.lowerBound }
        if let end = ends.max(), head.distance(from: head.startIndex, to: end) > max / 2 {
            return "\(head[...end]) The rest is on screen."
        }
        let cut = head.range(of: " ", options: .backwards).map { String(head[..<$0.lowerBound]) } ?? head
        return "\(cut.trimmingCharacters(in: .whitespaces))… The rest is on screen."
    }

    /// Text in sentence-sized pieces, so Stop and talking over it are quick.
    static func speechChunks(_ text: String, max: Int = 200) -> [String] {
        guard let re = try? NSRegularExpression(pattern: #"[^.!?]+(?:[.!?]+["')\]]*|$)"#) else { return [text] }
        let sentences = re.matches(in: text, range: NSRange(text.startIndex..., in: text))
            .compactMap { Range($0.range, in: text).map { String(text[$0]).trimmingCharacters(in: .whitespacesAndNewlines) } }
            .filter { !$0.isEmpty }
        var pieces: [String] = []
        for s in sentences {
            if s.count <= max { pieces.append(s); continue }
            var cur = ""
            for word in s.split(whereSeparator: \.isWhitespace) {
                if !cur.isEmpty, cur.count + 1 + word.count > max {
                    pieces.append(cur)
                    cur = String(word)
                } else { cur = cur.isEmpty ? String(word) : "\(cur) \(word)" }
            }
            if !cur.isEmpty { pieces.append(cur) }
        }
        var chunks: [String] = []
        for p in pieces {
            if let last = chunks.last, last.count + 1 + p.count <= max { chunks[chunks.count - 1] = "\(last) \(p)" } else { chunks.append(p) }
        }
        return chunks
    }

    /// Dictated words go after what is already in the box.
    static func joinDictation(_ base: String, _ spoken: String) -> String {
        if spoken.isEmpty { return base }
        if base.isEmpty || base.last?.isWhitespace == true { return base + spoken }
        return "\(base) \(spoken)"
    }

    static func words(_ s: String) -> [String] {
        let cleaned = s.lowercased().unicodeScalars.map { CharacterSet.letters.contains($0) || CharacterSet.decimalDigits.contains($0) || $0 == "'" ? Character($0) : " " }
        return String(cleaned).split(separator: " ").map(String.init)
    }

    /// Whether what the microphone picked up is the answer coming out of the
    /// speaker rather than the user: most of its words are in the answer.
    static func isEcho(_ heard: String, speaking: String) -> Bool {
        let got = words(heard)
        if got.isEmpty { return true }
        let said = Set(words(speaking))
        return Double(got.filter { said.contains($0) }.count) / Double(got.count) >= 0.7
    }

    /// "Stop" said over an answer ends the speech; it is not a message.
    static func isStopWord(_ text: String) -> Bool {
        let joined = words(text).joined(separator: " ")
        return matches(joined, #"^(?:ok(?:ay)? )?(?:stop(?: it| talking| speaking)?|quiet|be quiet|silence|shh+|hush|cancel|enough)$"#) != nil
    }

    // MARK: Cues

    enum CueKind: Equatable { case answer, approval, failed, stopped }

    struct Cue: Equatable {
        var kind: CueKind
        var text: String
    }

    /// One line of text for speech: Markdown marks and newlines dropped, capped.
    /// As `plainLine` in ui/src/notify.ts.
    static func plainLine(_ s: String, _ max: Int = 160) -> String {
        var line = replace(s, #"```[^\n]*\n?"#, "")
        line = replace(line, #"!?\[([^\]]*)\]\([^)]*\)"#, "$1")
        line = replace(line, #"(?m)^\s{0,3}(#+|>|[-*+]|\d+\.)\s+"#, "")
        line = replace(line, #"[*_`~]"#, "")
        line = replace(line, #"\s+"#, " ").trimmingCharacters(in: .whitespaces)
        guard line.count > max else { return line }
        return String(line.prefix(max - 1)).trimmingCharacters(in: .whitespaces) + "…"
    }

    /// What voice mode would say about the session as its items stand: an
    /// approval waiting, or how the current turn ended. `key` names that
    /// moment, so the composer speaks when it changes.
    static func cue(_ items: [Item]) -> (key: String, cue: Cue)? {
        let turn = Array(items[ThreadReducer.turnStart(items)...])
        for it in turn.reversed() {
            if case .approval(let a) = it, a.decision == nil, !a.unanswered {
                let what = plainLine([a.title, a.detail].filter { !$0.isEmpty }.joined(separator: ": "), 200)
                return ("approval:\(a.key)", Cue(kind: .approval, text: what.isEmpty ? "Needs your approval." : "Needs your approval: \(what)"))
            }
        }
        guard let end = turn.reversed().first(where: { if case .turn = $0 { return true }; return false }),
              case .turn(let k, _, _, let status) = end, status != "running" else { return nil }
        let key = "turn:\(k):\(status)"
        if status == "interrupted" { return (key, Cue(kind: .stopped, text: "")) }
        if status == "failed" {
            var why = ""
            for it in turn.reversed() { if case .error(_, let m) = it { why = plainLine(m, 200); break } }
            return (key, Cue(kind: .failed, text: why.isEmpty ? "The turn failed." : "The turn failed: \(why)"))
        }
        var text = ""
        for it in turn.reversed() {
            if case .assistant(_, _, let t, _) = it, !t.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { text = speakable(t); break }
        }
        return (key, Cue(kind: .answer, text: text.isEmpty ? "Done." : text))
    }

    // MARK: Voice mode

    enum Phase: Equatable { case off, listening, waiting, speaking }

    /// `after` is where speaking goes when it ends: back to listening, or to
    /// waiting when it announced an approval (answered on screen, never by voice).
    struct Mode: Equatable {
        var phase: Phase
        var after: After
        enum After: Equatable { case listen, wait }
        static let off = Mode(phase: .off, after: .listen)
    }

    enum Input: Equatable {
        case on, off
        /// New words that are the user's, not an echo of the answer.
        case heard
        case silence(hasText: Bool)
        /// The composer sent a message (after the pause, or by hand).
        case sent
        case cue(Cue)
        /// The speech engine finished (or dropped) what it was saying.
        case spoken
        case stop
        /// Recognition cannot go on (blocked, no microphone).
        case failed
    }

    enum Effect: Equatable { case listen, unlisten, speak(String), hush, arm, disarm, send }

    static let greeting = "Voice mode on."

    /// One step of voice mode: the next mode, and what the composer must do.
    /// The microphone stays on while an answer is read so the user can talk
    /// over it; it is off while the agent works.
    static func step(_ mode: Mode, _ input: Input) -> (mode: Mode, effects: [Effect]) {
        let same = (mode, [Effect]())
        func speak(_ text: String, _ after: Mode.After) -> (Mode, [Effect]) {
            (Mode(phase: .speaking, after: after), (mode.phase == .speaking ? [.hush] : []) + [.listen, .speak(text)])
        }
        func settle(_ effects: [Effect]) -> (Mode, [Effect]) {
            mode.after == .wait ? (Mode(phase: .waiting, after: .listen), effects + [.unlisten]) : (Mode(phase: .listening, after: .listen), effects)
        }
        if input == .on { return mode.phase == .off ? speak(greeting, .listen) : same }
        if mode.phase == .off { return same }
        switch input {
        case .on: return same
        case .off, .failed:
            return (.off, [.disarm, .hush, .unlisten])
        case .heard:
            if mode.phase == .listening { return (mode, [.arm]) }
            if mode.phase == .speaking { return (Mode(phase: .listening, after: .listen), [.hush, .arm]) }
            return same
        case .silence(let hasText):
            return mode.phase == .listening && hasText ? (mode, [.disarm, .send]) : same
        case .sent:
            if mode.phase == .waiting { return same }
            return (Mode(phase: .waiting, after: .listen), (mode.phase == .speaking ? [.hush] : []) + [.disarm, .unlisten])
        case .cue(let cue):
            if cue.kind == .stopped { return mode.phase == .waiting ? (Mode(phase: .listening, after: .listen), [.listen]) : same }
            return speak(cue.text, cue.kind == .approval ? .wait : .listen)
        case .spoken:
            return mode.phase == .speaking ? settle([]) : same
        case .stop:
            return mode.phase == .speaking ? settle([.hush]) : same
        }
    }
}
