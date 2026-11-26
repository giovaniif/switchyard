import SwiftUI

extension Color {
    /// The web UI's tokens (ui/src/styles.css), so both feel like one product.
    static let yardInfo = Color(red: 0.184, green: 0.435, blue: 0.929)
    static let yardOk = Color(red: 0.122, green: 0.616, blue: 0.333)
    static let yardWarn = Color(red: 0.851, green: 0.604, blue: 0.0)
    static let yardBad = Color(red: 0.706, green: 0.137, blue: 0.094)
    static let yardMerged = Color(red: 0.478, green: 0.247, blue: 0.82)
}

extension Engine {
    /// Each engine's colour, as its maker uses it.
    var tint: Color {
        switch self {
        case .claude: Color(red: 0.85, green: 0.47, blue: 0.34)
        case .codex: Color(red: 0.2, green: 0.2, blue: 0.22)
        case .opencode: Color(red: 0.36, green: 0.4, blue: 0.47)
        case .grok: Color(red: 0.1, green: 0.1, blue: 0.1)
        case .gemini: Color(red: 0.26, green: 0.52, blue: 0.96)
        }
    }

    var symbol: String {
        switch self {
        case .claude: "asterisk"
        case .codex: "chevron.left.forwardslash.chevron.right"
        case .opencode: "curlybraces"
        case .grok: "xmark"
        case .gemini: "sparkle"
        }
    }
}

func agentColor(_ name: String) -> Color {
    switch name {
    case "green": Color(red: 0.122, green: 0.541, blue: 0.298)
    case "amber": Color(red: 0.718, green: 0.475, blue: 0.122)
    case "rose": Color(red: 0.761, green: 0.255, blue: 0.42)
    case "violet": .yardMerged
    case "teal": Color(red: 0.059, green: 0.545, blue: 0.553)
    case "orange": Color(red: 0.761, green: 0.337, blue: 0.047)
    case "slate": Color(red: 0.333, green: 0.376, blue: 0.439)
    default: .yardInfo
    }
}

/// A rounded square with the engine's mark.
struct EngineBadge: View {
    let engine: Engine
    var size: CGFloat = 36

    var body: some View {
        RoundedRectangle(cornerRadius: size * 0.3, style: .continuous)
            .fill(engine.tint.gradient)
            .frame(width: size, height: size)
            .overlay {
                Image(systemName: engine.symbol)
                    .font(.system(size: size * 0.42, weight: .bold))
                    .foregroundStyle(.white)
            }
            .accessibilityHidden(true)
    }
}

/// Where a session is: a breathing dot while it works, a hand when it waits for you.
struct StatusMark: View {
    let session: SessionMeta

    var body: some View {
        Group {
            switch session.status {
            case .running:
                PulsingDot(color: .yardInfo)
            case .queued:
                Image(systemName: "hourglass").foregroundStyle(.secondary)
            case .needsApproval:
                Image(systemName: "hand.raised.fill").foregroundStyle(Color.yardWarn)
                    .symbolEffect(.pulse)
            case .error:
                Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(Color.yardBad)
            case .idle:
                if session.lastTurn == "failed" {
                    Image(systemName: "exclamationmark.circle").foregroundStyle(Color.yardBad)
                } else {
                    EmptyView()
                }
            }
        }
        .font(.footnote.weight(.semibold))
        .accessibilityLabel(session.status.label)
    }
}

struct PulsingDot: View {
    var color: Color
    @State private var on = false

    var body: some View {
        Circle()
            .fill(color)
            .frame(width: 8, height: 8)
            .background {
                Circle().fill(color.opacity(0.35))
                    .scaleEffect(on ? 2.4 : 1)
                    .opacity(on ? 0 : 1)
            }
            .onAppear {
                withAnimation(.easeOut(duration: 1.3).repeatForever(autoreverses: false)) { on = true }
            }
    }
}

/// "3 m", "2 h", "Yesterday": short, for list rows.
func shortAge(_ date: Date, now: Date = .now) -> String {
    let s = now.timeIntervalSince(date)
    if s < 60 { return "now" }
    if s < 3600 { return "\(Int(s / 60))m" }
    if s < 86_400 { return "\(Int(s / 3600))h" }
    if s < 7 * 86_400 { return "\(Int(s / 86_400))d" }
    return date.formatted(.dateTime.month(.abbreviated).day())
}

/// "Worked for 1m 12s".
func duration(_ ms: Double) -> String {
    let s = Int((ms / 1000).rounded())
    if s < 60 { return "\(max(1, s))s" }
    let m = s / 60
    if m < 60 { return s % 60 == 0 ? "\(m)m" : "\(m)m \(s % 60)s" }
    return "\(m / 60)h \(m % 60)m"
}

/// A path with the home folder as ~.
func tildePath(_ path: String, home: String?) -> String {
    guard let home, !home.isEmpty, path.hasPrefix(home) else { return path }
    return "~" + path.dropFirst(home.count)
}

/// A soft, slowly drifting backdrop that gives the glass something to refract.
struct AuroraBackground: View {
    var body: some View {
        TimelineView(.animation(minimumInterval: 1 / 20)) { context in
            MeshGradient(width: 3, height: 3, points: Self.points(context.date), colors: Self.colors)
        }
        .ignoresSafeArea()
    }

    private static let colors: [Color] = [
        .yardInfo.opacity(0.55), .teal.opacity(0.35), .indigo.opacity(0.5),
        .mint.opacity(0.3), .white.opacity(0.1), .yardMerged.opacity(0.35),
        .orange.opacity(0.35), .pink.opacity(0.3), .yardInfo.opacity(0.45),
    ]

    private static func points(_ date: Date) -> [SIMD2<Float>] {
        let t = Float(date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: 600))
        let a: Float = 0.5 + 0.12 * sin(t * 0.21)
        let b: Float = 0.5 + 0.1 * cos(t * 0.17)
        let cx: Float = 0.5 + 0.15 * cos(t * 0.13)
        let cy: Float = 0.5 + 0.12 * sin(t * 0.19)
        let d: Float = 0.5 + 0.1 * sin(t * 0.23)
        let e: Float = 0.5 + 0.12 * cos(t * 0.11)
        return [
            SIMD2(0, 0), SIMD2(a, 0), SIMD2(1, 0),
            SIMD2(0, b), SIMD2(cx, cy), SIMD2(1, d),
            SIMD2(0, 1), SIMD2(e, 1), SIMD2(1, 1),
        ]
    }
}
