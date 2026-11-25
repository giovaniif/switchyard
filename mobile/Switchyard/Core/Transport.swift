import Foundation

/// How the store reaches a yardd: its WebSocket, or the bundled demo.
@MainActor
protocol YardTransport: AnyObject {
    var onMessage: ((JSON) -> Void)? { get set }
    /// The socket closed or failed; nil when it opened.
    var onClosed: ((String?) -> Void)? { get set }
    var onOpen: (() -> Void)? { get set }
    func start()
    func stop()
    func send(_ msg: JSON)
}

/// yardd's `/ws`, as the web page opens it. yardd allows a client with no
/// Origin (a native one) or an allowed one; this sends the host's own origin,
/// which `YARD_ALLOW_ORIGIN` already lists for its web page.
@MainActor
final class WebSocketTransport: NSObject, YardTransport, URLSessionWebSocketDelegate {
    var onMessage: ((JSON) -> Void)?
    var onClosed: ((String?) -> Void)?
    var onOpen: (() -> Void)?

    private let host: URL
    private var session: URLSession?
    private var task: URLSessionWebSocketTask?
    private var pinger: Task<Void, Never>?
    private var generation = 0

    init(host: URL) { self.host = host }

    func start() {
        stop()
        generation += 1
        let gen = generation
        var components = URLComponents(url: host, resolvingAgainstBaseURL: false)!
        components.scheme = "wss"
        components.path = "/ws"
        var request = URLRequest(url: components.url!)
        request.setValue(origin, forHTTPHeaderField: "Origin")
        request.timeoutInterval = 20
        let session = URLSession(configuration: .default, delegate: self, delegateQueue: .main)
        self.session = session
        let task = session.webSocketTask(with: request)
        // A big snapshot or history can exceed the 1 MiB default.
        task.maximumMessageSize = 64 * 1024 * 1024
        self.task = task
        task.resume()
        receive(task, gen)
    }

    private var origin: String {
        let port = host.port.map { ":\($0)" } ?? ""
        return "https://\(host.host ?? "")\(port)"
    }

    private func receive(_ task: URLSessionWebSocketTask, _ gen: Int) {
        task.receive { [weak self] result in
            Task { @MainActor in
                guard let self, self.generation == gen else { return }
                switch result {
                case .success(let message):
                    let json: JSON?
                    switch message {
                    case .string(let text): json = JSON.parse(text)
                    case .data(let data): json = JSON.parse(data)
                    @unknown default: json = nil
                    }
                    if let json { self.onMessage?(json) }
                    self.receive(task, gen)
                case .failure(let error):
                    self.closed(Self.describe(error), gen)
                }
            }
        }
    }

    private func closed(_ reason: String?, _ gen: Int) {
        guard generation == gen else { return }
        generation += 1
        pinger?.cancel()
        task?.cancel(with: .goingAway, reason: nil)
        task = nil
        session?.invalidateAndCancel()
        session = nil
        onClosed?(reason ?? "The connection closed.")
    }

    func stop() {
        generation += 1
        pinger?.cancel()
        pinger = nil
        task?.cancel(with: .goingAway, reason: nil)
        task = nil
        session?.invalidateAndCancel()
        session = nil
    }

    func send(_ msg: JSON) {
        task?.send(.string(msg.encoded())) { _ in }
    }

    nonisolated func urlSession(_ session: URLSession, webSocketTask: URLSessionWebSocketTask, didOpenWithProtocol protocol: String?) {
        MainActor.assumeIsolated {
            guard webSocketTask === task else { return }
            let gen = generation
            onOpen?()
            // A phone can lose its network without the socket noticing; a ping finds out.
            pinger = Task { [weak self] in
                while !Task.isCancelled {
                    try? await Task.sleep(for: .seconds(20))
                    guard let self, self.generation == gen, let task = self.task else { return }
                    task.sendPing { error in
                        guard let error else { return }
                        Task { @MainActor [weak self] in self?.closed(Self.describe(error), gen) }
                    }
                }
            }
        }
    }

    nonisolated func urlSession(_ session: URLSession, webSocketTask: URLSessionWebSocketTask, didCloseWith closeCode: URLSessionWebSocketTask.CloseCode, reason: Data?) {
        MainActor.assumeIsolated {
            guard webSocketTask === task else { return }
            closed("The host closed the connection.", generation)
        }
    }

    nonisolated func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        MainActor.assumeIsolated {
            guard task === self.task else { return }
            let status = (task.response as? HTTPURLResponse)?.statusCode
            if status == 403 {
                closed("The host refused this app's connection (403). Add \(origin) to YARD_ALLOW_ORIGIN on the host.", generation)
            } else if let error {
                closed(Self.describe(error), generation)
            }
        }
    }

    nonisolated static func describe(_ error: Error) -> String {
        let ns = error as NSError
        guard ns.domain == NSURLErrorDomain else { return error.localizedDescription }
        switch ns.code {
        case NSURLErrorNotConnectedToInternet: return "You're offline."
        case NSURLErrorCannotFindHost, NSURLErrorCannotConnectToHost, NSURLErrorTimedOut:
            return "Can't reach the host. Check Tailscale or your private network."
        case NSURLErrorServerCertificateUntrusted, NSURLErrorServerCertificateHasBadDate, NSURLErrorSecureConnectionFailed:
            return "The host's HTTPS certificate isn't trusted."
        case NSURLErrorNetworkConnectionLost: return "The connection dropped."
        default: return error.localizedDescription
        }
    }
}

/// Checks that an address is a yardd before saving it: `/health` answers `yardd <version>`.
enum HealthCheck {
    static func run(_ host: URL) async throws -> String {
        var request = URLRequest(url: host.appendingPathComponent("health"))
        request.timeoutInterval = 15
        request.cachePolicy = .reloadIgnoringLocalCacheData
        let (data, response) = try await URLSession.shared.data(for: request)
        guard let response = response as? HTTPURLResponse, response.statusCode == 200,
              let finalURL = response.url, HostAddress.sameOrigin(host, finalURL),
              let text = String(data: data, encoding: .utf8), text.hasPrefix("yardd ") else {
            throw URLError(.badServerResponse)
        }
        return String(text.dropFirst(6)).trimmingCharacters(in: .whitespacesAndNewlines)
    }
}
