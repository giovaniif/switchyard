import Foundation

enum HostAddress {
    static func parse(_ value: String) -> URL? {
        guard let c = URLComponents(string: value.trimmingCharacters(in: .whitespacesAndNewlines)),
              c.scheme?.lowercased() == "https", let host = c.host?.lowercased(), !host.isEmpty,
              c.user == nil, c.password == nil, c.query == nil, c.fragment == nil,
              c.path.isEmpty || c.path == "/",
              host != "localhost", !host.hasSuffix(".localhost"), host != "::1", host != "[::1]",
              host != "0.0.0.0", !host.hasPrefix("127.") else { return nil }
        var origin = c
        origin.path = ""
        return origin.url
    }

    static func fromLink(_ url: URL) -> URL? {
        guard url.scheme == "switchyard", url.host == "connect",
              let c = URLComponents(url: url, resolvingAgainstBaseURL: false),
              let value = c.queryItems?.first(where: { $0.name == "host" })?.value else { return nil }
        return parse(value)
    }

    static func sameOrigin(_ a: URL, _ b: URL) -> Bool {
        a.scheme?.lowercased() == b.scheme?.lowercased() && a.host?.lowercased() == b.host?.lowercased() && (a.port ?? 443) == (b.port ?? 443)
    }
}
