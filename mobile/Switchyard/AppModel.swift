import SwiftUI
import Observation
import UserNotifications

/// The app's state above one host: which host, which session, voice, settings.
@MainActor
@Observable
final class AppModel {
    private(set) var yard: Yard?
    private(set) var recent: [String] = UserDefaults.standard.stringArray(forKey: "hosts") ?? []
    var checking = false
    var connectError: String?
    /// An address from a `switchyard://connect` link, waiting for the user to choose Connect.
    var linkedAddress: String?
    var selected: String?
    var showNew = false
    var showSettings = false
    var showConnect = false
    var workspace: String?
    let voice: VoiceController
    let demo: Bool
    /// `-screen <name>` for screenshots: which screen the demo opens on.
    let screen: String?

    var notify: Bool {
        get { access(keyPath: \.notify); return UserDefaults.standard.bool(forKey: "notify") }
        set { withMutation(keyPath: \.notify) { UserDefaults.standard.set(newValue, forKey: "notify") } }
    }

    @ObservationIgnored private var firstConnect: Task<Void, Never>?
    @ObservationIgnored var active = true

    init(arguments: [String] = ProcessInfo.processInfo.arguments) {
        demo = arguments.contains("-demo")
        screen = arguments.firstIndex(of: "-screen").flatMap { arguments.indices.contains($0 + 1) ? arguments[$0 + 1] : nil }
        voice = VoiceController(demo: demo)
        if demo {
            startDemo()
        } else if let saved = UserDefaults.standard.string(forKey: "selectedHost"), let url = HostAddress.parse(saved) {
            open(url, saving: false)
        }
    }

    // MARK: Hosts

    /// Checks `/health`, then opens the socket; the host is saved once its first snapshot arrives.
    func connect(_ address: String) async {
        guard let url = HostAddress.parse(address) else {
            connectError = "Enter the host's HTTPS address, like https://yard.example.ts.net, without a path or login."
            return
        }
        connectError = nil
        checking = true
        defer { checking = false }
        do {
            _ = try await HealthCheck.run(url)
        } catch {
            connectError = "Couldn't reach Switchyard there. Check the address, that Tailscale or your private network is on, and that the host is running."
            return
        }
        open(url, saving: true)
    }

    private func open(_ url: URL, saving: Bool) {
        yard?.stop()
        voice.turnOff()
        selected = nil
        let yard = Yard(host: url, transport: WebSocketTransport(host: url))
        yard.onConnected = { [weak self, weak yard] in
            guard let self, let yard, self.yard === yard else { return }
            self.firstConnect?.cancel()
            self.remember(url)
            self.showConnect = false
        }
        yard.onAttention = { [weak self] session in self?.attention(session) }
        self.yard = yard
        yard.start()
        guard saving else { return }
        firstConnect?.cancel()
        firstConnect = Task { [weak self, weak yard] in
            try? await Task.sleep(for: .seconds(20))
            guard let self, let yard, !Task.isCancelled, self.yard === yard, !yard.ready else { return }
            yard.stop()
            self.yard = nil
            self.connectError = "The host answered, but sessions didn't connect. Check that YARD_ALLOW_ORIGIN lists \(url.absoluteString) and that your proxy passes WebSockets."
        }
    }

    private func remember(_ url: URL) {
        let value = url.absoluteString
        recent = Array(([value] + recent.filter { $0 != value }).prefix(8))
        UserDefaults.standard.set(recent, forKey: "hosts")
        UserDefaults.standard.set(value, forKey: "selectedHost")
    }

    func forget(_ value: String) {
        recent.removeAll { $0 == value }
        UserDefaults.standard.set(recent, forKey: "hosts")
    }

    func disconnect() {
        voice.turnOff()
        yard?.stop()
        yard = nil
        selected = nil
        UserDefaults.standard.removeObject(forKey: "selectedHost")
    }

    func startDemo() {
        voice.turnOff()
        yard?.stop()
        let yard = Yard(host: URL(string: "https://demo.switchyard.local")!, transport: DemoTransport(), demo: true)
        self.yard = yard
        yard.start()
        guard let screen else { return }
        if screen.hasPrefix("session:") || screen.hasPrefix("voice:") {
            selected = String(screen.split(separator: ":", maxSplits: 1)[1])
        }
        if screen == "new" { showNew = true }
        if screen == "settings" { showSettings = true }
        if screen.hasPrefix("voice:") { voice.demo(.listening, heard: "Ship it once the tests pass, and tell me what changed") }
        if screen == "connect" { self.yard = nil }
    }

    // MARK: Lifecycle

    func scene(_ phase: ScenePhase) {
        active = phase == .active
        switch phase {
        case .active:
            yard?.resume()
        case .background:
            // Recording and speech stop in the background; sessions go on on the host.
            voice.turnOff()
        default: break
        }
    }

    // MARK: Notifications

    func setNotify(_ on: Bool) async {
        guard on else { notify = false; return }
        let granted = (try? await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge])) ?? false
        notify = granted
    }

    private func attention(_ session: SessionMeta) {
        guard notify, !(active && selected == session.id) else { return }
        let content = UNMutableNotificationContent()
        content.title = String(session.displayTitle.prefix(120))
        switch session.status {
        case .needsApproval: content.body = "Needs your approval."
        case .error: content.body = "Stopped with an error."
        default: content.body = session.lastTurn == "failed" ? "The turn failed." : "Finished."
        }
        content.sound = .default
        content.userInfo = ["host": yard?.host.absoluteString ?? "", "sessionId": session.id]
        UNUserNotificationCenter.current().add(UNNotificationRequest(identifier: session.id, content: content, trigger: nil))
    }

    func openFromNotification(_ info: [AnyHashable: Any]) {
        guard info["host"] as? String == yard?.host.absoluteString, let id = info["sessionId"] as? String, yard?.sessions[id] != nil else { return }
        showNew = false
        selected = id
    }
}
