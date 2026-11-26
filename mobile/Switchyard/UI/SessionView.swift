import SwiftUI

struct SessionView: View {
    let yard: Yard
    let id: String
    @Environment(AppModel.self) private var app
    @Environment(VoiceController.self) private var voice
    @State private var uploads: [UploadDraft] = []
    @State private var renaming = false
    @State private var newTitle = ""

    private var session: SessionMeta? { yard.sessions[id] }
    private var draft: Binding<String> {
        Binding(get: { yard.drafts[id] ?? "" }, set: { yard.drafts[id] = $0 })
    }

    var body: some View {
        let items = yard.items(id)
        let rows = ThreadRow.build(items)
        let cue = VoiceRules.cue(items)
        let running = session?.status == .running || session?.status == .queued

        ScrollView {
            LazyVStack(alignment: .leading, spacing: 16) {
                if let session { SessionHeader(session: session, yard: yard) }
                if !yard.loaded(id) {
                    ProgressView().frame(maxWidth: .infinity).padding(.top, 40)
                }
                ForEach(rows) { row in
                    rowView(row)
                }
                if running, !streamingLast(items) {
                    WorkingIndicator(since: turnStart(items))
                        .padding(.top, 2)
                }
                if session?.status == .queued, let reason = queuedReason(items) {
                    NoteLine(text: reason, symbol: "hourglass")
                }
                Color.clear.frame(height: 4).id("end")
            }
            .padding(.horizontal, 16)
            .padding(.top, 8)
            .frame(maxWidth: 760)
            .frame(maxWidth: .infinity)
        }
        .defaultScrollAnchor(.bottom)
        .defaultScrollAnchor(.bottom, for: .sizeChanges)
        .scrollDismissesKeyboard(.interactively)
        .safeAreaInset(edge: .bottom, spacing: 0) {
            VStack(spacing: 8) {
                if let pending = session?.pending, !pending.isEmpty {
                    QueuedMessages(pending: pending) { m, drop in yard.unqueue(id, messageId: m.id, drop: drop) }
                        .padding(.horizontal, 12)
                }
                Composer(
                    text: draft, uploads: $uploads,
                    placeholder: placeholder,
                    running: running,
                    onSend: send,
                    onStop: { yard.interrupt(id) },
                    upload: { name, mime, data in await yard.upload(name: name, mime: mime, data: data, sessionId: id, host: session?.host ?? "local") }
                )
                .frame(maxWidth: 760)
            }
        }
        .navigationTitle(session?.displayTitle ?? "Session")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) { menu }
        }
        .onAppear {
            yard.view(id)
            voice.attach(VoiceTarget(id: id, text: { yard.drafts[id] ?? "" }, setText: { yard.drafts[id] = $0 }, submit: send), cueKey: cue?.key)
        }
        .onDisappear {
            yard.unview(id)
            voice.detach(id)
        }
        .onChange(of: cue?.key) { _, key in
            voice.observe(cueKey: key, cue: cue?.cue, for: id)
        }
        .onChange(of: yard.returned[id]) { _, back in
            guard let back else { return }
            yard.returned[id] = nil
            draft.wrappedValue = VoiceRules.joinDictation(draft.wrappedValue, back)
        }
        .alert("Rename session", isPresented: $renaming) {
            TextField("Title", text: $newTitle)
            Button("Rename") {
                let title = newTitle
                Task { if let problem = await yard.rename(id, title: title) { yard.problem = problem } }
            }
            Button("Cancel", role: .cancel) {}
        }
    }

    private var placeholder: String {
        guard let session else { return "Reply" }
        if session.status == .running { return session.engine.canSteer ? "Steer the agent" : "Queue a message" }
        return "Reply to \(session.engine.name)"
    }

    @ViewBuilder
    private func rowView(_ row: ThreadRow) -> some View {
        switch row {
        case .tools(_, let nodes):
            ToolGroupView(nodes: nodes)
        case .item(let item):
            switch item {
            case .user(_, let text, let attachments, let steered):
                UserBubble(text: text, attachments: attachments, steered: steered)
            case .assistant(_, _, let text, let streaming):
                AssistantText(text: text, streaming: streaming)
            case .reasoning(_, _, let text, let streaming):
                ReasoningView(text: text, streaming: streaming)
            case .turn(_, let start, let end, let status):
                TurnFooter(start: start, end: end, status: status)
            case .tool(let t):
                ToolGroupView(nodes: [ToolNode(tool: t, children: [])])
            case .diff(_, let files):
                DiffCard(files: files)
            case .approval(let a):
                ApprovalCard(approval: a) { decision in yard.approve(id, requestId: a.requestId, decision: decision) }
            case .error(_, let message):
                ErrorCard(message: message)
            case .note(_, let text):
                NoteLine(text: text)
            case .queued(_, let reason):
                NoteLine(text: "Waiting for room on the host: \(reason)", symbol: "hourglass")
            }
        }
    }

    private func streamingLast(_ items: [Item]) -> Bool {
        if case .assistant(_, _, _, true)? = items.last { return true }
        if case .reasoning(_, _, _, true)? = items.last { return true }
        if case .approval(let a)? = items.last, a.decision == nil { return true }
        return false
    }

    private func turnStart(_ items: [Item]) -> Date? {
        for it in items.reversed() { if case .turn(_, let start?, nil, _) = it { return Date(timeIntervalSince1970: start / 1000) } }
        return nil
    }

    private func queuedReason(_ items: [Item]) -> String? {
        for it in items.reversed() { if case .queued(_, let reason) = it { return reason } }
        return nil
    }

    private func send() {
        let text = draft.wrappedValue.trimmingCharacters(in: .whitespacesAndNewlines)
        let attachments = uploads.compactMap(\.attachment)
        guard !text.isEmpty || !attachments.isEmpty else { return }
        yard.sendMessage(id, text: text, attachments: attachments)
        draft.wrappedValue = ""
        uploads = []
        voice.sent()
    }

    private var menu: some View {
        Menu {
            if let session {
                let models = yard.modelsOn(session.host)[session.engine] ?? []
                if !models.isEmpty {
                    Picker(selection: Binding(get: { session.model }, set: { m in
                        let effort = models.first { $0.id == m }?.effortOrDefault ?? session.effort
                        yard.setModel(id, model: m, effort: effort)
                    })) {
                        ForEach(models) { Text($0.name).tag($0.id) }
                    } label: {
                        Label("Model", systemImage: "cpu")
                    }
                    .pickerStyle(.menu)
                    let efforts = models.first { $0.id == session.model }?.efforts ?? []
                    if !efforts.isEmpty {
                        Picker(selection: Binding(get: { session.effort }, set: { yard.setModel(id, model: session.model, effort: $0) })) {
                            ForEach(efforts, id: \.self) { Text($0.capitalized).tag($0) }
                        } label: {
                            Label("Effort", systemImage: "gauge.with.dots.needle.50percent")
                        }
                        .pickerStyle(.menu)
                    }
                }
                Picker(selection: Binding(get: { session.permission }, set: { yard.setPermission(id, $0) })) {
                    ForEach(Permission.allCases) { Label($0.label, systemImage: $0.symbol).tag($0) }
                } label: {
                    Label("Permission", systemImage: "lock.shield")
                }
                .pickerStyle(.menu)
                Divider()
                Button(session.pinned ? "Unpin" : "Pin", systemImage: session.pinned ? "pin.slash" : "pin") { yard.setPinned(id, !session.pinned) }
                Button("Rename", systemImage: "pencil") {
                    newTitle = session.title
                    renaming = true
                }
                Button("Terminal, changes and PR", systemImage: "rectangle.split.2x1") { app.workspace = id }
                if let branch = session.branch {
                    Button("Copy branch", systemImage: "arrow.triangle.branch") { UIPasteboard.general.string = branch }
                }
                Divider()
                if session.status == .running {
                    Button("Stop", systemImage: "stop.circle", role: .destructive) { yard.interrupt(id) }
                }
                Button(session.archived ? "Unarchive" : "Archive", systemImage: "archivebox") {
                    yard.setArchived(id, !session.archived)
                    if !session.archived { app.selected = nil }
                }
            }
        } label: {
            Image(systemName: "ellipsis")
        }
        .accessibilityLabel("Session options")
    }
}

/// The session's engine, model and place, above its thread.
struct SessionHeader: View {
    let session: SessionMeta
    let yard: Yard

    var body: some View {
        let model = yard.modelsOn(session.host)[session.engine]?.first { $0.id == session.model }?.name ?? session.model
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                EngineBadge(engine: session.engine, size: 22)
                Text("\(session.engine.name) · \(model)\(session.effort.isEmpty ? "" : " · \(session.effort)")")
                    .font(.footnote.weight(.medium))
                Spacer(minLength: 0)
                Label(session.permission.label, systemImage: session.permission.symbol)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            HStack(spacing: 6) {
                Image(systemName: session.isRemote ? "cloud" : "folder")
                Text(tildePath(session.cwd, home: yard.home[session.host]))
                    .lineLimit(1)
                    .truncationMode(.head)
                if let branch = session.branch {
                    Image(systemName: "arrow.triangle.branch")
                    Text(branch).lineLimit(1).truncationMode(.middle)
                }
            }
            .font(.caption.monospaced())
            .foregroundStyle(.secondary)
        }
        .padding(12)
        .background(.fill.quinary, in: .rect(cornerRadius: 16, style: .continuous))
        .accessibilityElement(children: .combine)
    }
}

/// Messages sent while the agent worked, waiting for their turn.
struct QueuedMessages: View {
    let pending: [PendingMessage]
    let take: (PendingMessage, Bool) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(pending) { m in
                HStack(spacing: 8) {
                    Image(systemName: "tray.full").foregroundStyle(.secondary)
                    Text(m.text.isEmpty ? "\(m.attachments.count) attachment(s)" : m.text)
                        .font(.subheadline)
                        .lineLimit(2)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    Button("Edit") { take(m, false) }
                        .font(.footnote.weight(.semibold))
                    Button {
                        take(m, true)
                    } label: {
                        Image(systemName: "xmark").font(.caption.weight(.bold))
                    }
                    .accessibilityLabel("Remove queued message")
                }
                .buttonStyle(.plain)
                .padding(.horizontal, 14)
                .padding(.vertical, 10)
                .glassEffect(.regular, in: .rect(cornerRadius: 18))
            }
        }
    }
}
