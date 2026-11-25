import SwiftUI

/// What the thread draws: items, with consecutive tool calls grouped and a
/// subagent's steps nested under its call.
enum ThreadRow: Identifiable {
    case item(Item)
    case tools(key: String, nodes: [ToolNode])

    var id: String {
        switch self {
        case .item(let it): it.id
        case .tools(let key, _): "tools-\(key)"
        }
    }

    static func build(_ items: [Item]) -> [ThreadRow] {
        // Steps under an Agent call are drawn inside it.
        var children: [String: [ToolItem]] = [:]
        var agents = Set<String>()
        for it in items { if case .tool(let t) = it, t.agent != nil { agents.insert(t.itemId) } }
        for it in items {
            if case .tool(let t) = it, let p = t.parent, agents.contains(p) { children[p, default: []].append(t) }
        }
        func node(_ t: ToolItem) -> ToolNode { ToolNode(tool: t, children: (children[t.itemId] ?? []).map(node)) }

        var rows: [ThreadRow] = []
        var run: [ToolNode] = []
        func flush() {
            if let first = run.first { rows.append(.tools(key: first.tool.key, nodes: run)) }
            run = []
        }
        for it in items {
            switch it {
            case .tool(let t):
                if let p = t.parent, agents.contains(p) { continue }
                run.append(node(t))
            case .turn(_, _, let end, _) where end == nil:
                continue
            default:
                flush()
                rows.append(.item(it))
            }
        }
        flush()
        return rows
    }
}

struct ToolNode: Identifiable {
    let tool: ToolItem
    let children: [ToolNode]
    var id: String { tool.key }
}

// MARK: Messages

struct UserBubble: View {
    let text: String
    let attachments: [Attachment]
    let steered: Bool

    var body: some View {
        HStack {
            Spacer(minLength: 48)
            VStack(alignment: .trailing, spacing: 6) {
                if !attachments.isEmpty {
                    FlowChips(attachments: attachments)
                }
                if !text.isEmpty {
                    Text(Markdown.inline(text))
                        .font(.body)
                        .foregroundStyle(.white)
                        .tint(.white)
                        .textSelection(.enabled)
                        .padding(.horizontal, 16)
                        .padding(.vertical, 11)
                        .background(Color.yardInfo.gradient, in: .rect(cornerRadius: 22, style: .continuous))
                }
                if steered {
                    Label("Sent while it worked", systemImage: "arrow.turn.down.right")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel("You: \(text)")
    }
}

struct FlowChips: View {
    let attachments: [Attachment]
    var body: some View {
        HStack(spacing: 6) {
            ForEach(attachments) { a in
                Label(a.name, systemImage: a.isImage ? "photo" : "doc")
                    .font(.caption)
                    .lineLimit(1)
                    .padding(.horizontal, 10)
                    .padding(.vertical, 6)
                    .background(.fill.tertiary, in: .capsule)
            }
        }
    }
}

struct AssistantText: View {
    let text: String
    let streaming: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            MarkdownView(text: text)
            if streaming {
                StreamingCaret().padding(.top, 4)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

struct StreamingCaret: View {
    @State private var on = false
    var body: some View {
        Capsule()
            .fill(Color.yardInfo)
            .frame(width: 18, height: 4)
            .opacity(on ? 1 : 0.25)
            .onAppear { withAnimation(.easeInOut(duration: 0.6).repeatForever()) { on = true } }
            .accessibilityHidden(true)
    }
}

struct ReasoningView: View {
    let text: String
    let streaming: Bool
    @State private var open = false

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Button {
                withAnimation(.snappy) { open.toggle() }
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: "sparkles")
                        .symbolEffect(.pulse, isActive: streaming)
                    Text(streaming ? "Thinking" : "Thought")
                        .font(.subheadline.weight(.medium))
                    Image(systemName: "chevron.right")
                        .font(.caption.weight(.semibold))
                        .rotationEffect(.degrees(open ? 90 : 0))
                    Spacer(minLength: 0)
                }
                .foregroundStyle(.secondary)
                .contentShape(.rect)
            }
            .buttonStyle(.plain)
            .accessibilityHint(open ? "Hides the thinking" : "Shows the thinking")
            if open || streaming {
                Text(Markdown.inline(open ? text : String(text.suffix(160))))
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .lineLimit(open ? nil : 2)
                    .padding(.leading, 22)
                    .overlay(alignment: .leading) {
                        Capsule().fill(.quaternary).frame(width: 2).padding(.leading, 6)
                    }
                    .textSelection(.enabled)
            }
        }
    }
}

// MARK: Tools

struct ToolGroupView: View {
    let nodes: [ToolNode]
    @State private var expanded = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            let shown = nodes.count > 4 && !expanded ? Array(nodes.suffix(3)) : nodes
            if nodes.count > 4, !expanded {
                Button {
                    withAnimation(.snappy) { expanded = true }
                } label: {
                    Label("\(nodes.count - 3) earlier steps", systemImage: "ellipsis")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                        .padding(.horizontal, 14)
                        .padding(.vertical, 10)
                }
                .buttonStyle(.plain)
                Divider().padding(.leading, 14)
            }
            ForEach(Array(shown.enumerated()), id: \.element.id) { i, node in
                ToolRowView(node: node, depth: 0)
                if i < shown.count - 1 { Divider().padding(.leading, 46) }
            }
        }
        .background(.fill.quinary, in: .rect(cornerRadius: 18, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 18, style: .continuous).strokeBorder(.quaternary, lineWidth: 0.5))
    }
}

struct ToolRowView: View {
    let node: ToolNode
    let depth: Int
    @State private var open = false

    private var t: ToolItem { node.tool }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Button {
                withAnimation(.snappy) { open.toggle() }
            } label: {
                HStack(spacing: 10) {
                    Image(systemName: symbol)
                        .font(.footnote.weight(.semibold))
                        .foregroundStyle(tint)
                        .frame(width: 22)
                    VStack(alignment: .leading, spacing: 1) {
                        Text(label)
                            .font(.subheadline.weight(.medium))
                            .lineLimit(1)
                        if !t.detail.isEmpty {
                            Text(t.detail)
                                .font(.caption.monospaced())
                                .foregroundStyle(.secondary)
                                .lineLimit(open ? 6 : 1)
                                .truncationMode(.middle)
                        }
                    }
                    Spacer(minLength: 4)
                    state
                }
                .padding(.horizontal, 14)
                .padding(.vertical, 10)
                .contentShape(.rect)
            }
            .buttonStyle(.plain)
            .accessibilityLabel("\(label), \(t.detail), \(stateText)")

            if open, let output = t.output, !output.isEmpty {
                ScrollView(.horizontal, showsIndicators: false) {
                    Text(String(output.prefix(4000)))
                        .font(.caption.monospaced())
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                        .padding(10)
                }
                .frame(maxHeight: 220)
                .background(.fill.quaternary, in: .rect(cornerRadius: 10))
                .padding(.horizontal, 14)
                .padding(.bottom, 10)
            }
            if !node.children.isEmpty {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(node.children) { child in
                        ToolRowView(node: child, depth: depth + 1)
                    }
                }
                .padding(.leading, 22)
                .overlay(alignment: .leading) {
                    Rectangle().fill(.quaternary).frame(width: 1.5).padding(.leading, 24).padding(.vertical, 4)
                }
            }
        }
    }

    private var label: String {
        if let agent = t.agent { return "Subagent · \(agent)" }
        return t.title.isEmpty ? t.toolKind.capitalized : t.title
    }

    private var symbol: String {
        if t.agent != nil { return "person.2.fill" }
        switch t.toolKind {
        case "command": return "terminal"
        case "read": return "doc.text"
        case "edit", "write", "patch": return "pencil"
        case "search", "grep", "glob": return "magnifyingglass"
        case "web", "fetch": return "globe"
        case "mcp": return "puzzlepiece.extension"
        case "todo", "plan": return "checklist"
        default: return "wrench.and.screwdriver"
        }
    }

    private var tint: Color {
        if t.asked == .denied || t.state == .failed { return .yardBad }
        if t.asked == .waiting { return .yardWarn }
        return .secondary
    }

    @ViewBuilder private var state: some View {
        if t.asked == .waiting {
            Image(systemName: "hand.raised.fill").foregroundStyle(Color.yardWarn).font(.footnote)
        } else if t.asked == .denied {
            Text("Denied").font(.caption.weight(.medium)).foregroundStyle(Color.yardBad)
        } else {
            switch t.state {
            case .running: ProgressView().controlSize(.small)
            case .ok:
                HStack(spacing: 4) {
                    if let ms = t.ms { Text(duration(ms)).font(.caption2).foregroundStyle(.tertiary) }
                    Image(systemName: "checkmark").font(.caption.weight(.bold)).foregroundStyle(Color.yardOk)
                }
            case .failed: Image(systemName: "xmark").font(.caption.weight(.bold)).foregroundStyle(Color.yardBad)
            }
        }
    }

    private var stateText: String {
        if t.asked == .waiting { return "waiting for approval" }
        if t.asked == .denied { return "denied" }
        switch t.state {
        case .running: return "running"
        case .ok: return "done"
        case .failed: return "failed"
        }
    }
}

// MARK: Cards

struct DiffCard: View {
    let files: [ChangedFile]

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Label(files.count == 1 ? "Edited 1 file" : "Edited \(files.count) files", systemImage: "doc.badge.plus")
                .font(.subheadline.weight(.semibold))
            ForEach(files) { f in
                HStack(spacing: 8) {
                    Text((f.path as NSString).lastPathComponent)
                        .font(.footnote.monospaced())
                        .lineLimit(1)
                    Text((f.path as NSString).deletingLastPathComponent)
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                        .lineLimit(1)
                        .truncationMode(.head)
                    Spacer(minLength: 4)
                    if let a = f.added, a > 0 { Text("+\(a)").foregroundStyle(Color.yardOk) }
                    if let r = f.removed, r > 0 { Text("−\(r)").foregroundStyle(Color.yardBad) }
                }
                .font(.caption.monospacedDigit().weight(.medium))
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.fill.quinary, in: .rect(cornerRadius: 18, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 18, style: .continuous).strokeBorder(.quaternary, lineWidth: 0.5))
    }
}

struct ApprovalCard: View {
    let approval: ApprovalItem
    let decide: (Decision) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 8) {
                Image(systemName: approval.decision == nil && !approval.unanswered ? "hand.raised.fill" : "hand.raised")
                    .foregroundStyle(Color.yardWarn)
                Text(heading)
                    .font(.subheadline.weight(.semibold))
                Spacer()
            }
            if !approval.title.isEmpty {
                Text(approval.title)
                    .font(.callout.monospaced())
                    .textSelection(.enabled)
                    .padding(10)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(.fill.tertiary, in: .rect(cornerRadius: 10))
            }
            if !approval.detail.isEmpty {
                Text(approval.detail).font(.footnote).foregroundStyle(.secondary)
            }
            if approval.decision == nil, !approval.unanswered {
                GlassEffectContainer(spacing: 8) {
                    HStack(spacing: 8) {
                        Button {
                            decide(.allow)
                        } label: {
                            Text("Allow once").frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.glassProminent)
                        Button {
                            decide(.allowAlways)
                        } label: {
                            Text("Always").frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.glass)
                        Button(role: .destructive) {
                            decide(.deny)
                        } label: {
                            Text("Deny").frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.glass)
                        .tint(.yardBad)
                    }
                    .controlSize(.large)
                    .disabled(approval.sending)
                }
            }
        }
        .padding(14)
        .background(Color.yardWarn.opacity(0.08), in: .rect(cornerRadius: 20, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 20, style: .continuous).strokeBorder(Color.yardWarn.opacity(0.35), lineWidth: 1))
    }

    private var heading: String {
        if approval.sending { return "Sending…" }
        switch approval.decision {
        case .allow: return "Allowed once"
        case .allowAlways: return "Always allowed"
        case .deny: return "Denied"
        case nil: return approval.unanswered ? "Not answered" : "Needs your approval"
        }
    }
}

struct ErrorCard: View {
    let message: String
    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: "exclamationmark.octagon.fill").foregroundStyle(Color.yardBad)
            Text(message)
                .font(.callout)
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(14)
        .background(Color.yardBad.opacity(0.08), in: .rect(cornerRadius: 18, style: .continuous))
    }
}

struct NoteLine: View {
    let text: String
    var symbol = "info.circle"
    var body: some View {
        Label(text, systemImage: symbol)
            .font(.footnote)
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity)
            .multilineTextAlignment(.center)
    }
}

/// "Worked for 1m 12s" under a finished turn.
struct TurnFooter: View {
    let start: Double?
    let end: Double?
    let status: String

    var body: some View {
        HStack(spacing: 8) {
            line
            Group {
                switch status {
                case "failed": Label("Turn failed", systemImage: "xmark.circle").foregroundStyle(Color.yardBad)
                case "interrupted": Label("Stopped", systemImage: "stop.circle")
                default:
                    if let start, let end, end > start { Text("Worked for \(duration(end - start))") } else { Text("Done") }
                }
            }
            .font(.caption)
            .foregroundStyle(.secondary)
            .fixedSize()
            line
        }
        .padding(.vertical, 2)
    }

    private var line: some View { Rectangle().fill(.quaternary).frame(height: 0.5) }
}

/// Shown at the end of the thread while the agent works between messages.
struct WorkingIndicator: View {
    let since: Date?

    var body: some View {
        TimelineView(.periodic(from: .now, by: 1)) { context in
            HStack(spacing: 8) {
                ProgressView().controlSize(.small)
                Text("Working")
                    .font(.subheadline.weight(.medium))
                if let since {
                    Text(duration(context.date.timeIntervalSince(since) * 1000))
                        .font(.caption.monospacedDigit())
                        .foregroundStyle(.tertiary)
                }
            }
            .foregroundStyle(.secondary)
        }
        .accessibilityElement(children: .combine)
    }
}
