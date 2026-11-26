import SwiftUI
import WebKit

/// The host's own web page for one session: its terminals, changes, pull
/// request and processes, which the native screens do not redo.
struct WorkspaceView: View {
    let host: URL
    let sessionId: String
    let title: String
    @Environment(\.dismiss) private var dismiss
    @State private var loading = true
    @State private var failed: String?

    var body: some View {
        NavigationStack {
            HostPage(url: pageURL, loading: $loading, failed: $failed)
                .ignoresSafeArea(edges: .bottom)
                .overlay {
                    if let failed {
                        ContentUnavailableView("Couldn't open the page", systemImage: "wifi.exclamationmark", description: Text(failed))
                    } else if loading {
                        ProgressView()
                    }
                }
                .navigationTitle(title)
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Done", systemImage: "checkmark") { dismiss() }
                    }
                }
        }
    }

    private var pageURL: URL {
        var c = URLComponents(url: host, resolvingAgainstBaseURL: false)!
        c.path = "/"
        c.fragment = "/s/\(sessionId.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? sessionId)"
        return c.url!
    }
}

/// A web view that stays on the host's origin; other links open in Safari.
struct HostPage: UIViewRepresentable {
    let url: URL
    @Binding var loading: Bool
    @Binding var failed: String?

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeUIView(context: Context) -> WKWebView {
        let web = WKWebView(frame: .zero, configuration: WKWebViewConfiguration())
        web.navigationDelegate = context.coordinator
        web.allowsBackForwardNavigationGestures = false
        web.isInspectable = false
        web.load(URLRequest(url: url))
        return web
    }

    func updateUIView(_ web: WKWebView, context: Context) {}

    static func dismantleUIView(_ web: WKWebView, coordinator: Coordinator) {
        web.stopLoading()
        web.loadHTMLString("", baseURL: nil)
    }

    final class Coordinator: NSObject, WKNavigationDelegate {
        let parent: HostPage
        init(_ parent: HostPage) { self.parent = parent }

        func webView(_ webView: WKWebView, decidePolicyFor action: WKNavigationAction) async -> WKNavigationActionPolicy {
            guard let url = action.request.url else { return .cancel }
            // Connector widgets load in their own frames.
            if action.targetFrame?.isMainFrame == false { return .allow }
            if HostAddress.sameOrigin(parent.url, url), action.targetFrame != nil { return .allow }
            if ["https", "http", "mailto"].contains(url.scheme?.lowercased() ?? "") { _ = await UIApplication.shared.open(url) }
            return .cancel
        }

        func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
            parent.loading = false
        }

        func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) {
            parent.loading = false
            if (error as NSError).code != NSURLErrorCancelled { parent.failed = WebSocketTransport.describe(error) }
        }
    }
}
