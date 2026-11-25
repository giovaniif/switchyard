import SwiftUI

/// A full page with the prompt first, as on the desktop: what to do, then
/// where and with which engine.
struct NewSessionView: View {
    let yard: Yard
    @Environment(AppModel.self) private var app
    @Environment(VoiceController.self) private var voice
    @Environment(\.dismiss) private var dismiss
    @AppStorage("new.engine") private var engineRaw = Engine.claude.rawValue
    @AppStorage("new.model") private var savedModel = ""
    @AppStorage("new.effort") private var savedEffort = ""
    @AppStorage("new.permission") private var permissionRaw = Permission.ask.rawValue
    @AppStorage("new.worktree") private var worktree = true
    @AppStorage("new.cwd") private var savedCwd = ""
    @State private var host = "local"
    @State private var uploads: [UploadDraft] = []
    @State private var starting = false
    @State private var problem: String?
    @State private var pickFolder = false

    private var engine: Engine { Engine(rawValue: engineRaw) ?? .claude }
    private var models: [ModelInfo] { yard.modelsOn(host)[engine] ?? [] }
    private var model: ModelInfo? { models.first { $0.id == savedModel } ?? models.first(where: \.isDefault) ?? models.first }
    private var effort: String {
        guard let model else { return savedEffort }
        return model.efforts.contains(savedEffort) ? savedEffort : model.effortOrDefault
    }
    private var permission: Permission { Permission(rawValue: permissionRaw) ?? .ask }
    private var cwd: String {
        if host == "local", !savedCwd.isEmpty { return savedCwd }
        return yard.recentFolders(host: host).first ?? yard.workspace[host] ?? ""
    }
    private var draft: Binding<String> { Binding(get: { yard.drafts[""] ?? "" }, set: { yard.drafts[""] = $0 }) }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    Text("What should it do?")
                        .font(.largeTitle.weight(.bold))
                        .padding(.top, 8)
                    enginePicker
                    settings
                    if let problem {
                        ErrorCard(message: problem)
                    }
                }
                .padding(.horizontal, 18)
                .padding(.bottom, 20)
                .frame(maxWidth: 700)
                .frame(maxWidth: .infinity)
            }
            .scrollDismissesKeyboard(.interactively)
            .background {
                LinearGradient(colors: [engine.tint.opacity(0.14), .clear], startPoint: .top, endPoint: .center)
                    .ignoresSafeArea()
                    .animation(.smooth, value: engineRaw)
            }
            .safeAreaInset(edge: .bottom, spacing: 0) {
                Composer(
                    text: draft, uploads: $uploads,
                    placeholder: "Describe the task",
                    onSend: start,
                    upload: { name, mime, data in await yard.upload(name: name, mime: mime, data: data, sessionId: nil, host: host) }
                )
                .disabled(starting)
                .frame(maxWidth: 700)
            }
            .navigationTitle("New session")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Close", systemImage: "xmark") { dismiss() }
                }
                if starting {
                    ToolbarItem(placement: .confirmationAction) { ProgressView() }
                }
            }
            .navigationDestination(isPresented: $pickFolder) {
                FolderPicker(yard: yard, host: host, start: cwd) { picked in
                    if host == "local" { savedCwd = picked }
                    pickFolder = false
                }
            }
        }
        .onAppear {
            voice.attach(VoiceTarget(id: "", text: { yard.drafts[""] ?? "" }, setText: { yard.drafts[""] = $0 }, submit: start), cueKey: nil)
        }
        .onDisappear { voice.detach("") }
    }

    private var enginePicker: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            GlassEffectContainer(spacing: 8) {
                HStack(spacing: 8) {
                    ForEach(yard.engines(on: host)) { e in
                        Button {
                            withAnimation(.snappy) { engineRaw = e.rawValue }
                        } label: {
                            HStack(spacing: 8) {
                                EngineBadge(engine: e, size: 24)
                                Text(e.name).font(.subheadline.weight(.semibold))
                            }
                            .padding(.leading, 6)
                            .padding(.trailing, 14)
                            .padding(.vertical, 6)
                        }
                        .buttonStyle(.plain)
                        .glassEffect(e == engine ? .regular.tint(e.tint.opacity(0.35)).interactive() : .regular.interactive(), in: .capsule)
                        .accessibilityAddTraits(e == engine ? .isSelected : [])
                    }
                }
                .padding(.vertical, 4)
            }
        }
    }

    private var settings: some View {
        VStack(spacing: 0) {
            if yard.launchHosts.count > 1 {
                row("Host", "server.rack") {
                    Picker("Host", selection: $host) {
                        ForEach(yard.launchHosts, id: \.self) { Text(yard.hostLabel($0)).tag($0) }
                    }
                }
                Divider().padding(.leading, 48)
            }
            Button {
                pickFolder = true
            } label: {
                row("Folder", "folder") {
                    HStack(spacing: 4) {
                        Text(tildePath(cwd, home: yard.home[host]))
                            .lineLimit(1)
                            .truncationMode(.head)
                            .foregroundStyle(.secondary)
                        Image(systemName: "chevron.right").font(.caption.weight(.semibold)).foregroundStyle(.tertiary)
                    }
                }
            }
            .buttonStyle(.plain)
            Divider().padding(.leading, 48)
            row("Model", "cpu") {
                Menu {
                    Picker("Model", selection: Binding(get: { model?.id ?? "" }, set: { savedModel = $0 })) {
                        ForEach(models) { Text($0.name).tag($0.id) }
                    }
                    if let model, !model.efforts.isEmpty {
                        Picker("Effort", selection: Binding(get: { effort }, set: { savedEffort = $0 })) {
                            ForEach(model.efforts, id: \.self) { Text($0.capitalized).tag($0) }
                        }
                    }
                } label: {
                    HStack(spacing: 4) {
                        Text(model.map { "\($0.name)\(effort.isEmpty ? "" : " · \(effort.capitalized)")" } ?? "Default")
                        Image(systemName: "chevron.up.chevron.down").font(.caption).foregroundStyle(.tint)
                    }
                    .foregroundStyle(.primary)
                }
            }
            Divider().padding(.leading, 48)
            row("Permission", permission.symbol) {
                Picker("Permission", selection: $permissionRaw) {
                    ForEach(Permission.allCases) { Text($0.label).tag($0.rawValue) }
                }
            }
            Divider().padding(.leading, 48)
            row("Own worktree", "arrow.triangle.branch") {
                Toggle("Own worktree", isOn: $worktree).labelsHidden()
            }
        }
        .pickerStyle(.menu)
        .padding(.vertical, 4)
        .glassEffect(.regular, in: .rect(cornerRadius: 24))
    }

    private func row<Trailing: View>(_ title: String, _ symbol: String, @ViewBuilder trailing: () -> Trailing) -> some View {
        HStack(spacing: 12) {
            Image(systemName: symbol)
                .foregroundStyle(engine.tint)
                .frame(width: 24)
            Text(title)
            Spacer(minLength: 12)
            trailing()
        }
        .padding(.horizontal, 14)
        .frame(minHeight: 50)
        .contentShape(.rect)
    }

    private func start() {
        let text = draft.wrappedValue.trimmingCharacters(in: .whitespacesAndNewlines)
        let attachments = uploads.compactMap(\.attachment)
        guard !text.isEmpty || !attachments.isEmpty, !starting else { return }
        guard let model else { problem = "\(engine.name) has no models on \(yard.hostLabel(host)). Is it installed and logged in?"; return }
        guard !cwd.isEmpty else { problem = "Pick a folder for the session."; return }
        starting = true
        problem = nil
        voice.sent()
        let request = Yard.NewSession(engine: engine, model: model.id, effort: effort, cwd: cwd, text: text, permission: permission, worktree: worktree, host: host, attachments: attachments)
        Task {
            let result = await yard.create(request)
            starting = false
            switch result {
            case .success(let id):
                yard.drafts[""] = ""
                uploads = []
                app.selected = id
                dismiss()
            case .failure(let e):
                problem = e.message
                if voice.voiceOn { voice.turnOff() }
            }
        }
    }
}

/// Browses the host's folders (`fs.dirs`); repositories are marked.
struct FolderPicker: View {
    let yard: Yard
    let host: String
    let start: String
    let pick: (String) -> Void
    @State private var path = ""

    var body: some View {
        let listing = yard.dirs[DirListing.key(host, path)]
        List {
            Section {
                Button {
                    pick(path)
                } label: {
                    Label("Use \((path as NSString).lastPathComponent.isEmpty ? path : (path as NSString).lastPathComponent)", systemImage: "checkmark.circle.fill")
                        .font(.headline)
                }
            } footer: {
                Text(tildePath(path, home: yard.home[host])).font(.caption.monospaced())
            }
            let recents = yard.recentFolders(host: host)
            if !recents.isEmpty, path == start {
                Section("Recent") {
                    ForEach(recents.prefix(6), id: \.self) { folder in
                        Button {
                            pick(folder)
                        } label: {
                            Label(tildePath(folder, home: yard.home[host]), systemImage: "clock.arrow.circlepath")
                                .lineLimit(1)
                                .truncationMode(.head)
                        }
                    }
                }
            }
            Section(path) {
                if path != "/" {
                    Button {
                        go((path as NSString).deletingLastPathComponent)
                    } label: {
                        Label("Up", systemImage: "arrow.up.left")
                    }
                }
                if let listing {
                    if let error = listing.error {
                        Text(error).foregroundStyle(Color.yardBad)
                    }
                    ForEach(listing.dirs.filter { !$0.name.hasPrefix(".") }) { dir in
                        Button {
                            go((path as NSString).appendingPathComponent(dir.name))
                        } label: {
                            HStack {
                                Label(dir.name, systemImage: dir.git ? "arrow.triangle.branch" : "folder")
                                Spacer()
                                if dir.git { Text("repo").font(.caption).foregroundStyle(.secondary) }
                            }
                        }
                    }
                } else {
                    ProgressView()
                }
            }
        }
        .foregroundStyle(.primary)
        .navigationTitle("Folder")
        .navigationBarTitleDisplayMode(.inline)
        .onAppear {
            if path.isEmpty { go(start.isEmpty ? (yard.home[host] ?? "/") : start) }
        }
    }

    private func go(_ p: String) {
        path = p.isEmpty ? "/" : p
        yard.listDirs(path, host: host)
    }
}
