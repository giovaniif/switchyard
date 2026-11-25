import SwiftUI

struct SessionsView: View {
    let yard: Yard
    @Environment(AppModel.self) private var app
    @State private var query = ""
    @State private var showArchived = false

    private var visible: [SessionMeta] {
        let q = query.trimmingCharacters(in: .whitespaces).lowercased()
        return yard.sessions.values
            .filter { $0.automation == nil }
            .filter { q.isEmpty || $0.title.lowercased().contains(q) || $0.cwd.lowercased().contains(q) || ($0.branch ?? "").lowercased().contains(q) }
            .sorted { $0.updatedAt > $1.updatedAt }
    }

    var body: some View {
        @Bindable var app = app
        let all = visible
        let live = all.filter { !$0.archived }
        let attention = live.filter { $0.status == .needsApproval || $0.status == .error }
        let working = live.filter { $0.status == .running || $0.status == .queued }
        let pinned = live.filter { $0.pinned && !attention.contains($0) && !working.contains($0) }
        let recent = live.filter { !$0.pinned && !$0.isBusy && $0.status != .error }
        let archived = all.filter(\.archived)

        List(selection: $app.selected) {
            if case .offline(let why) = yard.link {
                OfflineRow(reason: why) { yard.resume() }
                    .listRowSeparator(.hidden)
            }
            section("Needs you", attention)
            section("Working", working)
            section("Pinned", pinned)
            section(attention.isEmpty && working.isEmpty && pinned.isEmpty ? nil : "Recent", recent)
            if !archived.isEmpty {
                Section(isExpanded: $showArchived) {
                    ForEach(archived) { row($0) }
                } header: {
                    Text("Archived")
                }
            }
        }
        .listStyle(.plain)
        .overlay {
            if !yard.ready, case .connecting = yard.link {
                ProgressView("Connecting to \(yard.host.host ?? "host")…")
            } else if yard.ready, all.isEmpty {
                ContentUnavailableView {
                    Label(query.isEmpty ? "No sessions yet" : "No matches", systemImage: query.isEmpty ? "sparkles" : "magnifyingglass")
                } description: {
                    Text(query.isEmpty ? "Start one and it runs on \(yard.hostLabel("local")), even with your phone locked." : "Try another word.")
                }
            }
        }
        .searchable(text: $query, placement: .navigationBarDrawer(displayMode: .automatic), prompt: "Search sessions")
        .navigationTitle("Sessions")
        .toolbar {
            ToolbarItem(placement: .topBarLeading) {
                HostMenu(yard: yard)
            }
            ToolbarItem(placement: .bottomBar) {
                HostStatsPill(yard: yard)
            }
            ToolbarSpacer(.flexible, placement: .bottomBar)
            ToolbarItem(placement: .bottomBar) {
                Button {
                    app.showNew = true
                } label: {
                    Label("New session", systemImage: "square.and.pencil")
                }
                .accessibilityIdentifier("new-session")
            }
        }
        .refreshable { yard.resume() }
    }

    @ViewBuilder
    private func section(_ title: String?, _ rows: [SessionMeta]) -> some View {
        if !rows.isEmpty {
            Section {
                ForEach(rows) { row($0) }
            } header: {
                if let title { Text(title) }
            }
        }
    }

    private func row(_ s: SessionMeta) -> some View {
        SessionRow(session: s, home: yard.home[s.host], hostLabel: s.isRemote ? yard.hostLabel(s.host) : nil, agent: yard.agents.first { $0.id == s.agent })
            .tag(s.id)
            .swipeActions(edge: .leading) {
                Button {
                    yard.setPinned(s.id, !s.pinned)
                } label: {
                    Label(s.pinned ? "Unpin" : "Pin", systemImage: s.pinned ? "pin.slash" : "pin")
                }
                .tint(.yardWarn)
            }
            .swipeActions(edge: .trailing) {
                Button {
                    yard.setArchived(s.id, !s.archived)
                } label: {
                    Label(s.archived ? "Unarchive" : "Archive", systemImage: s.archived ? "tray.and.arrow.up" : "archivebox")
                }
                .tint(.indigo)
            }
            .contextMenu {
                Button(s.pinned ? "Unpin" : "Pin", systemImage: s.pinned ? "pin.slash" : "pin") { yard.setPinned(s.id, !s.pinned) }
                Button(s.archived ? "Unarchive" : "Archive", systemImage: "archivebox") { yard.setArchived(s.id, !s.archived) }
                if s.status == .running {
                    Button("Stop", systemImage: "stop.circle", role: .destructive) { yard.interrupt(s.id) }
                }
            }
    }
}

struct SessionRow: View {
    let session: SessionMeta
    let home: String?
    let hostLabel: String?
    let agent: AgentInfo?

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            ZStack(alignment: .bottomTrailing) {
                if let agent {
                    Circle().fill(agentColor(agent.color).gradient)
                        .frame(width: 36, height: 36)
                        .overlay { Text(agent.name.prefix(1)).font(.headline).foregroundStyle(.white) }
                } else {
                    EngineBadge(engine: session.engine)
                }
                if session.pinned {
                    Image(systemName: "pin.fill")
                        .font(.system(size: 9, weight: .bold))
                        .foregroundStyle(.white)
                        .padding(3)
                        .background(Color.yardWarn, in: .circle)
                        .offset(x: 4, y: 4)
                }
            }
            VStack(alignment: .leading, spacing: 3) {
                HStack(alignment: .firstTextBaseline) {
                    Text(session.displayTitle)
                        .font(.body.weight(.semibold))
                        .lineLimit(2)
                    Spacer(minLength: 6)
                    Text(shortAge(session.updated))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .monospacedDigit()
                }
                HStack(spacing: 6) {
                    if let hostLabel {
                        Label(hostLabel, systemImage: "cloud").labelStyle(.iconOnly)
                    }
                    Text(subtitle)
                        .lineLimit(1)
                        .truncationMode(.middle)
                    Spacer(minLength: 0)
                    StatusMark(session: session)
                }
                .font(.subheadline)
                .foregroundStyle(.secondary)
                if !session.pending.isEmpty {
                    Label("\(session.pending.count) queued", systemImage: "tray.full")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .padding(.vertical, 4)
        .accessibilityElement(children: .combine)
    }

    private var subtitle: String {
        if session.status == .needsApproval { return "Needs your approval" }
        let folder = session.repo.map { ($0 as NSString).lastPathComponent } ?? session.folderName
        if let branch = session.branch, !branch.isEmpty { return "\(folder) · \(branch)" }
        return folder.isEmpty ? tildePath(session.cwd, home: home) : folder
    }
}

struct OfflineRow: View {
    let reason: String
    let retry: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "wifi.exclamationmark")
                .font(.title3)
                .foregroundStyle(Color.yardWarn)
            VStack(alignment: .leading, spacing: 2) {
                Text("Offline").font(.subheadline.weight(.semibold))
                Text(reason).font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
            Button("Retry", action: retry)
                .buttonStyle(.glass)
        }
        .padding(12)
        .glassEffect(.regular.tint(Color.yardWarn.opacity(0.12)), in: .rect(cornerRadius: 18))
    }
}

/// The host this list belongs to, with its link state, and what to do with it.
struct HostMenu: View {
    let yard: Yard
    @Environment(AppModel.self) private var app

    var body: some View {
        Menu {
            Section(yard.host.host ?? "") {
                Button("Settings", systemImage: "gearshape") { app.showSettings = true }
                Button("Switch host", systemImage: "server.rack") { app.showConnect = true }
                if let id = app.selected {
                    Button("Open on the web", systemImage: "safari") { app.workspace = id }
                }
            }
        } label: {
            HStack(spacing: 6) {
                Circle()
                    .fill(dot)
                    .frame(width: 8, height: 8)
                Text(yard.demo ? "Demo" : yard.hostLabel("local"))
                    .font(.subheadline.weight(.semibold))
                    .lineLimit(1)
            }
            .padding(.horizontal, 4)
        }
        .accessibilityLabel("Host \(yard.hostLabel("local")), \(linkText)")
    }

    private var dot: Color {
        switch yard.link {
        case .online: .yardOk
        case .connecting: .yardWarn
        case .offline: .yardBad
        }
    }

    private var linkText: String {
        switch yard.link {
        case .online: "connected"
        case .connecting: "connecting"
        case .offline: "offline"
        }
    }
}

/// CPU and memory of the host, always in view as on the desktop.
struct HostStatsPill: View {
    let yard: Yard

    var body: some View {
        if let s = yard.stats {
            HStack(spacing: 10) {
                meter("cpu", s.cpu / 100)
                meter("memorychip", s.memTotal > 0 ? s.memUsed / s.memTotal : 0)
            }
            .font(.caption.monospacedDigit())
            .padding(.horizontal, 6)
            .accessibilityElement(children: .combine)
            .accessibilityLabel("CPU \(Int(s.cpu)) percent, memory \(Int(s.memTotal > 0 ? s.memUsed / s.memTotal * 100 : 0)) percent")
        }
    }

    private func meter(_ symbol: String, _ fraction: Double) -> some View {
        let f = min(1, max(0, fraction))
        return HStack(spacing: 4) {
            Image(systemName: symbol).foregroundStyle(.secondary)
            Text("\(Int(f * 100))%")
                .foregroundStyle(f > 0.85 ? Color.yardBad : f > 0.65 ? Color.yardWarn : .primary)
        }
    }
}
