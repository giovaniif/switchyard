import SwiftUI
import UserNotifications

@main
struct SwitchyardApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    @State private var app = AppModel()
    @Environment(\.scenePhase) private var phase

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(app)
                .environment(app.voice)
                .onAppear { delegate.app = app }
                .onChange(of: phase) { _, phase in app.scene(phase) }
                .onOpenURL { url in
                    // A link fills the address; connecting stays a choice made on the phone.
                    guard let host = HostAddress.fromLink(url) else { return }
                    app.linkedAddress = host.absoluteString
                    app.showConnect = true
                }
                .tint(.yardInfo)
        }
    }
}

final class AppDelegate: NSObject, UIApplicationDelegate, UNUserNotificationCenterDelegate {
    weak var app: AppModel?

    func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
        UNUserNotificationCenter.current().delegate = self
        return true
    }

    func userNotificationCenter(_ center: UNUserNotificationCenter, willPresent notification: UNNotification) async -> UNNotificationPresentationOptions {
        [.banner, .sound]
    }

    func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse) async {
        let info = response.notification.request.content.userInfo
        await MainActor.run { app?.openFromNotification(info) }
    }
}
