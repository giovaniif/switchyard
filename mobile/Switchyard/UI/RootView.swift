import SwiftUI

struct RootView: View {
    @Environment(AppModel.self) private var app

    var body: some View {
        @Bindable var app = app
        Group {
            if let yard = app.yard {
                MainView(yard: yard)
            } else {
                ConnectView()
            }
        }
        .sheet(isPresented: $app.showConnect) {
            ConnectView(sheet: true)
                .presentationDetents([.large])
        }
    }
}

struct MainView: View {
    let yard: Yard
    @Environment(AppModel.self) private var app
    @State private var columns = NavigationSplitViewVisibility.all
    /// On a phone, which column shows: a session picked in code (a notification, a new session) opens.
    @State private var compact = NavigationSplitViewColumn.sidebar
    @Environment(\.horizontalSizeClass) private var size

    var body: some View {
        @Bindable var app = app
        NavigationSplitView(columnVisibility: $columns, preferredCompactColumn: $compact) {
            SessionsView(yard: yard)
        } detail: {
            if let id = app.selected, yard.sessions[id] != nil {
                SessionView(yard: yard, id: id)
                    .id(id)
            } else {
                ContentUnavailableView {
                    Label("No session open", systemImage: "bubble.left.and.text.bubble.right")
                } description: {
                    Text("Pick a session, or start a new one.")
                } actions: {
                    Button("New session", systemImage: "square.and.pencil") { app.showNew = true }
                        .buttonStyle(.glassProminent)
                }
            }
        }
        .onChange(of: app.selected, initial: true) { _, id in compact = id == nil ? .sidebar : .detail }
        .onChange(of: compact) { _, column in if column == .sidebar, size == .compact { app.selected = nil } }
        .sheet(isPresented: $app.showNew) {
            NewSessionView(yard: yard)
        }
        .sheet(isPresented: $app.showSettings) {
            SettingsView(yard: yard)
        }
        .sheet(item: Binding(get: { app.workspace.map(WorkspaceRoute.init) }, set: { app.workspace = $0?.id })) { route in
            WorkspaceView(host: yard.host, sessionId: route.id, title: yard.sessions[route.id]?.displayTitle ?? "Workspace")
        }
        .overlay(alignment: .top) {
            ProblemBanner(yard: yard)
        }
    }
}

struct WorkspaceRoute: Identifiable {
    let id: String
}

/// yardd's refusals and failures, for a few seconds at the top.
struct ProblemBanner: View {
    let yard: Yard

    var body: some View {
        if let problem = yard.problem {
            HStack(alignment: .top, spacing: 10) {
                Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(Color.yardBad)
                Text(problem).font(.subheadline).frame(maxWidth: .infinity, alignment: .leading)
                Button {
                    yard.problem = nil
                } label: {
                    Image(systemName: "xmark").font(.footnote.weight(.semibold))
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Dismiss")
            }
            .padding(14)
            .glassEffect(.regular, in: .rect(cornerRadius: 20))
            .padding(.horizontal)
            .transition(.move(edge: .top).combined(with: .opacity))
            .task(id: problem) {
                try? await Task.sleep(for: .seconds(6))
                withAnimation { yard.problem = nil }
            }
        }
    }
}
