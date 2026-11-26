import Foundation
import Observation

/// What a composer gives voice: its text, and sending it.
@MainActor
struct VoiceTarget {
    /// The session, or "" for the new-session page.
    var id: String
    var text: () -> String
    var setText: (String) -> Void
    var submit: () -> Void
}

/// Dictation and hands-free voice mode for whichever reply box is on screen.
/// One for the app, so voice mode started on the new-session page goes on in
/// the session it opens (as the web page carries it). Drives VoiceRules'
/// state machine with SpeechIO.
@MainActor
@Observable
final class VoiceController {
    private(set) var dictating = false
    private(set) var mode = VoiceRules.Mode.off
    var error: String?
    /// Microphone level, 0...1, for the waveform.
    private(set) var level: Float = 0
    /// What voice mode heard since it last sent, for the panel.
    private(set) var heard = ""

    @ObservationIgnored private let io: SpeechIO?
    @ObservationIgnored private var target: VoiceTarget?
    @ObservationIgnored private var want: Want = .none
    @ObservationIgnored private var base = ""
    /// Words of the current run's transcript already used (an echo, or typed over).
    @ObservationIgnored private var used = 0
    @ObservationIgnored private var transcript = ""
    @ObservationIgnored private var speakingText = ""
    @ObservationIgnored private var silence: Task<Void, Never>?
    @ObservationIgnored private var stopCheck: Task<Void, Never>?
    @ObservationIgnored private var restarts = 0
    @ObservationIgnored private var runStarted = Date()
    @ObservationIgnored private var lastCue: String?
    @ObservationIgnored private var detachTimer: Task<Void, Never>?
    @ObservationIgnored private var authorized = false
    /// A demo phase to show once a reply box attaches (screenshots).
    @ObservationIgnored private var demoPhase: (VoiceRules.Phase, String)?

    private enum Want { case none, dictate, voice }

    var voiceOn: Bool { mode.phase != .off }

    var rate: Float {
        get { Float(UserDefaults.standard.object(forKey: "voiceRate") as? Double ?? 1.0) }
        set { UserDefaults.standard.set(Double(newValue), forKey: "voiceRate") }
    }

    /// `demo` draws voice without touching audio (screenshots, the simulator demo).
    init(demo: Bool = false) {
        io = demo ? nil : SpeechIO()
        io?.onTranscript = { [weak self] text, final in self?.onTranscript(text, final: final) }
        io?.onListenEnded = { [weak self] problem in self?.onListenEnded(problem) }
        io?.onSpoken = { [weak self] in self?.dispatch(.spoken) }
        io?.onInterrupted = { [weak self] in self?.turnOff() }
        io?.onLevel = { [weak self] level in
            guard let self else { return }
            // Smoothed, so the bars breathe instead of flicker.
            self.level = self.level * 0.6 + level * 0.4
        }
    }

    // MARK: Targets

    /// A reply box appeared. `cueKey` is what the session would say now, so an
    /// old answer is not read when voice mode is turned on later.
    func attach(_ target: VoiceTarget, cueKey: String?) {
        detachTimer?.cancel()
        let carried = self.target?.id == "" && target.id != "" && mode.phase == .waiting
        if self.target?.id != target.id, !carried {
            if voiceOn || dictating { turnOff() }
            lastCue = cueKey
        }
        if carried { lastCue = nil }
        self.target = target
        if let shown = demoPhase {
            demoPhase = nil
            mode = VoiceRules.Mode(phase: shown.0, after: .listen)
            heard = shown.1
            level = 0.55
        }
    }

    /// A reply box went away. The new-session page hands voice mode on to
    /// the session it opened; leaving a session turns it off.
    func detach(_ id: String) {
        guard target?.id == id else { return }
        detachTimer?.cancel()
        let grace: Duration = id == "" && mode.phase == .waiting ? .seconds(30) : .milliseconds(400)
        detachTimer = Task { [weak self] in
            try? await Task.sleep(for: grace)
            guard let self, !Task.isCancelled, self.target?.id == id else { return }
            self.turnOff()
            self.target = nil
        }
    }

    /// What the session on screen has to say, numbered by its key.
    func observe(cueKey: String?, cue: VoiceRules.Cue?, for id: String) {
        guard target?.id == id, cueKey != lastCue else { return }
        lastCue = cueKey
        if let cue, voiceOn { dispatch(.cue(cue)) }
    }

    // MARK: Buttons

    func toggleDictation() {
        if dictating { stopDictation(); return }
        error = nil
        Task {
            guard await ready() else { return }
            if voiceOn { dispatch(.off) }
            want = .dictate
            restarts = 0
            dictating = true
            listen()
        }
    }

    func stopDictation() {
        if want == .dictate { want = .none }
        dictating = false
        io?.stopListening(abort: false)
        idleSoon()
    }

    func toggleVoice() {
        error = nil
        if voiceOn { dispatch(.off); return }
        Task {
            guard await ready() else { return }
            if dictating { stopDictation() }
            restarts = 0
            heard = ""
            dispatch(.on)
        }
    }

    func stopSpeaking() { dispatch(.stop) }

    func turnOff() {
        if voiceOn { dispatch(.off) }
        if dictating { stopDictation() }
        want = .none
        silence?.cancel()
        io?.shutdown()
        level = 0
    }

    /// The composer sent a message.
    func sent() {
        heard = ""
        if voiceOn { dispatch(.sent); return }
        if want == .dictate { want = .none }
        dictating = false
        io?.stopListening(abort: true)
        idleSoon()
    }

    /// The user typed: dictation carries on after their text.
    func typed(_ text: String) {
        base = text
        used = words(transcript).count
    }

    private func ready() async -> Bool {
        guard let _ = io else { return true }
        if authorized { return true }
        if let problem = await SpeechIO.authorize() {
            error = problem
            return false
        }
        authorized = true
        return true
    }

    // MARK: The state machine

    private func dispatch(_ input: VoiceRules.Input) {
        let step = VoiceRules.step(mode, input)
        mode = step.mode
        for effect in step.effects { run(effect) }
        if mode.phase == .off, !dictating { idleSoon() }
    }

    private func run(_ effect: VoiceRules.Effect) {
        switch effect {
        case .listen:
            want = .voice
            listen()
        case .unlisten:
            if want == .voice { want = .none }
            io?.stopListening(abort: true)
            transcript = ""
            used = 0
        case .speak(let text):
            speakingText = text
            if let io { io.speak(text, rate: rate) } else {
                Task { [weak self] in
                    try? await Task.sleep(for: .seconds(1))
                    self?.dispatch(.spoken)
                }
            }
        case .hush:
            speakingText = ""
            io?.hush()
        case .arm:
            silence?.cancel()
            silence = Task { [weak self] in
                try? await Task.sleep(for: VoiceRules.silence)
                guard let self, !Task.isCancelled else { return }
                let hasText = !(self.target?.text().trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ?? true)
                self.dispatch(.silence(hasText: hasText))
            }
        case .disarm:
            silence?.cancel()
            silence = nil
        case .send:
            target?.submit()
        }
    }

    private func listen() {
        guard let io, !io.isListening else { return }
        base = target?.text() ?? ""
        transcript = ""
        used = 0
        runStarted = Date()
        do { try io.startListening() } catch {
            fail((error as? VoiceProblem)?.message ?? "Voice input could not start.")
        }
    }

    private func fail(_ problem: String) {
        error = problem
        if want == .voice { dispatch(.failed) }
        want = .none
        dictating = false
        io?.stopListening(abort: true)
        idleSoon()
    }

    private func idleSoon() {
        Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(300))
            guard let self, self.mode.phase == .off, !self.dictating else { return }
            self.io?.idle()
            self.level = 0
        }
    }

    private func words(_ s: String) -> [Substring] { s.split(whereSeparator: \.isWhitespace) }

    private func onTranscript(_ text: String, final: Bool) {
        transcript = text
        let all = words(text)
        let spoken = all.dropFirst(min(used, all.count)).joined(separator: " ")
        guard !spoken.isEmpty else { return }
        if want == .voice, mode.phase == .speaking {
            if VoiceRules.isEcho(spoken, speaking: speakingText) {
                used = all.count
                return
            }
            if VoiceRules.isStopWord(spoken) {
                // Wait for the words to settle: "stop" may go on to be a real request.
                stopCheck?.cancel()
                let mark = all.count
                stopCheck = Task { [weak self] in
                    try? await Task.sleep(for: .milliseconds(final ? 0 : 900))
                    guard let self, !Task.isCancelled, self.words(self.transcript).count == mark, self.mode.phase == .speaking else { return }
                    self.used = mark
                    self.dispatch(.stop)
                }
                return
            }
            base = target?.text() ?? ""
            dispatch(.heard)
        } else if want == .voice {
            guard mode.phase == .listening else { return }
            dispatch(.heard)
        }
        if want == .voice { heard = spoken }
        target?.setText(VoiceRules.joinDictation(base, spoken))
    }

    private func onListenEnded(_ problem: String?) {
        if let problem { fail(problem); return }
        // A run ends on its own after a pause or a minute; keep listening for as long as wanted.
        guard want != .none else {
            // The last words of a stopped dictation are in: the microphone can go.
            dictating = false
            idleSoon()
            return
        }
        restarts = Date().timeIntervalSince(runStarted) < 1.5 ? restarts + 1 : 0
        if restarts > 5 { fail("Voice input keeps stopping. Try again."); return }
        // What the run heard is in the box now; the next run adds after it.
        Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(200))
            guard let self, self.want != .none else { return }
            if self.want == .voice, self.mode.phase != .listening, self.mode.phase != .speaking { return }
            self.listen()
        }
    }

    // MARK: Demo

    /// Shows voice mode in a phase without audio, for screenshots.
    func demo(_ phase: VoiceRules.Phase, heard: String) {
        demoPhase = (phase, heard)
    }
}
