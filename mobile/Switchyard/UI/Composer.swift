import SwiftUI
import PhotosUI
import UniformTypeIdentifiers

/// A file on its way to the host, then ready to go with the message.
struct UploadDraft: Identifiable, Equatable {
    let id = UUID()
    var name: String
    var state: UploadState

    enum UploadState: Equatable {
        case uploading
        case ready(Attachment)
        case failed(String)
    }

    var attachment: Attachment? { if case .ready(let a) = state { return a }; return nil }
}

/// The reply box: a floating glass bar. Voice mode turns it into a panel
/// that says what it is doing.
struct Composer: View {
    @Binding var text: String
    @Binding var uploads: [UploadDraft]
    var placeholder: String
    /// The session is working: an empty box offers Stop.
    var running = false
    var allowVoiceMode = true
    var onSend: () -> Void
    var onStop: (() -> Void)?
    var upload: (String, String, Data) async -> Result<Attachment, YardError>

    @Environment(VoiceController.self) private var voice
    @Namespace private var glass
    @FocusState private var focused: Bool
    @State private var photoItems: [PhotosPickerItem] = []
    @State private var showPhotos = false
    @State private var showFiles = false

    private var trimmed: String { text.trimmingCharacters(in: .whitespacesAndNewlines) }
    private var uploading: Bool { uploads.contains { $0.state == .uploading } }
    private var canSend: Bool { (!trimmed.isEmpty || uploads.contains { $0.attachment != nil }) && !uploading }

    var body: some View {
        GlassEffectContainer(spacing: 10) {
            VStack(spacing: 10) {
                if let error = voice.error {
                    VoiceErrorView(message: error) { voice.error = nil }
                }
                if !uploads.isEmpty { uploadChips }
                if voice.voiceOn {
                    VoicePanel()
                        .glassEffectID("bar", in: glass)
                        .transition(.blurReplace)
                } else {
                    inputRow
                }
            }
        }
        .padding(.horizontal, 12)
        .padding(.bottom, 8)
        .padding(.top, 6)
        .animation(.smooth(duration: 0.35), value: voice.voiceOn)
        .animation(.smooth(duration: 0.25), value: canSend)
        .animation(.smooth(duration: 0.25), value: voice.dictating)
        .photosPicker(isPresented: $showPhotos, selection: $photoItems, maxSelectionCount: 10, matching: .images)
        .fileImporter(isPresented: $showFiles, allowedContentTypes: [.item], allowsMultipleSelection: true) { result in
            if case .success(let urls) = result { for url in urls { attachFile(url) } }
        }
        .onChange(of: photoItems) { _, items in
            guard !items.isEmpty else { return }
            photoItems = []
            for (i, item) in items.enumerated() { attachPhoto(item, n: i + 1) }
        }
    }

    private var inputRow: some View {
        HStack(alignment: .bottom, spacing: 10) {
            Menu {
                Button("Photos", systemImage: "photo.on.rectangle") { showPhotos = true }
                Button("Files", systemImage: "folder") { showFiles = true }
            } label: {
                Image(systemName: "plus")
                    .font(.title3.weight(.medium))
                    .frame(width: 46, height: 46)
                    .contentShape(.circle)
            }
            .glassEffect(.regular.interactive(), in: .circle)
            .glassEffectID("plus", in: glass)
            .accessibilityLabel("Attach files")

            HStack(alignment: .bottom, spacing: 6) {
                TextField(placeholder, text: $text, axis: .vertical)
                    .lineLimit(1...7)
                    .focused($focused)
                    .padding(.vertical, 12)
                    .accessibilityLabel("Message")
                Button {
                    voice.toggleDictation()
                } label: {
                    Image(systemName: voice.dictating ? "stop.circle.fill" : "mic")
                        .font(.title3)
                        .foregroundStyle(voice.dictating ? Color.yardBad : .secondary)
                        .symbolEffect(.pulse, isActive: voice.dictating)
                        .contentTransition(.symbolEffect(.replace))
                        .frame(width: 34, height: 44)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(voice.dictating ? "Stop dictation" : "Dictate")
            }
            .padding(.leading, 16)
            .padding(.trailing, 6)
            .frame(minHeight: 46)
            .glassEffect(.regular.tint(voice.dictating ? Color.yardBad.opacity(0.12) : .clear).interactive(), in: .rect(cornerRadius: 23))
            .glassEffectID("bar", in: glass)

            action
                .glassEffectID("action", in: glass)
        }
    }

    @ViewBuilder private var action: some View {
        if canSend {
            round("arrow.up", .yardInfo, running ? "Send (queued until the agent's next step)" : "Send") { onSend() }
        } else if running, let onStop {
            round("stop.fill", .yardBad, "Stop") { onStop() }
        } else if allowVoiceMode {
            round("waveform", .yardInfo, "Voice mode") {
                focused = false
                voice.toggleVoice()
            }
        } else {
            round("arrow.up", .secondary, "Send") {}
                .disabled(true)
        }
    }

    /// A 46-point glass circle, as big as the + beside the box.
    private func round(_ symbol: String, _ color: Color, _ label: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 18, weight: .bold))
                .foregroundStyle(.white)
                .contentTransition(.symbolEffect(.replace))
                .frame(width: 46, height: 46)
                .contentShape(.circle)
        }
        .buttonStyle(.plain)
        .glassEffect(.regular.tint(color).interactive(), in: .circle)
        .accessibilityLabel(label)
    }

    private var uploadChips: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(uploads) { u in
                    HStack(spacing: 6) {
                        switch u.state {
                        case .uploading: ProgressView().controlSize(.mini)
                        case .ready(let a): Image(systemName: a.isImage ? "photo" : "doc").foregroundStyle(.secondary)
                        case .failed: Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(Color.yardBad)
                        }
                        Text(u.name).lineLimit(1).frame(maxWidth: 150)
                        if case .failed(let why) = u.state { Text(why).foregroundStyle(Color.yardBad).lineLimit(1) }
                        Button {
                            uploads.removeAll { $0.id == u.id }
                        } label: {
                            Image(systemName: "xmark.circle.fill").foregroundStyle(.secondary)
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel("Remove \(u.name)")
                    }
                    .font(.caption)
                    .padding(.horizontal, 10)
                    .padding(.vertical, 7)
                    .glassEffect(.regular, in: .capsule)
                }
            }
            .padding(.horizontal, 2)
        }
    }

    private func attachPhoto(_ item: PhotosPickerItem, n: Int) {
        let draft = UploadDraft(name: "Photo \(n).jpg", state: .uploading)
        uploads.append(draft)
        Task {
            guard let data = try? await item.loadTransferable(type: Data.self),
                  let jpeg = UIImage(data: data)?.jpegData(compressionQuality: 0.85) else {
                finish(draft.id, .failed("Couldn't read the photo."))
                return
            }
            finish(draft.id, await upload(draft.name, "image/jpeg", jpeg))
        }
    }

    private func attachFile(_ url: URL) {
        let draft = UploadDraft(name: url.lastPathComponent, state: .uploading)
        uploads.append(draft)
        Task {
            let scoped = url.startAccessingSecurityScopedResource()
            defer { if scoped { url.stopAccessingSecurityScopedResource() } }
            guard let data = try? Data(contentsOf: url) else {
                finish(draft.id, .failed("Couldn't read the file."))
                return
            }
            let mime = UTType(filenameExtension: url.pathExtension)?.preferredMIMEType ?? "application/octet-stream"
            finish(draft.id, await upload(draft.name, mime, data))
        }
    }

    private func finish(_ id: UUID, _ result: Result<Attachment, YardError>) {
        switch result {
        case .success(let a): finish(id, .ready(a))
        case .failure(let e): finish(id, .failed(e.message))
        }
    }

    private func finish(_ id: UUID, _ state: UploadDraft.UploadState) {
        guard let i = uploads.firstIndex(where: { $0.id == id }) else { return }
        uploads[i].state = state
    }
}

struct VoiceErrorView: View {
    let message: String
    let dismiss: () -> Void

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: "mic.slash.fill").foregroundStyle(Color.yardBad)
            Text(message).font(.footnote).frame(maxWidth: .infinity, alignment: .leading)
            if message.contains("Settings") {
                Button("Settings") {
                    if let url = URL(string: UIApplication.openSettingsURLString) { UIApplication.shared.open(url) }
                }
                .font(.footnote.weight(.semibold))
            }
            Button(action: dismiss) { Image(systemName: "xmark").font(.caption.weight(.bold)) }
                .buttonStyle(.plain)
                .accessibilityLabel("Dismiss")
        }
        .padding(12)
        .glassEffect(.regular.tint(Color.yardBad.opacity(0.1)), in: .rect(cornerRadius: 18))
    }
}

/// Voice mode's bar: what it is doing, what it heard, Stop and Turn off.
struct VoicePanel: View {
    @Environment(VoiceController.self) private var voice

    var body: some View {
        HStack(spacing: 14) {
            VoiceOrb(level: voice.level, phase: voice.mode.phase)
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.headline)
                    .contentTransition(.opacity)
                Text(detail)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
                    .contentTransition(.opacity)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityElement(children: .combine)
            if voice.mode.phase == .speaking {
                Button {
                    voice.stopSpeaking()
                } label: {
                    Image(systemName: "stop.fill").frame(width: 22, height: 22)
                }
                .buttonStyle(.glass)
                .buttonBorderShape(.circle)
                .controlSize(.large)
                .accessibilityLabel("Stop reading aloud")
            }
            Button {
                voice.toggleVoice()
            } label: {
                Image(systemName: "xmark").font(.body.weight(.bold)).frame(width: 22, height: 22)
            }
            .buttonStyle(.glassProminent)
            .buttonBorderShape(.circle)
            .controlSize(.large)
            .tint(Color.primary.opacity(0.85))
            .accessibilityLabel("Turn off voice mode")
        }
        .padding(.leading, 12)
        .padding(.trailing, 10)
        .padding(.vertical, 10)
        .glassEffect(.regular.tint(tint.opacity(0.14)).interactive(), in: .rect(cornerRadius: 32))
        .animation(.smooth, value: voice.mode.phase)
    }

    private var tint: Color {
        switch voice.mode.phase {
        case .listening: .yardInfo
        case .speaking: .yardMerged
        case .waiting: .yardWarn
        case .off: .clear
        }
    }

    private var title: String {
        switch voice.mode.phase {
        case .listening: voice.heard.isEmpty ? "Listening" : "Listening…"
        case .speaking: "Reading aloud"
        case .waiting: voice.mode.after == .wait ? "Waiting for your approval" : "Working on it"
        case .off: ""
        }
    }

    private var detail: String {
        switch voice.mode.phase {
        case .listening: voice.heard.isEmpty ? "Speak; it sends when you pause." : voice.heard
        case .speaking: "Talk over it, or say “stop”."
        case .waiting: "The mic is off until the agent answers."
        case .off: ""
        }
    }
}

/// A glowing orb whose bars follow the microphone, or the voice while it reads.
struct VoiceOrb: View {
    let level: Float
    let phase: VoiceRules.Phase

    var body: some View {
        TimelineView(.animation(minimumInterval: 1 / 30, paused: phase == .off)) { context in
            let t = context.date.timeIntervalSinceReferenceDate
            ZStack {
                Circle()
                    .fill(gradient)
                    .frame(width: 48, height: 48)
                    .shadow(color: color.opacity(0.45), radius: 10 + CGFloat(level) * 10)
                HStack(spacing: 3) {
                    ForEach(0..<5, id: \.self) { i in
                        Capsule()
                            .fill(.white)
                            .frame(width: 4, height: barHeight(i, t))
                    }
                }
            }
        }
        .frame(width: 52, height: 52)
        .accessibilityHidden(true)
    }

    private var color: Color {
        switch phase {
        case .listening: .yardInfo
        case .speaking: .yardMerged
        case .waiting: .yardWarn
        case .off: .gray
        }
    }

    private var gradient: some ShapeStyle {
        LinearGradient(colors: [color, color.opacity(0.7)], startPoint: .topLeading, endPoint: .bottomTrailing)
    }

    private func barHeight(_ i: Int, _ t: TimeInterval) -> CGFloat {
        let wave = (sin(t * 6 + Double(i) * 0.9) + 1) / 2
        switch phase {
        case .listening:
            let l = Double(max(0.08, level))
            return 6 + CGFloat(l * (10 + wave * 14))
        case .speaking:
            return 7 + CGFloat(wave * 15)
        case .waiting:
            let dot = (sin(t * 3 - Double(i) * 0.8) + 1) / 2
            return 4 + CGFloat(dot * 4)
        case .off:
            return 4
        }
    }
}
