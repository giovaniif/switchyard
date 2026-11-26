import SwiftUI

/// An agent's Markdown as native text. Links open in Safari; nothing is HTML.
struct MarkdownView: View {
    let text: String

    var body: some View {
        let blocks = Markdown.parse(text)
        VStack(alignment: .leading, spacing: 10) {
            ForEach(Array(blocks.enumerated()), id: \.offset) { _, block in
                view(block)
            }
        }
        .textSelection(.enabled)
    }

    @ViewBuilder
    private func view(_ block: MarkdownBlock) -> some View {
        switch block {
        case .heading(let level, let text):
            Text(Markdown.inline(text))
                .font(level <= 1 ? Font.title2.weight(.bold) : level == 2 ? Font.title3.weight(.semibold) : Font.headline)
                .padding(.top, 4)
        case .paragraph(let text):
            Text(Markdown.inline(text))
                .font(.body)
                .lineSpacing(2)
                .fixedSize(horizontal: false, vertical: true)
        case .list(_, let items):
            VStack(alignment: .leading, spacing: 6) {
                ForEach(Array(items.enumerated()), id: \.offset) { _, item in
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        Group {
                            if let checked = item.checked {
                                Image(systemName: checked ? "checkmark.square.fill" : "square")
                                    .foregroundStyle(checked ? Color.yardOk : .secondary)
                            } else {
                                Text(item.marker)
                                    .foregroundStyle(.secondary)
                                    .monospacedDigit()
                            }
                        }
                        .frame(minWidth: 16, alignment: .trailing)
                        Text(Markdown.inline(item.text))
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    .padding(.leading, CGFloat(item.depth) * 18)
                }
            }
        case .code(let language, let code):
            CodeBlock(language: language, code: code)
        case .quote(let text):
            Text(Markdown.inline(text))
                .foregroundStyle(.secondary)
                .padding(.leading, 12)
                .overlay(alignment: .leading) { Capsule().fill(Color.yardInfo.opacity(0.5)).frame(width: 3) }
        case .table(let header, let rows):
            TableBlock(header: header, rows: rows)
        case .rule:
            Divider().padding(.vertical, 4)
        }
    }
}

struct CodeBlock: View {
    let language: String
    let code: String
    @State private var copied = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text(language.isEmpty ? "code" : language)
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.secondary)
                Spacer()
                Button {
                    UIPasteboard.general.string = code
                    copied = true
                    Task {
                        try? await Task.sleep(for: .seconds(1.5))
                        copied = false
                    }
                } label: {
                    Label(copied ? "Copied" : "Copy", systemImage: copied ? "checkmark" : "doc.on.doc")
                        .font(.caption)
                        .contentTransition(.symbolEffect(.replace))
                }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
            }
            .padding(.horizontal, 12)
            .padding(.top, 8)
            .padding(.bottom, 4)
            ScrollView(.horizontal, showsIndicators: false) {
                Text(code)
                    .font(.footnote.monospaced())
                    .padding(.horizontal, 12)
                    .padding(.bottom, 12)
                    .padding(.top, 2)
            }
        }
        .background(.fill.quaternary, in: .rect(cornerRadius: 14, style: .continuous))
    }
}

struct TableBlock: View {
    let header: [String]
    let rows: [[String]]

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            Grid(alignment: .leading, horizontalSpacing: 16, verticalSpacing: 8) {
                GridRow {
                    ForEach(Array(header.enumerated()), id: \.offset) { _, cell in
                        Text(Markdown.inline(cell)).font(.footnote.weight(.semibold))
                    }
                }
                Divider()
                ForEach(Array(rows.enumerated()), id: \.offset) { _, row in
                    GridRow {
                        ForEach(Array(row.enumerated()), id: \.offset) { _, cell in
                            Text(Markdown.inline(cell)).font(.footnote)
                        }
                    }
                }
            }
            .padding(12)
        }
        .background(.fill.quinary, in: .rect(cornerRadius: 14, style: .continuous))
    }
}
