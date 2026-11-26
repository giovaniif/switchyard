import SwiftUI

struct SettingsView: View {
    let yard: Yard
    @Environment(AppModel.self) private var app
    @Environment(VoiceController.self) private var voice
    @Environment(\.dismiss) private var dismiss
    @State private var rate: Double = 1

    var body: some View {
        NavigationStack {
            Form {
                Section("Host") {
                    LabeledContent("Name", value: yard.demo ? "Demo" : yard.hostLabel("local"))
                    LabeledContent("Address", value: yard.host.host ?? "")
                    LabeledContent("Status") {
                        switch yard.link {
                        case .online: Text("Connected").foregroundStyle(Color.yardOk)
                        case .connecting: Text("Connecting…")
                        case .offline(let why): Text(why).foregroundStyle(Color.yardBad)
                        }
                    }
                    if !yard.hosts.isEmpty {
                        ForEach(yard.hosts) { h in
                            LabeledContent(h.name) {
                                Text(h.connected ? "Connected" : h.status.capitalized)
                                    .foregroundStyle(h.connected ? Color.yardOk : .secondary)
                            }
                        }
                    }
                    Button("Switch host", systemImage: "server.rack") {
                        dismiss()
                        app.showConnect = true
                    }
                    Button("Disconnect", systemImage: "eject", role: .destructive) {
                        dismiss()
                        app.disconnect()
                    }
                }
                Section {
                    VStack(alignment: .leading) {
                        LabeledContent("Speaking rate", value: rate.formatted(.number.precision(.fractionLength(1))) + "×")
                        Slider(value: $rate, in: 0.6...1.6, step: 0.1) { Text("Speaking rate") }
                            .onChange(of: rate) { _, r in voice.rate = Float(r) }
                    }
                } header: {
                    Text("Voice")
                } footer: {
                    Text("Voice mode listens, sends when you pause, reads the answer aloud and listens again. Speech is recognized on this iPhone when it can be; only the text reaches your host. Approvals are always answered on screen.")
                }
                Section {
                    Toggle("Notify me", isOn: Binding(get: { app.notify }, set: { on in Task { await app.setNotify(on) } }))
                } header: {
                    Text("Notifications")
                } footer: {
                    Text("When a session finishes or needs approval while the app is open on another screen. There is no push service: a closed app isn't told.")
                }
                Section("About") {
                    LabeledContent("Version", value: Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "")
                    LabeledContent("Build", value: Bundle.main.object(forInfoDictionaryKey: "CFBundleVersion") as? String ?? "")
                }
            }
            .navigationTitle("Settings")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done", systemImage: "checkmark") { dismiss() }
                }
            }
            .onAppear { rate = Double(voice.rate) }
        }
    }
}
